//! Loads the original Baidu file-type icons, UI icons and logo shipped with
//! the official 8.9.7 Linux client and rasterizes them once at startup.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kasumi::Svg;
use kasumi_core::ImageData;

use crate::models::FileKind;

const UI_ICONS: &[&str] = &[
    "add-folder",
    "arrow-down",
    "arrow-left",
    "arrow-right",
    "arrow-up",
    "audio",
    "back",
    "check",
    "close",
    "cloud",
    "collect",
    "copy",
    "create",
    "delete",
    "download",
    "download-bold",
    "edit",
    "error",
    "file",
    "file-upload",
    "folder",
    "forward",
    "history",
    "image",
    "img-list",
    "info",
    "link",
    "loading",
    "lock",
    "magnify",
    "more",
    "more-vertical",
    "open",
    "open-file",
    "pan-file",
    "pause",
    "pdf",
    "play",
    "plus",
    "qr-code",
    "quit",
    "refresh",
    "remove",
    "rename",
    "restore",
    "retry",
    "search",
    "setting",
    "share",
    "sort",
    "success",
    "suspend",
    "switch-space",
    "time",
    "transfer",
    "upload",
    "user",
    "video",
    "view-list",
    "view-norm",
    "warning",
];

pub struct Icons {
    file_icons: HashMap<String, Arc<ImageData>>,
    ui_icons: HashMap<String, Arc<ImageData>>,
    logo: Arc<ImageData>,
    blank: Arc<ImageData>,
}

impl Icons {
    pub fn load() -> Self {
        let root = assets_root();
        let blank = Arc::new(ImageData::from_rgba8(1, 1, vec![0, 0, 0, 0]));
        let mut file_icons = HashMap::new();
        let file_dir = root.join("icons/file");
        if let Ok(entries) = std::fs::read_dir(&file_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("svg") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };
                if let Some(image) = rasterize(&path, 96) {
                    file_icons.insert(stem.to_owned(), image);
                }
            }
        }
        let mut ui_icons = HashMap::new();
        let ui_dir = root.join("icons/ui");
        for name in UI_ICONS {
            let path = ui_dir.join(format!("{name}.svg"));
            if let Some(image) = rasterize(&path, 64) {
                ui_icons.insert((*name).to_owned(), image);
            }
        }
        let logo = rasterize(&root.join("branding/logo.svg"), 128)
            .or_else(|| rasterize_png(&root.join("branding/logo-32.png")))
            .unwrap_or_else(|| blank.clone());
        Self {
            file_icons,
            ui_icons,
            logo,
            blank,
        }
    }

    pub fn file_icon(&self, kind: FileKind) -> Arc<ImageData> {
        self.file_icons
            .get(kind.icon())
            .cloned()
            .unwrap_or_else(|| self.blank.clone())
    }

    pub fn ui(&self, name: &str) -> Arc<ImageData> {
        self.ui_icons
            .get(name)
            .cloned()
            .unwrap_or_else(|| self.blank.clone())
    }

    pub fn has_ui(&self, name: &str) -> bool {
        self.ui_icons.contains_key(name)
    }

    pub fn logo(&self) -> Arc<ImageData> {
        self.logo.clone()
    }

    pub fn blank(&self) -> Arc<ImageData> {
        self.blank.clone()
    }

    pub fn load_ui_icon(&mut self, name: &str) -> Arc<ImageData> {
        if let Some(icon) = self.ui_icons.get(name) {
            return icon.clone();
        }
        let path = assets_root().join("icons/ui").join(format!("{name}.svg"));
        let image = rasterize(&path, 64).unwrap_or_else(|| self.blank.clone());
        self.ui_icons.insert(name.to_owned(), image.clone());
        image
    }
}

pub fn assets_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("BAIDUNETDISK_ASSETS") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = installed_assets_root("lib/baidunetdisk-rs/assets") {
        return dir;
    }
    let cwd = PathBuf::from("assets");
    if cwd.join("icons").is_dir() {
        return cwd;
    }
    installed_assets_root("assets")
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"))
}

/// Answers where the install prefix keeps the asset tree, or `None` when the
/// executable does not live in an install prefix that has one. Packages
/// installed by a release build carry no fallback path, so this is the only
/// place they can find the icons (see `debian/install`).
///
/// * `usr/bin/baidunetdisk-rs` -> `usr/` + `relative` (the Debian layout).
/// * any other location        -> `<dir of the binary>/` + `relative`.
fn installed_assets_root(relative: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let dir = exe.parent()?;
    let candidates = [dir.parent().map(|prefix| prefix.join(relative)), Some(dir.join(relative))];
    candidates
        .into_iter()
        .flatten()
        .find(|candidate| candidate.join("icons").is_dir())
}

fn rasterize(path: &Path, size: u32) -> Option<Arc<ImageData>> {
    if !path.is_file() {
        return None;
    }
    let mut svg = Svg::from_path(path).ok()?;
    svg = svg.render_size(size, size);
    svg.rasterize().ok().map(Arc::new)
}

fn rasterize_png(path: &Path) -> Option<Arc<ImageData>> {
    let bytes = std::fs::read(path).ok()?;
    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(Arc::new(ImageData::from_rgba8(
        width,
        height,
        image.into_raw(),
    )))
}
