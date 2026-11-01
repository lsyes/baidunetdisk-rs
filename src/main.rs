//! Pure-Rust Baidu Netdisk client built on the kasumi immediate-mode GUI.

mod api;
mod app;
mod auth;
mod download;
mod events;
mod icons;
mod jobs;
mod media;
mod models;
mod pdf;
mod share;
mod theme;
mod ui;
mod upload;

use kasumi_backend::{WindowOptions, run};
use kasumi_core::Size;

fn main() -> anyhow::Result<()> {
    let options = WindowOptions {
        title: "百度网盘 · Baidu Netdisk RS".to_owned(),
        logical_size: Size::new(1280.0, 820.0),
        min_size: Some(Size::new(1000.0, 640.0)),
        theme: theme::baidu_theme(),
        vsync: true,
    };
    run(app::BaiduApp::new(), options)
}
