//! Share models and helpers.

use serde_json::Value;

use crate::api::normalize_remote_path;
use crate::models::Entry;

#[derive(Debug, Clone, Default)]
pub struct ShareSummary {
    pub share_id: i64,
    pub surl: String,
    pub pwd: String,
    pub ctime: i64,
    pub expired_type: i64,
    pub expired_time: i64,
    pub status: i64,
    pub names: Vec<String>,
    pub fs_ids: Vec<i64>,
    pub shortlink: String,
}

impl ShareSummary {
    pub fn link(&self) -> String {
        let mut link = if !self.shortlink.is_empty() {
            self.shortlink.clone()
        } else if self.surl.starts_with("http") {
            self.surl.clone()
        } else {
            let token = self.surl.trim_start_matches('/').trim_start_matches("s/");
            let token = if token.starts_with('1') {
                token.to_owned()
            } else {
                format!("1{token}")
            };
            format!("https://pan.baidu.com/s/{token}")
        };
        if !self.pwd.is_empty() && !link.contains("pwd=") {
            link.push_str(if link.contains('?') { "&pwd=" } else { "?pwd=" });
            link.push_str(&self.pwd);
        }
        link
    }

    pub fn period_label(&self) -> String {
        // 0 = permanent, 1 = 7 days, 2 = 30 days, 3 = custom days.
        match self.expired_type {
            0 => "永久有效".to_owned(),
            1 => "7 天".to_owned(),
            2 => "30 天".to_owned(),
            _ => {
                if self.expired_time > 0 {
                    crate::models::format_timestamp(self.expired_time)
                } else {
                    "自定义".to_owned()
                }
            }
        }
    }
}

pub fn parse_share_summaries(value: &Value) -> Vec<ShareSummary> {
    let mut out = Vec::new();
    for item in value
        .get("list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let share_id = item
            .get("shareid")
            .and_then(json_i64)
            .or_else(|| item.get("shareId").and_then(json_i64))
            .or_else(|| item.get("share_id").and_then(json_i64))
            .unwrap_or(0);
        let surl = item
            .get("shorturl")
            .or_else(|| item.get("surl"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let shortlink = item
            .get("shortlink")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let pwd = item
            .get("passwd")
            .or_else(|| item.get("pwd"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let mut names = Vec::new();
        let mut fs_ids = Vec::new();
        for key in ["file_list", "filelist", "files"] {
            if let Some(files) = item.get(key).and_then(Value::as_array) {
                for file in files {
                    if let Some(name) = file
                        .get("server_filename")
                        .or_else(|| file.get("name"))
                        .and_then(Value::as_str)
                    {
                        names.push(name.to_owned());
                    }
                    if let Some(fs_id) = file.get("fs_id").and_then(json_i64) {
                        fs_ids.push(fs_id);
                    }
                }
            }
        }
        if names.is_empty() {
            if let Some(path) = item.get("typicalPath").and_then(Value::as_str) {
                let name = path.rsplit('/').next().unwrap_or(path).to_owned();
                if !name.is_empty() && name != "分享的文件已被删除" {
                    names.push(name);
                }
            } else if let Some(name) = item.get("server_filename").and_then(Value::as_str) {
                names.push(name.to_owned());
            }
        }
        if let Some(list) = item.get("fsid_list").and_then(Value::as_array) {
            for fs_id in list.iter().filter_map(|value| json_i64(value)) {
                fs_ids.push(fs_id);
            }
        }
        if let Some(list) = item.get("fsIds").and_then(Value::as_array) {
            for fs_id in list.iter().filter_map(|value| json_i64(value)) {
                fs_ids.push(fs_id);
            }
        }
        out.push(ShareSummary {
            share_id,
            surl,
            pwd,
            ctime: item.get("ctime").and_then(json_i64).unwrap_or(0),
            expired_type: item
                .get("expiredType")
                .or_else(|| item.get("expired_type"))
                .and_then(json_i64)
                .unwrap_or(0),
            expired_time: item
                .get("expiredTime")
                .or_else(|| item.get("expired_time"))
                .and_then(json_i64)
                .unwrap_or(0),
            status: item.get("status").and_then(json_i64).unwrap_or(0),
            names,
            fs_ids,
            shortlink,
        });
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct ShareBrowser {
    pub surl: String,
    pub share_id: i64,
    pub uk: i64,
    pub pwd: String,
    pub cwd: String,
    pub root_path: String,
    pub entries: Vec<Entry>,
    pub root_name: String,
    pub sekey: String,
    pub history: Vec<String>,
}

impl ShareBrowser {
    pub fn can_go_up(&self) -> bool {
        normalize_remote_path(&self.cwd) != normalize_remote_path(&self.root_path)
    }
}

pub fn parse_share_entries(value: &Value) -> Vec<Entry> {
    value
        .get("list")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .cloned()
                .filter_map(|item| serde_json::from_value::<Entry>(item).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn json_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}
