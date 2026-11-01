//! Thin, synchronous client for the Baidu Netdisk web endpoints used by the
//! official Linux client (8.9.7). Every request is a plain blocking call so
//! worker threads can own their own retry/segment logic.

use std::io::Read;
use std::sync::RwLock;
use std::time::Duration;

use serde_json::{Value, json};

use crate::models::{
    APP_ID, Entry, FileMetaInfo, Quota, SessionFile, UA, UserInfo, cookie_value, errno_message,
    normalize_cookie,
};

pub const PAN: &str = "https://pan.baidu.com";
pub const PCS: &str = "https://pcs.baidu.com";
pub const DPCS: &str = "https://d.pcs.baidu.com";
pub const THUMB: &str = "https://pcsdata.baidu.com/rest/2.0/pcs/thumbnail";

#[derive(Debug, Clone)]
pub struct ApiError {
    pub errno: i64,
    pub message: String,
    pub http_status: Option<u16>,
}

impl ApiError {
    pub fn new(errno: i64, message: impl Into<String>) -> Self {
        Self {
            errno,
            message: message.into(),
            http_status: None,
        }
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::new(-1, message)
    }

    pub fn is_auth(&self) -> bool {
        matches!(self.errno, -4 | -5 | -6 | 31045 | 36004 | 36005)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let detail = if self.message.is_empty() {
            errno_message(self.errno).to_owned()
        } else {
            self.message.clone()
        };
        if self.errno == 0 {
            write!(f, "{detail}")
        } else {
            write!(f, "{detail}（errno={}）", self.errno)
        }
    }
}

impl std::error::Error for ApiError {}

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Clone)]
pub struct SharePageInfo {
    pub share_id: i64,
    pub uk: i64,
    pub page_type: String,
    pub entries: Vec<Entry>,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    fn reqwest(self) -> reqwest::Method {
        match self {
            Self::Get => reqwest::Method::GET,
            Self::Post => reqwest::Method::POST,
        }
    }
}

/// A logged-in session: cookie jar + the write token.
pub struct ApiClient {
    http: reqwest::blocking::Client,
    download_http: reqwest::blocking::Client,
    cookie: RwLock<String>,
    bdstoken: RwLock<Option<String>>,
    uk: i64,
}

impl ApiClient {
    pub fn new(cookie: &str) -> Result<Self> {
        let cookie = normalize_cookie(cookie);
        if cookie.is_empty() {
            return Err(ApiError::new(-4, "缺少 BDUSS，请先登录"));
        }
        let http = reqwest::blocking::Client::builder()
            .user_agent(UA)
            .danger_accept_invalid_certs(false)
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(90))
            .redirect(reqwest::redirect::Policy::limited(10))
            .pool_max_idle_per_host(8)
            .build()
            .map_err(|error| ApiError::network(format!("初始化 HTTP 客户端失败：{error}")))?;
        let download_http = reqwest::blocking::Client::builder()
            .user_agent(UA)
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(3600))
            .redirect(reqwest::redirect::Policy::limited(10))
            .pool_max_idle_per_host(32)
            .build()
            .map_err(|error| ApiError::network(format!("初始化下载客户端失败：{error}")))?;
        Ok(Self {
            http,
            download_http,
            cookie: RwLock::new(cookie),
            bdstoken: RwLock::new(None),
            uk: 0,
        })
    }

    pub fn from_session(session: &SessionFile) -> Result<Self> {
        let mut client = Self::new(&session.cookie)?;
        if !session.bdstoken.is_empty() {
            *client.bdstoken.write().unwrap() = Some(session.bdstoken.clone());
        }
        client.uk = session.user.uk;
        Ok(client)
    }

    pub fn cookie(&self) -> String {
        self.cookie.read().unwrap().clone()
    }

    pub fn uk(&self) -> i64 {
        self.uk
    }

    pub fn set_uk(&mut self, uk: i64) {
        self.uk = uk;
    }

    /// Merge extra cookies (for example the `BDCLND` share password token).
    pub fn merge_cookie(&self, extra: &str) {
        let extra = normalize_cookie(extra);
        if extra.is_empty() {
            return;
        }
        let mut guard = self.cookie.write().unwrap();
        let mut parts: Vec<String> = guard
            .split(';')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        for new_part in extra.split(';').map(str::trim).filter(|p| !p.is_empty()) {
            let name = new_part.split_once('=').map(|(n, _)| n).unwrap_or(new_part);
            parts.retain(|part| {
                part.split_once('=')
                    .map(|(n, _)| n != name)
                    .unwrap_or(true)
            });
            parts.push(new_part.to_owned());
        }
        *guard = parts.join("; ");
    }

    pub fn session_file(&self, user: &UserInfo) -> SessionFile {
        SessionFile {
            cookie: self.cookie(),
            bdstoken: self.bdstoken.read().unwrap().clone().unwrap_or_default(),
            user: crate::models::UserInfoOwned::from(user),
            saved_at: chrono::Utc::now().timestamp(),
        }
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        use reqwest::header::{HeaderMap, HeaderValue};
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::REFERER,
            HeaderValue::from_static("https://pan.baidu.com/"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            HeaderValue::from_static("application/json, text/plain, */*"),
        );
        headers.insert(
            reqwest::header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("zh-CN,zh;q=0.9"),
        );
        if let Ok(cookie) = HeaderValue::from_str(&self.cookie()) {
            headers.insert(reqwest::header::COOKIE, cookie);
        }
        headers
    }

    fn send(
        &self,
        method: HttpMethod,
        url: &str,
        params: &[(String, String)],
        form: Option<&[(String, String)]>,
        body: Option<Vec<u8>>,
        headers: Option<reqwest::header::HeaderMap>,
        retries: usize,
    ) -> Result<reqwest::blocking::Response> {
        let mut attempt = 0;
        loop {
            let mut request = self
                .http
                .request(method.reqwest(), url)
                .headers(self.headers());
            if !params.is_empty() {
                request = request.query(params);
            }
            if let Some(headers) = &headers {
                request = request.headers(headers.clone());
            }
            if let Some(form) = form {
                request = request.form(form);
            } else if let Some(body) = &body {
                request = request
                    .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                    .body(body.clone());
            }
            match request.send() {
                Ok(response) => return Ok(response),
                Err(error) => {
                    attempt += 1;
                    if attempt > retries {
                        return Err(ApiError::network(format!("网络请求失败：{error}")));
                    }
                    std::thread::sleep(Duration::from_millis(300 * attempt as u64));
                }
            }
        }
    }

    fn send_json(
        &self,
        method: HttpMethod,
        url: &str,
        params: &[(String, String)],
        form: Option<&[(String, String)]>,
        json_body: Option<&Value>,
    ) -> Result<Value> {
        let mut headers = reqwest::header::HeaderMap::new();
        if json_body.is_some() {
            headers.insert(
                reqwest::header::CONTENT_TYPE,
                reqwest::header::HeaderValue::from_static("application/json"),
            );
        }
        let body = json_body.map(|value| value.to_string().into_bytes());
        let response = self.send(
            method,
            url,
            params,
            if json_body.is_some() { None } else { form },
            body,
            Some(headers),
            2,
        )?;
        self.parse_json(response)
    }

    fn parse_json(&self, response: reqwest::blocking::Response) -> Result<Value> {
        let status = response.status();
        let text = response
            .text()
            .map_err(|error| ApiError::network(format!("读取响应失败：{error}")))?;
        let trimmed = text.trim();
        let payload = if trimmed.starts_with('{') {
            trimmed
        } else if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
            &trimmed[start..=end]
        } else {
            let snippet: String = trimmed.chars().take(160).collect();
            return Err(ApiError {
                errno: status.as_u16() as i64,
                message: format!("HTTP {} 返回非 JSON：{snippet}", status.as_u16()),
                http_status: Some(status.as_u16()),
            });
        };
        let value: Value = serde_json::from_str(payload).map_err(|error| ApiError {
            errno: status.as_u16() as i64,
            message: format!("JSON 解析失败：{error}"),
            http_status: Some(status.as_u16()),
        })?;
        let errno = value
            .get("errno")
            .or_else(|| value.get("error_code"))
            .and_then(json_i64);
        if let Some(errno) = errno
            && errno != 0
        {
            let message = value
                .get("error_msg")
                .or_else(|| value.get("show_msg"))
                .or_else(|| value.get("errmsg"))
                .or_else(|| value.get("request_id"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            return Err(ApiError {
                errno,
                message,
                http_status: Some(status.as_u16()),
            });
        }
        if !status.is_success() {
            return Err(ApiError {
                errno: status.as_u16() as i64,
                message: format!("HTTP {}", status.as_u16()),
                http_status: Some(status.as_u16()),
            });
        }
        Ok(value)
    }

    fn get_json(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json_base(PAN, path, params)
    }

    fn get_json_base(&self, base: &str, path: &str, params: &[(&str, String)]) -> Result<Value> {
        let params: Vec<(String, String)> = params
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect();
        self.send_json(
            HttpMethod::Get,
            &format!("{base}{path}"),
            &params,
            None,
            None,
        )
    }

    fn post_form(&self, path: &str, params: &[(&str, String)], form: &[(&str, String)]) -> Result<Value> {
        let params: Vec<(String, String)> = params
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect();
        let form: Vec<(String, String)> = form
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect();
        self.send_json(
            HttpMethod::Post,
            &format!("{PAN}{path}"),
            &params,
            Some(&form),
            None,
        )
    }

    fn post_raw_form(&self, path: &str, params: &[(&str, String)], body: String) -> Result<Value> {
        let params: Vec<(String, String)> = params
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect();
        let response = self
            .http
            .post(format!("{PAN}{path}"))
            .headers(self.headers())
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .query(&params)
            .body(body)
            .send()
            .map_err(|error| ApiError::network(format!("请求失败：{error}")))?;
        self.parse_json(response)
    }

    // ------------------------------------------------------------------
    // Account
    // ------------------------------------------------------------------

    pub fn whoami(&self) -> Result<UserInfo> {
        let value = self.get_json(
            "/rest/2.0/xpan/nas",
            &[("method", "uinfo".to_owned()), ("app_id", APP_ID.to_owned())],
        )?;
        Ok(UserInfo {
            uk: value.get("uk").and_then(json_i64).unwrap_or(self.uk),
            baidu_name: value
                .get("baidu_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            netdisk_name: value
                .get("netdisk_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            vip_type: value.get("vip_type").and_then(json_i64).unwrap_or(0),
            avatar_url: value
                .get("avatar_url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        })
    }

    pub fn quota(&self) -> Result<Quota> {
        let value = self.get_json(
            "/api/quota",
            &[
                ("checkfree", "1".to_owned()),
                ("checkexpire", "1".to_owned()),
                ("app_id", APP_ID.to_owned()),
            ],
        )?;
        Ok(Quota {
            total: value.get("total").and_then(json_u64).unwrap_or(0),
            used: value.get("used").and_then(json_u64).unwrap_or(0),
            free: value.get("free").and_then(json_u64).unwrap_or(0),
            expire: value.get("expire").and_then(Value::as_bool).unwrap_or(false),
            expire_at: value.get("expire_at").and_then(json_i64),
        })
    }

    pub fn bdstoken(&self) -> Result<String> {
        if let Some(token) = self.bdstoken.read().unwrap().clone()
            && !token.is_empty()
        {
            return Ok(token);
        }
        let fields = json!(["bdstoken"]).to_string();
        let value = self.get_json(
            "/api/gettemplatevariable",
            &[("fields", fields), ("app_id", APP_ID.to_owned())],
        )?;
        let token = value
            .get("result")
            .and_then(|result| result.get("bdstoken"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if token.is_empty() {
            return Err(ApiError::new(8001, "未获取到 bdstoken"));
        }
        *self.bdstoken.write().unwrap() = Some(token.clone());
        Ok(token)
    }

    // ------------------------------------------------------------------
    // Files
    // ------------------------------------------------------------------

    pub fn list_page(
        &self,
        path: &str,
        order: &str,
        desc: bool,
        page: u32,
        num: u32,
    ) -> Result<Vec<Entry>> {
        let value = self.get_json(
            "/api/list",
            &[
                ("dir", normalize_remote_path(path)),
                ("web", "1".to_owned()),
                ("app_id", APP_ID.to_owned()),
                ("channel", "chunlei".to_owned()),
                ("clienttype", "0".to_owned()),
                ("num", num.to_string()),
                ("page", page.to_string()),
                ("order", order.to_owned()),
                ("desc", if desc { "1" } else { "0" }.to_owned()),
            ],
        )?;
        let list = value.get("list").and_then(Value::as_array).cloned();
        Ok(list
            .map(|items| {
                items
                    .into_iter()
                    .filter_map(|item| serde_json::from_value::<Entry>(item).ok())
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn list_all(&self, path: &str, order: &str, desc: bool) -> Result<Vec<Entry>> {
        let mut out = Vec::new();
        for page in 1..=100u32 {
            let items = self.list_page(path, order, desc, page, 100)?;
            let count = items.len();
            out.extend(items);
            if count < 100 {
                break;
            }
        }
        Ok(out)
    }

    pub fn filemetas(&self, fsids: &[i64]) -> Result<Vec<FileMetaInfo>> {
        let list = serde_json::to_string(fsids).unwrap_or_else(|_| "[]".to_owned());
        let value = self.get_json(
            "/api/filemetas",
            &[
                ("dlink", "1".to_owned()),
                ("media", "1".to_owned()),
                ("target", "0".to_owned()),
                ("channel", "chunlei".to_owned()),
                ("vip", "0".to_owned()),
                ("web", "1".to_owned()),
                ("app_id", APP_ID.to_owned()),
                ("fsids", list),
            ],
        )?;
        let mut out = Vec::new();
        if let Some(items) = value.get("info").and_then(Value::as_array) {
            for item in items {
                let mut meta: FileMetaInfo =
                    serde_json::from_value(item.clone()).unwrap_or_default();
                if meta.dlink.is_empty() {
                    meta.dlink = item
                        .get("dlink")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                }
                out.push(meta);
            }
        }
        Ok(out)
    }

    /// Resolve a download URL for an account-bound file. Falls back to the
    /// PCS download endpoint (302 to a CDN URL) when filemetas has no dlink.
    pub fn dlink(&self, entry: &Entry) -> Result<String> {
        if entry.fs_id != 0
            && let Ok(metas) = self.filemetas(&[entry.fs_id])
            && let Some(meta) = metas.into_iter().find(|meta| !meta.dlink.is_empty())
        {
            return Ok(meta.dlink);
        }
        let response = self
            .download_http
            .get(format!("{PCS}/rest/2.0/pcs/file"))
            .headers(self.headers())
            .query(&[
                ("app_id", APP_ID),
                ("method", "download"),
                ("path", entry.path.as_str()),
            ])
            .send()
            .map_err(|error| ApiError::network(format!("获取直链失败：{error}")))?;
        let final_url = response.url().clone();
        if response.status().is_success() || response.status().is_redirection() {
            return Ok(final_url.to_string());
        }
        Err(ApiError::new(
            response.status().as_u16() as i64,
            "无法获取直链",
        ))
    }

    pub fn thumbnail(&self, path: &str, width: u32, height: u32) -> Result<Vec<u8>> {
        let response = self
            .download_http
            .get(THUMB)
            .headers(self.headers())
            .query(&[
                ("method", "generate"),
                ("app_id", APP_ID),
                ("path", path),
                ("size", &format!("c{width}_u{height}")),
                ("quality", "90"),
            ])
            .send()
            .map_err(|error| ApiError::network(format!("缩略图请求失败：{error}")))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .map_err(|error| ApiError::network(format!("读取缩略图失败：{error}")))?
            .to_vec();
        if status.is_success() && looks_like_image(&bytes) {
            Ok(bytes)
        } else {
            let snippet = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).to_string();
            Err(ApiError::new(
                status.as_u16() as i64,
                format!("缩略图不可用：{snippet}"),
            ))
        }
    }

    pub fn delete(&self, paths: &[String]) -> Result<Value> {
        let token = self.bdstoken()?;
        let filelist = serde_json::to_string(paths).unwrap_or_else(|_| "[]".to_owned());
        self.post_form(
            "/api/filemanager",
            &[
                ("opera", "delete".to_owned()),
                ("bdstoken", token),
                ("app_id", APP_ID.to_owned()),
                ("channel", "chunlei".to_owned()),
                ("web", "1".to_owned()),
            ],
            &[
                ("async", "0".to_owned()),
                ("filelist", filelist),
                ("ondup", "fail".to_owned()),
            ],
        )
    }

    pub fn rename(&self, path: &str, new_name: &str) -> Result<Value> {
        let token = self.bdstoken()?;
        let encoded_path = urlencoding::encode(path).into_owned();
        let encoded_name = urlencoding::encode(new_name).into_owned();
        let body = format!(
            "filelist={}",
            json!([{"path": encoded_path, "newname": encoded_name}])
        );
        self.post_raw_form(
            "/api/filemanager",
            &[
                ("opera", "rename".to_owned()),
                ("bdstoken", token),
                ("app_id", APP_ID.to_owned()),
                ("channel", "chunlei".to_owned()),
                ("web", "1".to_owned()),
                ("async", "0".to_owned()),
            ],
            body,
        )
    }

    pub fn move_paths(&self, paths: &[String], dest: &str) -> Result<Value> {
        self.file_operation(paths, dest, "move")
    }

    pub fn copy_paths(&self, paths: &[String], dest: &str) -> Result<Value> {
        self.file_operation(paths, dest, "copy")
    }

    fn file_operation(&self, paths: &[String], dest: &str, opera: &str) -> Result<Value> {
        let token = self.bdstoken()?;
        let dest = normalize_remote_path(dest);
        let encoded_dest = urlencoding::encode(&dest).into_owned();
        let items: Vec<Value> = paths
            .iter()
            .map(|path| {
                let name = path.rsplit('/').next().unwrap_or(path);
                json!({
                    "path": urlencoding::encode(path).into_owned(),
                    "dest": encoded_dest,
                    "newname": urlencoding::encode(name).into_owned(),
                    "ondup": "newcopy",
                })
            })
            .collect();
        let body = format!("filelist={}", serde_json::to_string(&items).unwrap_or_default());
        self.post_raw_form(
            "/api/filemanager",
            &[
                ("opera", opera.to_owned()),
                ("bdstoken", token),
                ("app_id", APP_ID.to_owned()),
                ("channel", "chunlei".to_owned()),
                ("web", "1".to_owned()),
                ("async", "0".to_owned()),
            ],
            body,
        )
    }

    pub fn create_folder(&self, path: &str) -> Result<Value> {
        self.post_form(
            "/api/create",
            &[("a", "commit".to_owned()), ("app_id", APP_ID.to_owned())],
            &[
                ("path", normalize_remote_path(path)),
                ("size", "0".to_owned()),
                ("isdir", "1".to_owned()),
                ("block_list", "[]".to_owned()),
                ("rtype", "3".to_owned()),
            ],
        )
    }

    pub fn search(&self, keyword: &str, dir: &str) -> Result<Vec<Entry>> {
        let value = self.get_json(
            "/api/search",
            &[
                ("key", keyword.to_owned()),
                ("dir", dir.to_owned()),
                ("recursion", "1".to_owned()),
                ("web", "1".to_owned()),
                ("app_id", APP_ID.to_owned()),
                ("channel", "chunlei".to_owned()),
                ("clienttype", "0".to_owned()),
                ("page", "1".to_owned()),
                ("num", "100".to_owned()),
            ],
        )?;
        let list = value.get("list").and_then(Value::as_array);
        Ok(list
            .map(|items| {
                items
                    .iter()
                    .cloned()
                    .filter_map(|item| serde_json::from_value::<Entry>(item).ok())
                    .collect()
            })
            .unwrap_or_default())
    }

    // ------------------------------------------------------------------
    // Upload
    // ------------------------------------------------------------------

    pub fn precreate(
        &self,
        server_path: &str,
        size: u64,
        block_list: &[String],
        rtype: u32,
    ) -> Result<Value> {
        let blocks = serde_json::to_string(block_list).unwrap_or_else(|_| "[]".to_owned());
        self.post_form(
            "/api/precreate",
            &[("app_id", APP_ID.to_owned())],
            &[
                ("path", normalize_remote_path(server_path)),
                ("size", size.to_string()),
                ("isdir", "0".to_owned()),
                ("autoinit", "1".to_owned()),
                ("block_list", blocks),
                ("rtype", rtype.to_string()),
            ],
        )
    }

    pub fn upload_part(
        &self,
        host: &str,
        server_path: &str,
        upload_id: &str,
        part_seq: u32,
        part_offset: u64,
        data: &[u8],
    ) -> Result<Value> {
        let response = self
            .http
            .post(format!("https://{host}/rest/2.0/pcs/superfile2"))
            .headers(self.headers())
            .query(&[
                ("method", "upload"),
                ("app_id", APP_ID),
                ("path", server_path),
                ("uploadid", upload_id),
                ("partseq", &part_seq.to_string()),
                ("partoffset", &part_offset.to_string()),
            ])
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(data.to_vec())
            .send()
            .map_err(|error| ApiError::network(format!("分片上传失败：{error}")))?;
        self.parse_json(response)
    }

    pub fn commit(
        &self,
        server_path: &str,
        size: u64,
        block_list: &[String],
        upload_id: &str,
        rtype: u32,
    ) -> Result<Value> {
        let blocks = serde_json::to_string(block_list).unwrap_or_else(|_| "[]".to_owned());
        self.post_form(
            "/api/create",
            &[("a", "commit".to_owned()), ("app_id", APP_ID.to_owned())],
            &[
                ("path", normalize_remote_path(server_path)),
                ("size", size.to_string()),
                ("isdir", "0".to_owned()),
                ("block_list", blocks),
                ("uploadid", upload_id.to_owned()),
                ("rtype", rtype.to_string()),
            ],
        )
    }

    // ------------------------------------------------------------------
    // Shares
    // ------------------------------------------------------------------

    pub fn share_set(
        &self,
        paths: &[String],
        pwd: &str,
        period: u32,
        public: bool,
    ) -> Result<Value> {
        let token = self.bdstoken()?;
        let path_list = serde_json::to_string(paths).unwrap_or_else(|_| "[]".to_owned());
        let channel_list = json!([]).to_string();
        let form = vec![
            ("pwd", pwd.to_owned()),
            ("schannel", "0".to_owned()),
            ("period", period.to_string()),
            ("path_list", path_list),
            ("channel_list", channel_list),
            ("public", if public { "1" } else { "0" }.to_owned()),
            ("memo", String::new()),
            ("view_cnt", "0".to_owned()),
            ("eflag_disable", "0".to_owned()),
            ("is_shared_space", "0".to_owned()),
            ("allow_comment_for_space", "0".to_owned()),
            ("download_trans_auth_space", "0".to_owned()),
        ];
        self.post_form(
            "/share/set",
            &[
                ("app_id", APP_ID.to_owned()),
                ("channel", "chunlei".to_owned()),
                ("clienttype", "0".to_owned()),
                ("web", "1".to_owned()),
                ("bdstoken", token),
            ],
            &form,
        )
    }

    pub fn my_shares(&self, page: u32, num: u32) -> Result<Value> {
        self.get_json(
            "/share/record",
            &[
                ("app_id", APP_ID.to_owned()),
                ("channel", "chunlei".to_owned()),
                ("clienttype", "0".to_owned()),
                ("web", "1".to_owned()),
                ("page", page.to_string()),
                ("num", num.to_string()),
            ],
        )
    }

    pub fn cancel_share(&self, share_ids: &[i64]) -> Result<Value> {
        let token = self.bdstoken()?;
        let ids = serde_json::to_string(share_ids).unwrap_or_else(|_| "[]".to_owned());
        self.post_form(
            "/share/cancel",
            &[
                ("app_id", APP_ID.to_owned()),
                ("channel", "chunlei".to_owned()),
                ("clienttype", "0".to_owned()),
                ("web", "1".to_owned()),
                ("bdstoken", token),
            ],
            &[("shareid_list", ids)],
        )
    }

    /// Fetch a share page and parse its embedded `locals-data` JSON. `surl`
    /// must be the shorturl token without the leading `1`.
    pub fn share_page_info(&self, surl: &str) -> Result<SharePageInfo> {
        let url = format!("{PAN}/s/1{surl}");
        let response = self
            .download_http
            .get(&url)
            .headers(self.headers())
            .send()
            .map_err(|error| ApiError::network(format!("打开分享页失败：{error}")))?;
        let status = response.status();
        let html = response
            .text()
            .map_err(|error| ApiError::network(format!("读取分享页失败：{error}")))?;
        let locals = extract_share_locals(&html).ok_or_else(|| {
            ApiError::new(
                status.as_u16() as i64,
                "无法解析分享页面，链接可能已失效或需要登录",
            )
        })?;
        let share_id = locals
            .get("shareid")
            .and_then(json_i64)
            .unwrap_or(0);
        let uk = locals
            .get("share_uk")
            .and_then(json_i64)
            .or_else(|| locals.get("uk").and_then(json_i64))
            .unwrap_or(0);
        let page_type = locals
            .get("share_page_type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let entries: Vec<Entry> = locals
            .get("file_list")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .cloned()
                    .filter_map(|item| serde_json::from_value::<Entry>(item).ok())
                    .collect()
            })
            .unwrap_or_default();
        let title = entries
            .first()
            .map(|entry| entry.server_filename.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("分享 {share_id}"));
        if share_id == 0 || entries.is_empty() {
            return Err(ApiError::new(-2, "分享页面中没有文件列表"));
        }
        Ok(SharePageInfo {
            share_id,
            uk,
            page_type,
            entries,
            title,
        })
    }

    /// Fetch the public share page and extract `shareid` and `uk`.
    pub fn share_page_meta(&self, surl: &str) -> Result<(i64, i64)> {
        let url = format!("{PAN}/s/1{surl}");
        let response = self
            .download_http
            .get(&url)
            .headers(self.headers())
            .send()
            .map_err(|error| ApiError::network(format!("打开分享页失败：{error}")))?;
        let status = response.status();
        let html = response
            .text()
            .map_err(|error| ApiError::network(format!("读取分享页失败：{error}")))?;
        let shareid = find_json_number(&html, "shareid");
        let uk = find_json_number(&html, "uk");
        match (shareid, uk) {
            (Some(shareid), Some(uk)) => Ok((shareid, uk)),
            _ => Err(ApiError::new(
                status.as_u16() as i64,
                "无法从分享页解析 shareid/uk，链接可能已失效",
            )),
        }
    }

    pub fn share_verify(&self, surl: &str, pwd: &str) -> Result<String> {
        let value = self.post_form(
            "/share/verify",
            &[
                ("surl", surl.to_owned()),
                ("tpl", "netdisk".to_owned()),
                ("web", "1".to_owned()),
            ],
            &[
                ("pwd", pwd.to_owned()),
                ("vcode", String::new()),
                ("vcode_str", String::new()),
            ],
        )?;
        Ok(value
            .get("randsk")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned())
    }

    pub fn share_list(
        &self,
        surl: &str,
        share_id: i64,
        uk: i64,
        dir: &str,
    ) -> Result<Value> {
        self.get_json(
            "/share/list",
            &[
                ("shorturl", surl.to_owned()),
                ("shareid", share_id.to_string()),
                ("uk", uk.to_string()),
                ("dir", dir.to_owned()),
                ("page", "1".to_owned()),
                ("num", "100".to_owned()),
                ("order", "other".to_owned()),
                ("desc", "1".to_owned()),
                ("showempty", "0".to_owned()),
                ("web", "1".to_owned()),
                ("app_id", APP_ID.to_owned()),
                ("channel", "chunlei".to_owned()),
                ("clienttype", "0".to_owned()),
            ],
        )
    }

    pub fn share_transfer(
        &self,
        share_id: i64,
        from_uk: i64,
        fs_ids: &[i64],
        dest: &str,
        sekey: &str,
    ) -> Result<Value> {
        let token = self.bdstoken()?;
        let fsidlist = serde_json::to_string(fs_ids).unwrap_or_else(|_| "[]".to_owned());
        let mut params = vec![
            ("shareid", share_id.to_string()),
            ("from", from_uk.to_string()),
            ("bdstoken", token),
            ("app_id", APP_ID.to_owned()),
            ("channel", "chunlei".to_owned()),
            ("clienttype", "0".to_owned()),
            ("web", "1".to_owned()),
            ("ondup", "newcopy".to_owned()),
            ("async", "1".to_owned()),
        ];
        if !sekey.is_empty() {
            params.push(("sekey", urlencoding::decode(sekey).map(|value| value.into_owned()).unwrap_or_else(|_| sekey.to_owned())));
        }
        self.post_form(
            "/share/transfer",
            &params,
            &[
                ("fsidlist", fsidlist),
                ("path", dest.to_owned()),
            ],
        )
    }

    // ------------------------------------------------------------------
    // Downloads / previews
    // ------------------------------------------------------------------

    pub fn open_range(
        &self,
        url: &str,
        start: u64,
        end: Option<u64>,
    ) -> Result<reqwest::blocking::Response> {
        let range = match end {
            Some(end) => format!("bytes={start}-{end}"),
            None => format!("bytes={start}-"),
        };
        let response = self
            .download_http
            .get(url)
            .headers(self.headers())
            .header(reqwest::header::RANGE, range)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .map_err(|error| ApiError::network(format!("下载请求失败：{error}")))?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::PARTIAL_CONTENT
        {
            Ok(response)
        } else {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            let snippet: String = body.chars().take(200).collect();
            Err(ApiError {
                errno: status.as_u16() as i64,
                message: format!("下载失败 HTTP {}：{snippet}", status.as_u16()),
                http_status: Some(status.as_u16()),
            })
        }
    }

    /// Probe a URL with a one-byte range request. Returns `(total_size,
    /// range_supported)`.
    pub fn probe_size(&self, url: &str) -> Result<(u64, bool)> {
        let response = self.open_range(url, 0, Some(0))?;
        let partial = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let total = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.rsplit('/').next())
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| {
                response
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .unwrap_or(0);
        drop(response);
        Ok((total, partial))
    }

    /// Fetch the head of a remote file. Returns `(bytes, total_size)`.
    pub fn fetch_prefix(&self, url: &str, limit: usize) -> Result<(Vec<u8>, u64)> {
        self.fetch_range(url, 0, limit)
    }

    /// Fetch `length` bytes starting at `start`. Returns `(bytes, total_size)`.
    pub fn fetch_range(&self, url: &str, start: u64, length: usize) -> Result<(Vec<u8>, u64)> {
        let end = start.saturating_add(length.max(1) as u64).saturating_sub(1);
        let mut response = self.open_range(url, start, Some(end))?;
        let total = response_total(&response, end + 1);
        let mut output = Vec::with_capacity(length.min(256 * 1024));
        let mut buffer = vec![0u8; 64 * 1024];
        while output.len() < length {
            let wanted = (length - output.len()).min(buffer.len());
            match response.read(&mut buffer[..wanted]) {
                Ok(0) => break,
                Ok(read) => output.extend_from_slice(&buffer[..read]),
                Err(error) => {
                    if output.is_empty() {
                        return Err(ApiError::network(format!("读取文件失败：{error}")));
                    }
                    break;
                }
            }
        }
        Ok((output, total))
    }

    /// Fetch a whole remote file into memory, up to `limit` bytes.
    pub fn fetch_all(&self, url: &str, limit: u64) -> Result<(Vec<u8>, u64)> {
        let mut response = self.open_range(url, 0, None)?;
        let total = response_total(&response, 0).max(limit.min(1));
        if total > limit {
            return Err(ApiError::new(
                -2,
                format!(
                    "文件过大（{}），内置预览上限为 {}",
                    crate::models::human_size(total),
                    crate::models::human_size(limit)
                ),
            ));
        }
        let mut output = Vec::new();
        let mut buffer = vec![0u8; 128 * 1024];
        loop {
            match response.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    output.extend_from_slice(&buffer[..read]);
                    if output.len() as u64 > limit {
                        return Err(ApiError::new(-2, "文件超过预览上限"));
                    }
                }
                Err(error) => {
                    return Err(ApiError::network(format!("读取文件失败：{error}")));
                }
            }
        }
        Ok((output, total))
    }

    pub fn download_to_file(
        &self,
        url: &str,
        path: &std::path::Path,
        progress: &dyn Fn(u64),
    ) -> Result<u64> {
        let mut response = self.open_range(url, 0, None)?;
        let mut file = std::fs::File::create(path)
            .map_err(|error| ApiError::new(-1, format!("创建文件失败：{error}")))?;
        let mut buffer = vec![0u8; 256 * 1024];
        let mut done = 0u64;
        loop {
            let read = response
                .read(&mut buffer)
                .map_err(|error| ApiError::network(format!("下载中断：{error}")))?;
            if read == 0 {
                break;
            }
            use std::io::Write;
            file.write_all(&buffer[..read])
                .map_err(|error| ApiError::new(-1, format!("写入文件失败：{error}")))?;
            done += read as u64;
            progress(done);
        }
        Ok(done)
    }
}

fn response_total(response: &reqwest::blocking::Response, fallback: u64) -> u64 {
    response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit('/').next())
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| {
            response
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn looks_like_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"GIF8")
        || (bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
        || bytes.starts_with(b"BM")
}

fn json_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

fn json_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

pub fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "/".to_owned();
    }
    let mut out = if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    };
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

pub fn join_remote(dir: &str, name: &str) -> String {
    let dir = normalize_remote_path(dir);
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

pub fn parent_remote(path: &str) -> String {
    let path = normalize_remote_path(path);
    if path == "/" {
        return "/".to_owned();
    }
    let (parent, _) = path.rsplit_once('/').unwrap_or(("/", &path));
    if parent.is_empty() {
        "/".to_owned()
    } else {
        parent.to_owned()
    }
}

fn extract_share_locals(html: &str) -> Option<Value> {
    let regex = regex::Regex::new(r#"(?s)<script id="locals-data" type="application/json">(.*?)</script>"#)
        .ok()?;
    let captures = regex.captures(html)?;
    let text = captures.get(1)?.as_str().trim();
    serde_json::from_str(text).ok()
}

fn find_json_number(html: &str, key: &str) -> Option<i64> {
    let pattern = format!("\"{key}\"");
    let index = html.find(&pattern)?;
    let rest = &html[index + pattern.len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let digits: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect();
    digits.parse().ok()
}

/// Extract `surl` and optional `pwd` from a Baidu share URL or bare surl.
pub fn parse_share_link(input: &str) -> Option<(String, Option<String>)> {
    let text = input.trim();
    if text.is_empty() {
        return None;
    }
    let mut surl = None;
    if let Some(index) = text.find("/s/1") {
        let rest = &text[index + 4..];
        let end = rest
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
            .unwrap_or(rest.len());
        if end > 0 {
            surl = Some(rest[..end].to_owned());
        }
    } else if let Some(index) = text.find("surl=") {
        let rest = &text[index + 5..];
        let end = rest
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
            .unwrap_or(rest.len());
        if end > 0 {
            surl = Some(rest[..end].to_owned());
        }
    } else if text.starts_with("1")
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        surl = Some(text.to_owned());
    }
    let surl = surl?;
    let pwd = text
        .split(['?', '&'])
        .find_map(|part| part.strip_prefix("pwd="))
        .map(|value| value.chars().take(4).collect::<String>())
        .filter(|value| !value.is_empty());
    Some((surl, pwd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_links_are_parsed() {
        assert_eq!(
            parse_share_link("https://pan.baidu.com/s/1abcDEF_12?pwd=x1y2"),
            Some(("abcDEF_12".to_owned(), Some("x1y2".to_owned())))
        );
        assert_eq!(
            parse_share_link("https://pan.baidu.com/s/1zzz"),
            Some(("zzz".to_owned(), None))
        );
        assert_eq!(parse_share_link("abc"), None);
    }

    #[test]
    fn paths_are_normalized() {
        assert_eq!(normalize_remote_path("a/b/"), "/a/b");
        assert_eq!(normalize_remote_path("/"), "/");
        assert_eq!(join_remote("/", "x"), "/x");
        assert_eq!(join_remote("/a", "x"), "/a/x");
        assert_eq!(parent_remote("/a/b"), "/a");
        assert_eq!(parent_remote("/"), "/");
    }

    #[test]
    fn cookies_are_normalized() {
        assert_eq!(
            normalize_cookie("Cookie: BDUSS=abc; STOKEN=def;"),
            "BDUSS=abc; STOKEN=def"
        );
        assert_eq!(cookie_value("BDUSS=abc; STOKEN=def", "STOKEN").as_deref(), Some("def"));
    }
}
