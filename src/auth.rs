//! Authentication flows: QR code, account password and cookie import.
//!
//! The QR protocol follows the current passport endpoints:
//! `getqrcode` -> long-poll `channel/unicast` -> `v3/login/main/qrbdusslogin`.
//! Password login uses the RSA-encrypted v3 form; Baidu's anti-bot layer
//! frequently rejects third-party password logins, so QR and cookie import are
//! the reliable paths.

use std::collections::BTreeMap;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use regex::Regex;
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, COOKIE, REFERER, SET_COOKIE};
use rsa::RsaPublicKey;
use rsa::pkcs8::DecodePublicKey;
use serde_json::Value;

use crate::api::ApiClient;
use crate::events::{Event, QrState};
use crate::models::{Quota, SessionFile, UA, UserInfo};

const PASSPORT: &str = "https://passport.baidu.com";
const PAN_REFERER: &str = "https://pan.baidu.com/";

struct Guard(Sender<Event>);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.send(Event::JobDone);
    }
}

#[derive(Default, Clone)]
struct Jar {
    map: BTreeMap<String, String>,
}

impl Jar {
    fn header(&self) -> String {
        self.map
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(String::as_str)
    }

    fn insert(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        if !value.is_empty() {
            self.map.insert(name.to_owned(), value);
        }
    }

    fn absorb(&mut self, response: &Response) {
        for value in response.headers().get_all(SET_COOKIE).iter() {
            let Ok(text) = value.to_str() else {
                continue;
            };
            let first = text.split(';').next().unwrap_or("");
            if let Some((name, value)) = first.split_once('=') {
                let name = name.trim();
                let value = value.trim().trim_matches('"');
                if !name.is_empty() && !value.is_empty() {
                    self.insert(name, value);
                }
            }
        }
    }
}

fn finish_login(client: ApiClient, tx: &Sender<Event>) {
    let mut client = client;
    let result = (|| {
        let user: UserInfo = client.whoami().map_err(|error| error.to_string())?;
        client.set_uk(user.uk);
        let quota: Quota = client.quota().map_err(|error| error.to_string())?;
        let session: SessionFile = client.session_file(&user);
        Ok::<_, String>((session, user, quota))
    })();
    match result {
        Ok((session, user, quota)) => {
            let _ = tx.send(Event::LoginOk {
                session,
                user,
                quota,
            });
        }
        Err(error) => {
            let _ = tx.send(Event::LoginFailed {
                message: format!("登录成功但无法获取账号信息：{error}"),
                captcha: None,
            });
        }
    }
}

pub fn login_with_cookie(tx: Sender<Event>, guard_tx: Sender<Event>, cookie: String) {
    std::thread::spawn(move || {
        let _guard = Guard(guard_tx);
        match ApiClient::new(&cookie) {
            Ok(client) => finish_login(client, &tx),
            Err(error) => {
                let _ = tx.send(Event::LoginFailed {
                    message: error.to_string(),
                    captcha: None,
                });
            }
        }
    });
}

/// Starts the QR login flow. The thread exits when the code is confirmed or
/// expires; the app can start another one by refreshing.
pub fn start_qr_login(
    tx: Sender<Event>,
    guard_tx: Sender<Event>,
    _saved_cookie: Option<String>,
) {
    std::thread::spawn(move || {
        let _guard = Guard(guard_tx);
        if let Err(error) = qr_flow(&tx) {
            let _ = tx.send(Event::LoginQrStatus {
                state: QrState::Failed,
                message: error.clone(),
            });
            let _ = tx.send(Event::LoginFailed {
                message: error,
                captcha: None,
            });
        }
    });
}

pub fn login_with_password(
    tx: Sender<Event>,
    guard_tx: Sender<Event>,
    account: String,
    password: String,
    vcode: String,
) {
    std::thread::spawn(move || {
        let _guard = Guard(guard_tx);
        if let Err(error) = password_flow(&tx, &account, &password, &vcode) {
            let _ = tx.send(Event::LoginFailed {
                message: error,
                captcha: None,
            });
        }
    });
}

fn qr_flow(tx: &Sender<Event>) -> Result<(), String> {
    let client = http_client()?;
    let mut jar = Jar::default();
    let gid = uuid::Uuid::new_v4().to_string().to_uppercase();
    let tt = now_ms();
    let callback = "bd__pcbs__qr";
    let url = format!(
        "{PASSPORT}/v2/api/getqrcode?lp=pc&qrloginfrom=pc&gid={gid}&tpl=netdisk&apiver=v3&tt={tt}&callback={callback}"
    );
    let response = passport_get(&client, &mut jar, &url, PAN_REFERER)?;
    let text = response
        .text()
        .map_err(|error| format!("读取二维码响应失败：{error}"))?;
    let json = parse_jsonp(&text)?;
    let errno = json.get("errno").and_then(Value::as_i64).unwrap_or(-1);
    if errno != 0 {
        return Err(format!("获取二维码失败（errno={errno}）"));
    }
    let sign = json
        .get("sign")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let imgurl = json
        .get("imgurl")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if sign.is_empty() {
        return Err("二维码响应缺少 sign".to_owned());
    }
    let img_url = if imgurl.starts_with("http") {
        imgurl
    } else {
        format!("https://{imgurl}")
    };
    let image_response = passport_get(&client, &mut jar, &img_url, PAN_REFERER)?;
    let bytes = image_response
        .bytes()
        .map_err(|error| format!("下载二维码失败：{error}"))?;
    let image = crate::media::decode_image(&bytes, 512)
        .ok_or_else(|| "二维码图片解码失败".to_owned())?;
    let _ = tx.send(Event::LoginQr {
        image,
        message: "请使用百度网盘 App 扫描二维码".to_owned(),
    });

    let started = Instant::now();
    let mut scanned = false;
    let mut failures = 0u32;
    let confirmed = loop {
        if started.elapsed() > Duration::from_secs(590) {
            let _ = tx.send(Event::LoginQrStatus {
                state: QrState::Expired,
                message: "二维码已过期，请点击刷新".to_owned(),
            });
            return Ok(());
        }
        let tt = now_ms();
        let url = format!(
            "{PASSPORT}/channel/unicast?channel_id={sign}&gid={gid}&tpl=netdisk&_sdkFrom=1&apiver=v3&tt={tt}&callback={callback}"
        );
        let response = match passport_get(&client, &mut jar, &url, PAN_REFERER) {
            Ok(response) => response,
            Err(_) => {
                failures += 1;
                if failures >= 6 {
                    return Err("二维码状态轮询失败，请检查网络".to_owned());
                }
                std::thread::sleep(Duration::from_millis(1500));
                continue;
            }
        };
        let text = response.text().unwrap_or_default();
        let Ok(json) = parse_jsonp(&text) else {
            failures += 1;
            if failures >= 6 {
                return Err("二维码状态响应异常".to_owned());
            }
            std::thread::sleep(Duration::from_millis(800));
            continue;
        };
        failures = 0;
        let errno = json.get("errno").and_then(Value::as_i64).unwrap_or(-1);
        if errno != 0 {
            if errno == -1 {
                let _ = tx.send(Event::LoginQrStatus {
                    state: QrState::Expired,
                    message: "二维码已失效，请刷新".to_owned(),
                });
                return Ok(());
            }
            continue;
        }
        let channel = json
            .get("channel_v")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Ok(inner) = serde_json::from_str::<Value>(channel) else {
            continue;
        };
        let status = inner.get("status").and_then(Value::as_i64).unwrap_or(-1);
        match status {
            1 => {
                if !scanned {
                    scanned = true;
                    let _ = tx.send(Event::LoginQrStatus {
                        state: QrState::Scanned,
                        message: "二维码已扫描，请在手机上确认登录".to_owned(),
                    });
                }
            }
            0 => {
                let v = inner
                    .get("v")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let u = inner
                    .get("u")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                if v.is_empty() {
                    return Err("手机确认信息不完整".to_owned());
                }
                break (v, u);
            }
            2 => {
                let _ = tx.send(Event::LoginQrStatus {
                    state: QrState::Expired,
                    message: "登录已取消，请刷新二维码".to_owned(),
                });
                return Ok(());
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(400));
    };
    let (v, u) = confirmed;
    let _ = tx.send(Event::LoginQrStatus {
        state: QrState::Confirmed,
        message: "已确认，正在登录…".to_owned(),
    });
    let tt = now_ms();
    let url = format!(
        "{PASSPORT}/v3/login/main/qrbdusslogin?v={tt}&bduss={}&u={}&loginVersion=v4&qrcode=1&tpl=netdisk&apiver=v3&tt={tt}&callback={callback}",
        urlencoding::encode(&v),
        urlencoding::encode(&u),
    );
    let response = passport_get(&client, &mut jar, &url, PAN_REFERER)?;
    let text = response.text().unwrap_or_default();
    let json = parse_jsonp(&text).unwrap_or(Value::Null);
    let err_info = json.pointer("/errInfo/no").and_then(Value::as_str);
    if let Some(no) = err_info
        && no != "0"
    {
        let message = json
            .pointer("/errInfo/errMsg")
            .and_then(Value::as_str)
            .unwrap_or("二维码登录失败");
        return Err(format!("{message}（{no}）"));
    }
    if let Some(session) = json.pointer("/data/session") {
        if jar.get("BDUSS").is_none()
            && let Some(bduss) = session.get("bduss").and_then(Value::as_str)
        {
            jar.insert("BDUSS", bduss);
        }
        if jar.get("STOKEN").is_none()
            && let Some(stoken) = session.get("stoken").and_then(Value::as_str)
        {
            jar.insert("STOKEN", stoken);
        }
        if jar.get("PTOKEN").is_none()
            && let Some(ptoken) = session.get("ptoken").and_then(Value::as_str)
        {
            jar.insert("PTOKEN", ptoken);
        }
    }
    let Some(bduss) = jar.get("BDUSS").map(str::to_owned) else {
        return Err("二维码登录成功，但没有获得 BDUSS".to_owned());
    };
    jar.insert("BDUSS", bduss);
    jar.insert("PANWEB", "1");
    let cookie = jar.header();
    let client = ApiClient::new(&cookie).map_err(|error| error.to_string())?;
    finish_login(client, tx);
    Ok(())
}

fn password_flow(
    tx: &Sender<Event>,
    account: &str,
    password: &str,
    vcode: &str,
) -> Result<(), String> {
    let client = http_client()?;
    let mut jar = Jar::default();
    let gid = uuid::Uuid::new_v4().to_string().to_uppercase();
    let tt = now_ms();
    // Bootstrap BAIDUID; without it getapi/token refuses to issue a token.
    let url = format!("{PASSPORT}/v2/?login&tpl=netdisk&apiver=v3&tt={tt}");
    let _ = passport_get(&client, &mut jar, &url, PAN_REFERER)?;

    let tt = now_ms();
    let url = format!(
        "{PASSPORT}/v2/api/?getapi&tpl=netdisk&subpro=netdisk_web&apiver=v3&tt={tt}&class=login&gid={gid}&logintype=basicLogin&callback=bd__pcbs__api"
    );
    let response = passport_get(&client, &mut jar, &url, PAN_REFERER)?;
    let text = response.text().unwrap_or_default();
    let json = parse_jsonp(&text)?;
    let token = json
        .pointer("/data/token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if token.is_empty() {
        let message = json
            .pointer("/data/errmsg")
            .and_then(Value::as_str)
            .unwrap_or("无法获取登录 token");
        return Err(format!("百度 Passport：{message}"));
    }

    let is_phone = account
        .chars()
        .all(|ch| ch.is_ascii_digit())
        && account.len() == 11;
    let tt = now_ms();
    let url = format!(
        "{PASSPORT}/v2/api/?logincheck&token={token}&tpl=netdisk&subpro=netdisk_web&apiver=v3&tt={tt}&username={}&isphone={}&callback=bd__pcbs__check",
        urlencoding::encode(account),
        if is_phone { 1 } else { 0 },
    );
    let check_text = passport_get(&client, &mut jar, &url, PAN_REFERER)?
        .text()
        .unwrap_or_default();
    if let Ok(check) = parse_jsonp(&check_text)
        && let Some(code_string) = check
            .pointer("/data/codeString")
            .or_else(|| check.pointer("/codeString"))
            .and_then(Value::as_str)
        && !code_string.is_empty()
        && vcode.is_empty()
    {
        let captcha = fetch_captcha(&client, &mut jar, code_string).ok();
        let _ = tx.send(Event::LoginFailed {
            message: "账号登录需要图形验证码，请输入后重试".to_owned(),
            captcha,
        });
        return Ok(());
    }

    let tt = now_ms();
    let url = format!(
        "{PASSPORT}/v2/getpublickey?token={token}&tpl=netdisk&apiver=v3&tt={tt}&gid={gid}&subpro=netdisk_web&callback=bd__pcbs__key"
    );
    let key_text = passport_get(&client, &mut jar, &url, PAN_REFERER)?
        .text()
        .unwrap_or_default();
    let key_json = parse_jsonp(&key_text)?;
    let pubkey = key_json
        .get("pubkey")
        .and_then(Value::as_str)
        .unwrap_or("")
        .replace("\\n", "\n");
    let key = key_json
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if pubkey.is_empty() || key.is_empty() {
        return Err("无法获取百度登录公钥".to_owned());
    }
    let encrypted = encrypt_password(&pubkey, password)?;
    let ppui = 6000 + (now_ms() % 12000);
    let form = vec![
        ("staticpage", "https://pan.baidu.com/disk/home".to_owned()),
        ("charset", "utf-8".to_owned()),
        ("tpl", "netdisk".to_owned()),
        ("subpro", "netdisk_web".to_owned()),
        ("apiver", "v3".to_owned()),
        ("tt", now_ms().to_string()),
        ("gid", gid.clone()),
        ("logintype", "basicLogin".to_owned()),
        ("logLoginType", "pc_loginBasic".to_owned()),
        ("username", account.to_owned()),
        ("password", encrypted),
        ("rsakey", key),
        ("crypttype", "12".to_owned()),
        ("ppui_logintime", ppui.to_string()),
        ("token", token.clone()),
        ("codestring", vcode.to_owned()),
        ("verifycode", vcode.to_owned()),
        ("isPhone", if is_phone { "1" } else { "0" }.to_owned()),
        ("countrycode", String::new()),
        ("u", "https://pan.baidu.com/disk/home".to_owned()),
        ("loginmerge", "true".to_owned()),
        ("splogin", "rate".to_owned()),
        ("mem_pass", "on".to_owned()),
        ("safeflg", "0".to_owned()),
        ("detect", "1".to_owned()),
        ("quick_user", "0".to_owned()),
        ("loginversion", "v4".to_owned()),
        ("supportdv", "1".to_owned()),
        ("callback", "bd__pcbs__login".to_owned()),
    ];
    let tt = now_ms();
    let url = format!(
        "{PASSPORT}/v2/api/?login&tpl=netdisk&apiver=v3&tt={tt}&gid={gid}&loginversion=v4&callback=bd__pcbs__login"
    );
    let response = passport_post(
        &client,
        &mut jar,
        &url,
        "https://passport.baidu.com/v2/?login&tpl=netdisk",
        &form,
    )?;
    let status = response.status();
    let text = response.text().unwrap_or_default();
    let err_no = Regex::new(r#"err_no["']?\s*[=:]\s*["']?(-?\d+)"#)
        .ok()
        .and_then(|regex| regex.captures(&text))
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse::<i64>().ok())
        .unwrap_or(0);
    if jar.get("BDUSS").is_none() {
        if let Some(code_string) = Regex::new(r#"codeString["']?\s*[=:]\s*["']([^"']+)"#)
            .ok()
            .and_then(|regex| regex.captures(&text))
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_owned())
        {
            let captcha = fetch_captcha(&client, &mut jar, &code_string).ok();
            let _ = tx.send(Event::LoginFailed {
                message: "百度要求图形验证码，请输入后重试".to_owned(),
                captcha,
            });
            return Ok(());
        }
        let hint = match err_no {
            50052 => "百度安全系统拦截了账号密码登录（err_no=50052）。请改用扫码登录或 Cookie 登录。",
            4 | 1 => "账号或密码不正确",
            257 => "需要完成百度安全验证",
            _ => "账号密码登录失败，百度可能要求安全验证。请改用扫码或 Cookie 登录。",
        };
        return Err(format!("{hint}（HTTP {}，err_no={err_no}）", status.as_u16()));
    }
    jar.insert("PANWEB", "1");
    let cookie = jar.header();
    let client = ApiClient::new(&cookie).map_err(|error| error.to_string())?;
    finish_login(client, tx);
    Ok(())
}

fn fetch_captcha(
    client: &Client,
    jar: &mut Jar,
    code_string: &str,
) -> Result<std::sync::Arc<kasumi_core::ImageData>, String> {
    let url = format!("{PASSPORT}/cgi-bin/genimage?{code_string}");
    let response = passport_get(client, jar, &url, PAN_REFERER)?;
    let bytes = response
        .bytes()
        .map_err(|error| format!("下载验证码失败：{error}"))?;
    crate::media::decode_image(&bytes, 200).ok_or_else(|| "验证码解码失败".to_owned())
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("初始化 Passport 客户端失败：{error}"))
}

fn passport_get(
    client: &Client,
    jar: &mut Jar,
    url: &str,
    referer: &str,
) -> Result<Response, String> {
    let mut request = client
        .get(url)
        .header(REFERER, referer)
        .header(ACCEPT, "application/json, text/plain, */*");
    let cookie = jar.header();
    if !cookie.is_empty() {
        request = request.header(COOKIE, cookie);
    }
    let response = request
        .send()
        .map_err(|error| format!("Passport 请求失败：{error}"))?;
    jar.absorb(&response);
    Ok(response)
}

fn passport_post(
    client: &Client,
    jar: &mut Jar,
    url: &str,
    referer: &str,
    form: &[(&str, String)],
) -> Result<Response, String> {
    let mut request = client
        .post(url)
        .header(REFERER, referer)
        .header(ACCEPT, "application/json, text/plain, */*")
        .form(form);
    let cookie = jar.header();
    if !cookie.is_empty() {
        request = request.header(COOKIE, cookie);
    }
    let response = request
        .send()
        .map_err(|error| format!("Passport 登录请求失败：{error}"))?;
    jar.absorb(&response);
    Ok(response)
}

fn encrypt_password(pubkey: &str, password: &str) -> Result<String, String> {
    let public_key = RsaPublicKey::from_public_key_pem(pubkey)
        .map_err(|error| format!("解析百度登录公钥失败：{error}"))?;
    let mut rng = rand::thread_rng();
    let encrypted = public_key
        .encrypt(&mut rng, rsa::Pkcs1v15Encrypt, password.as_bytes())
        .map_err(|error| format!("加密密码失败：{error}"))?;
    Ok(STANDARD.encode(encrypted))
}

fn parse_jsonp(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) else {
        return Err("Passport 返回了无法解析的内容".to_owned());
    };
    serde_json::from_str(&trimmed[start..=end])
        .map_err(|error| format!("Passport JSON 解析失败：{error}"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
