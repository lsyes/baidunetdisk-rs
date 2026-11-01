//! Data models shared by the API layer, background workers and the UI.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "250528";
pub const UA: &str = "netdisk;P2SP;8.9.7;netdisk;PC;zh";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Thumbs {
    #[serde(default)]
    pub url1: String,
    #[serde(default)]
    pub url2: String,
    #[serde(default)]
    pub url3: String,
}

/// A row from `GET /api/list`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Entry {
    #[serde(default)]
    pub fs_id: i64,
    #[serde(default)]
    pub server_filename: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub isdir: i64,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub md5: String,
    #[serde(default)]
    pub server_mtime: i64,
    #[serde(default)]
    pub server_ctime: i64,
    #[serde(default)]
    pub local_mtime: i64,
    #[serde(default)]
    pub category: i64,
    #[serde(default)]
    pub thumbs: Option<Thumbs>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub owner_id: Option<i64>,
    #[serde(default)]
    pub share: Option<i64>,
    #[serde(default)]
    pub dir_empty: Option<i64>,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.isdir == 1
    }

    pub fn extension(&self) -> String {
        self.server_filename
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default()
    }

    pub fn kind(&self) -> FileKind {
        if self.is_dir() {
            return FileKind::Folder;
        }
        FileKind::from_extension(&self.extension())
    }

    pub fn modified(&self) -> i64 {
        if self.server_mtime > 0 {
            self.server_mtime
        } else {
            self.local_mtime
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Folder,
    Pdf,
    Markdown,
    Text,
    Document,
    Spreadsheet,
    Presentation,
    Image,
    Video,
    Audio,
    Code,
    Archive,
    Apk,
    Exe,
    Font,
    Psd,
    Ai,
    Epub,
    Torrent,
    Cad,
    Other,
}

impl FileKind {
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "pdf" => Self::Pdf,
            "md" | "markdown" | "mdown" | "mkd" => Self::Markdown,
            "txt" | "log" | "ini" | "conf" | "csv" | "tsv" | "nfo" | "srt" | "ass" => Self::Text,
            "doc" | "docx" | "dot" | "dotx" | "rtf" | "odt" | "wps" => Self::Document,
            "xls" | "xlsx" | "xlt" | "xltx" | "ods" | "et" => Self::Spreadsheet,
            "ppt" | "pptx" | "pps" | "ppsx" | "pot" | "potx" | "dps" => Self::Presentation,
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "tif" | "tiff" | "svg"
            | "heic" | "heif" | "avif" => Self::Image,
            "mp4" | "mkv" | "mov" | "avi" | "flv" | "wmv" | "webm" | "m4v" | "mpg" | "mpeg"
            | "rmvb" | "rm" | "ts" | "m2ts" | "3gp" | "vob" => Self::Video,
            "mp3" | "flac" | "aac" | "wav" | "ogg" | "m4a" | "wma" | "ape" | "amr" | "opus"
            | "mid" | "midi" => Self::Audio,
            "rs" | "py" | "js" | "mjs" | "cjs" | "tsx" | "jsx" | "json" | "yaml"
            | "yml" | "toml" | "xml" | "html" | "htm" | "css" | "scss" | "less" | "c" | "h"
            | "cc" | "cpp" | "hpp" | "go" | "java" | "kt" | "swift" | "sh" | "bash" | "zsh"
            | "sql" | "lua" | "rb" | "php" | "vue" | "svelte" | "zig" | "nim" | "dart"
            | "gradle" | "cmake" | "dockerfile" | "gitignore" => Self::Code,
            "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "jar" | "iso"
            | "cab" | "deb" | "rpm" => Self::Archive,
            "apk" | "aab" | "ipa" | "dmg" => Self::Apk,
            "exe" | "msi" | "appimage" | "bin" => Self::Exe,
            "ttf" | "otf" | "ttc" | "woff" | "woff2" | "eot" => Self::Font,
            "psd" | "psb" => Self::Psd,
            "ai" | "ait" => Self::Ai,
            "epub" | "mobi" | "azw3" | "fb2" | "chm" => Self::Epub,
            "torrent" => Self::Torrent,
            "dwg" | "dxf" | "dwt" | "vsd" | "vsdx" | "mmap" | "mm" | "xmind" => Self::Cad,
            _ => Self::Other,
        }
    }

    /// The official Baidu file-type icon that ships with the 8.9.7 client.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Folder => "ic_folder",
            Self::Pdf => "ic_pdf",
            Self::Markdown => "ic_code",
            Self::Text => "ic_txt",
            Self::Document => "ic_doc",
            Self::Spreadsheet => "ic_excel",
            Self::Presentation => "ic_ppt",
            Self::Image => "ic_image",
            Self::Video => "ic_video",
            Self::Audio => "ic_audio",
            Self::Code => "ic_code",
            Self::Archive => "ic_zip",
            Self::Apk => "ic_apk",
            Self::Exe => "ic_exe",
            Self::Font => "ic_font",
            Self::Psd => "ic_psd",
            Self::Ai => "ic_ai",
            Self::Epub => "ic_epub",
            Self::Torrent => "ic_torrent",
            Self::Cad => "ic_cad",
            Self::Other => "ic_other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Folder => "文件夹",
            Self::Pdf => "PDF",
            Self::Markdown => "Markdown",
            Self::Text => "文本",
            Self::Document => "Word 文档",
            Self::Spreadsheet => "表格",
            Self::Presentation => "演示文稿",
            Self::Image => "图片",
            Self::Video => "视频",
            Self::Audio => "音频",
            Self::Code => "代码",
            Self::Archive => "压缩包",
            Self::Apk => "安装包",
            Self::Exe => "程序",
            Self::Font => "字体",
            Self::Psd => "设计稿",
            Self::Ai => "Illustrator",
            Self::Epub => "电子书",
            Self::Torrent => "种子",
            Self::Cad => "图纸",
            Self::Other => "文件",
        }
    }

    pub fn can_preview_text(self) -> bool {
        matches!(self, Self::Markdown | Self::Text | Self::Code | Self::Other)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FileMetaInfo {
    #[serde(default)]
    pub fs_id: i64,
    #[serde(default)]
    pub dlink: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub md5: String,
    #[serde(default)]
    pub thumbs: Option<Thumbs>,
    #[serde(default)]
    pub filename: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserInfo {
    #[serde(default)]
    pub uk: i64,
    #[serde(default)]
    pub baidu_name: String,
    #[serde(default)]
    pub netdisk_name: String,
    #[serde(default)]
    pub vip_type: i64,
    #[serde(default)]
    pub avatar_url: String,
}

impl UserInfo {
    pub fn display_name(&self) -> &str {
        if !self.netdisk_name.is_empty() {
            &self.netdisk_name
        } else if !self.baidu_name.is_empty() {
            &self.baidu_name
        } else {
            "百度网盘用户"
        }
    }

    pub fn initial(&self) -> String {
        self.display_name().chars().next().unwrap_or('百').to_string()
    }

    pub fn vip_label(&self) -> &'static str {
        match self.vip_type {
            2 => "超级会员",
            1 => "会员",
            _ => "普通用户",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Quota {
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub used: u64,
    #[serde(default)]
    pub free: u64,
    #[serde(default)]
    pub expire: bool,
    #[serde(default)]
    pub expire_at: Option<i64>,
}

impl Quota {
    pub fn used_fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f32 / self.total as f32).clamp(0.0, 1.0)
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionFile {
    #[serde(default)]
    pub cookie: String,
    #[serde(default)]
    pub bdstoken: String,
    #[serde(default)]
    pub user: UserInfoOwned,
    #[serde(default)]
    pub saved_at: i64,
}

/// Serializable mirror of [`UserInfo`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserInfoOwned {
    #[serde(default)]
    pub uk: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub netdisk_name: String,
    #[serde(default)]
    pub vip_type: i64,
    #[serde(default)]
    pub avatar_url: String,
}

impl From<&UserInfo> for UserInfoOwned {
    fn from(value: &UserInfo) -> Self {
        Self {
            uk: value.uk,
            name: value.baidu_name.clone(),
            netdisk_name: value.netdisk_name.clone(),
            vip_type: value.vip_type,
            avatar_url: value.avatar_url.clone(),
        }
    }
}

impl From<&UserInfoOwned> for UserInfo {
    fn from(value: &UserInfoOwned) -> Self {
        Self {
            uk: value.uk,
            baidu_name: value.name.clone(),
            netdisk_name: value.netdisk_name.clone(),
            vip_type: value.vip_type,
            avatar_url: value.avatar_url.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub download_dir: String,
    pub upload_threads: usize,
    pub download_threads: usize,
    pub max_active_downloads: usize,
    pub verify_md5: bool,
    pub grid_view: bool,
    pub sort_key: String,
    pub sort_desc: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
        let base = PathBuf::from(&home);
        let download_dir = if base.join("下载").is_dir() {
            base.join("下载").join("百度网盘")
        } else if base.join("Downloads").is_dir() {
            base.join("Downloads").join("BaiduNetdisk")
        } else {
            base.join("BaiduNetdisk")
        };
        Self {
            download_dir: download_dir.display().to_string(),
            upload_threads: 3,
            download_threads: 8,
            max_active_downloads: 2,
            verify_md5: false,
            grid_view: false,
            sort_key: "name".to_owned(),
            sort_desc: false,
        }
    }
}

pub fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
    PathBuf::from(home).join(".config").join("baidunetdisk-rs")
}

pub fn session_path() -> PathBuf {
    config_dir().join("session.json")
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

pub fn human_speed(bytes_per_second: f64) -> String {
    if bytes_per_second <= 0.0 {
        return "0 B/s".to_owned();
    }
    format!("{}/s", human_size(bytes_per_second as u64))
}

pub fn format_timestamp(seconds: i64) -> String {
    use chrono::{Local, TimeZone};
    if seconds <= 0 {
        return "—".to_owned();
    }
    match Local.timestamp_opt(seconds, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        _ => "—".to_owned(),
    }
}

pub fn errno_message(errno: i64) -> &'static str {
    match errno {
        -1 => "用户名或密码错误",
        -2 => "参数错误",
        -3 => "用户未激活",
        -4 => "登录信息已失效，请重新登录",
        -5 => "登录信息无效，请重新登录",
        -6 => "未登录或登录已失效",
        -7 => "文件或目录名错误",
        -8 => "文件或目录已存在",
        -9 => "文件或目录不存在",
        -10 => "空间已满",
        -11 => "父目录不存在",
        -20 => "需要验证码",
        2 => "参数错误",
        4 => "操作不被允许",
        20 => "没有权限",
        114 => "任务不存在",
        133 => "需要广告验证",
        2102 => "群组不存在",
        2103 => "不在该群组中",
        2131 => "分享不存在",
        2132 => "无权访问该分享",
        31023 => "参数错误",
        31045 => "用户不存在或登录已失效",
        31066 => "文件不存在",
        31304 => "文件类型不支持",
        31363 => "分片校验失败",
        36001 => "参数错误",
        36003 => "请刷新后重试",
        36004 => "请重新登录",
        36005 => "未登录",
        36007 => "未授权",
        36009 => "空间不足",
        36010 => "文件不存在",
        36013 => "任务数量过多",
        36025 => "分享已失效",
        36032 => "内容因违规被屏蔽",
        8001 => "缺少写操作令牌（bdstoken）",
        _ => "请求失败",
    }
}

/// Normalize a cookie from any of: a full `Cookie:` header, a raw
/// `BDUSS=...; STOKEN=...` string, or a copied browser header.
pub fn normalize_cookie(raw: &str) -> String {
    let mut text = raw.trim().to_owned();
    if text.to_ascii_lowercase().starts_with("cookie:") {
        text = text[7..].trim().to_owned();
    }
    let mut pairs = Vec::new();
    for part in text.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((name, value)) = part.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || value.trim().is_empty() {
            continue;
        }
        pairs.push(format!("{}={}", name, value.trim()));
    }
    pairs.join("; ")
}

pub fn cookie_value(cookie: &str, name: &str) -> Option<String> {
    cookie.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key.trim() == name).then(|| value.trim().to_owned())
    })
}
