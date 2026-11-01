//! Application shell: persistent state, background events and top-level layout.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use kasumi::prelude::*;
use kasumi::{App, Image};
use kasumi_core::{Color, ImageData, Key, Length, TextWrap};

use crate::api::{ApiClient, join_remote, normalize_remote_path};
use crate::download::{DownloadRequest, planned_path};
use crate::events::{Event, PreviewContent, QrState};
use crate::icons::Icons;
use crate::jobs::{Control, Progress, TaskStatus};
use crate::models::{
    Entry, FileKind, Quota, SessionFile, Settings, UserInfo, config_dir, format_timestamp,
    human_size, human_speed, session_path, settings_path,
};
use crate::share::{ShareBrowser, ShareSummary, parse_share_entries, parse_share_summaries};
use crate::ui::components::{self, ActionStyle};
use crate::download::sanitize_file_name;
use crate::upload::{UploadRequest, run_upload};

/// Width, in logical pixels, that a PDF page is rasterized at before zoom is
/// applied. The preview scales the texture to `PDF_BASE_WIDTH * zoom`, which
/// keeps the first (fit-to-viewport) frame sharp even though it is rendered
/// before the viewport size is known.
pub const PDF_BASE_WIDTH: f32 = 1200.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Splash,
    Login,
    Files,
    Transfers,
    Shares,
    Settings,
    Preview,
    OpenShare,
    ShareBrowser,
    LocalPicker,
    Dialog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Files,
    Transfers,
    Shares,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginTab {
    Qr,
    Account,
    Cookie,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferTab {
    Downloads,
    Uploads,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Time,
    Kind,
}

impl SortKey {
    pub fn api_order(self) -> &'static str {
        match self {
            Self::Name | Self::Kind => "name",
            Self::Size => "size",
            Self::Time => "time",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "名称",
            Self::Size => "大小",
            Self::Time => "修改时间",
            Self::Kind => "类型",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadState {
    Idle,
    Loading,
    Ready,
    Failed,
}

#[derive(Clone)]
pub struct Inputs {
    pub search: State<String>,
    pub account: State<String>,
    pub password: State<String>,
    pub vcode: State<String>,
    pub cookie: State<String>,
    pub rename: State<String>,
    pub new_folder: State<String>,
    pub share_pwd: State<String>,
    pub share_link: State<String>,
    pub share_link_pwd: State<String>,
    pub move_dest: State<String>,
    pub settings_dir: State<String>,
}

#[derive(Debug, Clone, Default)]
pub struct LoginState {
    pub tab: LoginTab,
    pub busy: bool,
    pub qr_image: Option<Arc<ImageData>>,
    pub qr_state: QrState,
    pub qr_message: String,
    pub error: Option<String>,
    pub captcha: Option<Arc<ImageData>>,
}

impl Default for LoginTab {
    fn default() -> Self {
        Self::Qr
    }
}

impl Default for QrState {
    fn default() -> Self {
        Self::Loading
    }
}

#[derive(Clone)]
pub struct DownloadJob {
    pub id: u64,
    pub entry: Entry,
    pub final_path: PathBuf,
    pub progress: Arc<Progress>,
    pub control: Arc<Control>,
    pub status: TaskStatus,
    pub message: String,
    pub note: String,
    pub speed: f64,
    pub last_done: u64,
    pub last_sample: Instant,
    pub started: Instant,
}

#[derive(Clone)]
pub struct UploadJob {
    pub id: u64,
    pub local_path: PathBuf,
    pub remote_dir: String,
    pub progress: Arc<Progress>,
    pub control: Arc<Control>,
    pub status: TaskStatus,
    pub message: String,
    pub note: String,
    pub speed: f64,
    pub last_done: u64,
    pub last_sample: Instant,
    pub started: Instant,
}

#[derive(Clone)]
pub struct PreviewState {
    pub entry: Entry,
    pub content: Option<PreviewContent>,
    pub loading: bool,
    pub error: Option<String>,
    pub zoom: f32,
    pub font_size: f32,
    pub pdf_page: usize,
    pub pdf_pages: usize,
    pub pdf_bytes: Option<Arc<Vec<u8>>>,
    pub pdf_loading: bool,
    /// The first rendered page has been fitted to the preview viewport.
    pub pdf_fitted: bool,
    pub text_offset: u64,
    pub text_limit: u64,
    pub total: u64,
}

#[derive(Clone, Debug)]
pub struct LocalEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: i64,
}

#[derive(Clone)]
pub struct PickerState {
    pub cwd: PathBuf,
    pub entries: Vec<LocalEntry>,
    pub selected: BTreeSet<PathBuf>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    NewFolder,
    Rename,
    Delete,
    Share,
    Move,
    Copy,
    Actions,
    Details,
}

#[derive(Clone)]
pub struct DialogState {
    pub kind: DialogKind,
    pub targets: Vec<Entry>,
    pub title: String,
    pub message: String,
    pub period: u32,
    pub public: bool,
}

#[derive(Clone)]
pub struct Toast {
    pub text: String,
    pub error: bool,
    pub born: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SearchResults {
    pub request: u64,
}

pub enum Action {
    Navigate(String),
    Back,
    Forward,
    Up,
    Refresh,
    Select { fs_id: i64, toggle: bool },
    Open(Entry),
    Download(Vec<Entry>),
    ShareDialog(Vec<Entry>),
    DeleteDialog(Vec<Entry>),
    RenameDialog(Entry),
    MoveDialog(Vec<Entry>),
    CopyDialog(Vec<Entry>),
    NewFolder,
    Details(Entry),
    Actions(Entry),
    Sort(SortKey),
    ToggleGrid,
    SetPage(Page),
    SetNav(Nav),
    SaveSettings,
    Logout,
    StartUpload(PathBuf),
    OpenPicker,
    PickerEnter(PathBuf),
    PickerToggle(PathBuf),
    PickerUpload,
    PickerUploadDir(PathBuf),
    OpenShareLink,
    OpenShareSummary(ShareSummary),
    ShareBrowserEnter(String),
    RefreshShares,
    CancelShare(i64),
    CopyText(String),
    PauseDownload(u64),
    ResumeDownload(u64),
    CancelDownload(u64),
    RemoveDownload(u64),
    RetryDownload(u64),
    OpenDownloadFolder(u64),
    OpenDownloadFile(u64),
    ClearFinishedTasks,
    PauseUpload(u64),
    ResumeUpload(u64),
    CancelUpload(u64),
    RemoveUpload(u64),
    OpenUploadFolder(u64),
    CloseDialog,
    ConfirmDialog,
    ToggleLoginTab(LoginTab),
    LoginQrRefresh,
    LoginAccount,
    LoginCookie,
    LogoutAndExit,
    ClosePreview,
    PdfPrev,
    PdfNext,
    PdfZoom(f32),
    PreviewFontSize(f32),
    PreviewPagePrev,
    PreviewPageNext,
    TransferShare(Vec<Entry>),
    ShareBrowserOpen(Entry),
    ShareBrowserUp,
}

pub struct BaiduApp {
    pub icons: Icons,
    pub theme: Theme,
    pub page: Page,
    pub nav: Nav,
    pub client: Option<Arc<ApiClient>>,
    pub session: Option<SessionFile>,
    pub user: UserInfo,
    pub quota: Quota,
    pub inputs: Option<Inputs>,
    pub settings: Settings,
    pub login: LoginState,
    pub qr_started: bool,
    pub cwd: String,
    pub entries: Vec<Entry>,
    pub load_state: LoadState,
    pub load_error: Option<String>,
    pub request_seq: u64,
    pub loaded_path: String,
    pub selected: BTreeSet<i64>,
    pub active: Option<i64>,
    pub last_click: Option<(i64, f64)>,
    pub history: Vec<String>,
    pub history_pos: usize,
    pub search_keyword: Option<String>,
    pub search_entries: Vec<Entry>,
    pub thumbnails: HashMap<i64, Arc<ImageData>>,
    pub thumb_pending: HashSet<i64>,
    pub thumb_failed: HashSet<i64>,
    pub my_shares: Vec<ShareSummary>,
    pub shares_state: LoadState,
    pub shares_error: Option<String>,
    pub shares_seq: u64,
    pub share_browser: Option<ShareBrowser>,
    pub share_browse_state: LoadState,
    pub share_browse_error: Option<String>,
    pub share_browse_seq: u64,
    pub downloads: Vec<DownloadJob>,
    pub uploads: Vec<UploadJob>,
    pub transfer_tab: TransferTab,
    pub next_task_id: u64,
    pub preview: Option<PreviewState>,
    pub preview_seq: u64,
    pub picker: Option<PickerState>,
    pub dialog: Option<DialogState>,
    pub actions: Vec<Action>,
    pub tx: Sender<Event>,
    pub rx: Receiver<Event>,
    pub pending: usize,
    pub toasts: Vec<Toast>,
    pub now: f64,
    pub clipboard: Option<arboard::Clipboard>,
    pub last_dir_refresh: Instant,
    pub pending_search: Option<String>,
    pub logout_armed: bool,
}

impl BaiduApp {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        let settings = load_settings();
        let session = load_session();
        let mut app = Self {
            icons: Icons::load(),
            theme: crate::theme::baidu_theme(),
            page: Page::Splash,
            nav: Nav::Files,
            client: None,
            session: session.clone(),
            user: UserInfo::default(),
            quota: Quota::default(),
            inputs: None,
            settings,
            login: LoginState::default(),
            qr_started: false,
            cwd: "/".to_owned(),
            entries: Vec::new(),
            load_state: LoadState::Idle,
            load_error: None,
            request_seq: 0,
            loaded_path: String::new(),
            selected: BTreeSet::new(),
            active: None,
            last_click: None,
            history: vec!["/".to_owned()],
            history_pos: 0,
            search_keyword: None,
            search_entries: Vec::new(),
            thumbnails: HashMap::new(),
            thumb_pending: HashSet::new(),
            thumb_failed: HashSet::new(),
            my_shares: Vec::new(),
            shares_state: LoadState::Idle,
            shares_error: None,
            shares_seq: 0,
            share_browser: None,
            share_browse_state: LoadState::Idle,
            share_browse_error: None,
            share_browse_seq: 0,
            downloads: Vec::new(),
            uploads: Vec::new(),
            transfer_tab: TransferTab::Downloads,
            next_task_id: 1,
            preview: None,
            preview_seq: 0,
            picker: None,
            dialog: None,
            actions: Vec::new(),
            tx,
            rx,
            pending: 0,
            toasts: Vec::new(),
            now: 0.0,
            clipboard: None,
            last_dir_refresh: Instant::now(),
            pending_search: None,
            logout_armed: false,
        };
        if let Some(session) = session {
            app.check_saved_session(session);
        } else {
            app.page = Page::Login;
        }
        app
    }

    fn check_saved_session(&mut self, session: SessionFile) {
        let cookie = session.cookie.clone();
        self.page = Page::Splash;
        self.spawn_job(move |tx| {
            let result = (|| {
                let mut client = ApiClient::new(&cookie)
                    .map_err(|error| format!("登录信息无效：{error}"))?;
                let user = client.whoami().map_err(|error| error.to_string())?;
                client.set_uk(user.uk);
                let quota = client.quota().map_err(|error| error.to_string())?;
                let file = client.session_file(&user);
                Ok((user, quota, file))
            })();
            let _ = tx.send(Event::SessionChecked { result });
        });
    }

    fn spawn_job<F>(&mut self, f: F)
    where
        F: FnOnce(Sender<Event>) + Send + 'static,
    {
        self.pending += 1;
        let tx = self.tx.clone();
        let guard_tx = tx.clone();
        std::thread::spawn(move || {
            struct Guard(Sender<Event>);
            impl Drop for Guard {
                fn drop(&mut self) {
                    let _ = self.0.send(Event::JobDone);
                }
            }
            let _guard = Guard(guard_tx);
            f(tx);
        });
    }

    pub fn client(&self) -> Option<&Arc<ApiClient>> {
        self.client.as_ref()
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn is_authenticated(&self) -> bool {
        self.client.is_some()
    }

    pub fn ensure_inputs(&mut self, ui: &mut Ui) {
        if self.inputs.is_some() {
            return;
        }
        self.inputs = Some(Inputs {
            search: ui.state_persistent(String::new),
            account: ui.state_persistent(String::new),
            password: ui.state_persistent(String::new),
            vcode: ui.state_persistent(String::new),
            cookie: ui.state_persistent(String::new),
            rename: ui.state_persistent(String::new),
            new_folder: ui.state_persistent(String::new),
            share_pwd: ui.state_persistent(String::new),
            share_link: ui.state_persistent(String::new),
            share_link_pwd: ui.state_persistent(String::new),
            move_dest: ui.state_persistent(String::new),
            settings_dir: ui.state_persistent(String::new),
        });
        if let Some(inputs) = &self.inputs {
            inputs.settings_dir.set(self.settings.download_dir.clone());
        }
    }

    pub fn inputs(&self) -> &Inputs {
        self.inputs.as_ref().expect("inputs initialized in view")
    }

    // ------------------------------------------------------------------
    // Notifications and clipboard
    // ------------------------------------------------------------------

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        if self.toasts.len() >= 4 {
            self.toasts.remove(0);
        }
        self.toasts.push(Toast {
            text: text.into(),
            error,
            born: self.now,
        });
    }

    pub fn copy_text(&mut self, text: &str) {
        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new().ok();
        }
        let copied = self
            .clipboard
            .as_mut()
            .map(|clipboard| clipboard.set_text(text.to_owned()).is_ok())
            .unwrap_or(false);
        if copied {
            self.toast("已复制到剪贴板", false);
        } else {
            self.toast("无法访问系统剪贴板", true);
        }
    }

    // ------------------------------------------------------------------
    // Session
    // ------------------------------------------------------------------

    fn install_session(&mut self, session: SessionFile) {
        match ApiClient::from_session(&session) {
            Ok(client) => {
                self.user = UserInfo::from(&session.user);
                self.client = Some(Arc::new(client));
                self.session = Some(session.clone());
                save_session(&session);
                self.page = Page::Files;
                self.nav = Nav::Files;
                self.cwd = "/".to_owned();
                self.history = vec!["/".to_owned()];
                self.history_pos = 0;
                self.load_dir("/".to_owned());
                self.load_shares();
            }
            Err(error) => {
                self.toast(error.to_string(), true);
            }
        }
    }

    fn logout(&mut self) {
        let _ = std::fs::remove_file(session_path());
        self.client = None;
        self.session = None;
        self.user = UserInfo::default();
        self.quota = Quota::default();
        self.entries.clear();
        self.downloads.clear();
        self.uploads.clear();
        self.share_browser = None;
        self.page = Page::Login;
        self.nav = Nav::Files;
        self.qr_started = false;
        self.toast("已退出登录", false);
    }

    // ------------------------------------------------------------------
    // Directory browsing
    // ------------------------------------------------------------------

    pub fn load_dir(&mut self, path: String) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let path = normalize_remote_path(&path);
        self.request_seq += 1;
        let request = self.request_seq;
        self.load_state = LoadState::Loading;
        self.load_error = None;
        self.selected.clear();
        self.active = None;
        self.search_keyword = None;
        self.search_entries.clear();
        self.thumbnails.clear();
        self.thumb_pending.clear();
        self.thumb_failed.clear();
        let order = self.sort_key().api_order();
        let desc = self.sort_desc();
        let target = path.clone();
        self.spawn_job(move |tx| {
            let result = client.list_all(&target, order, desc);
            let (entries, error) = match result {
                Ok(entries) => (entries, None),
                Err(error) => (Vec::new(), Some(error.to_string())),
            };
            let _ = tx.send(Event::DirLoaded {
                request,
                path: target,
                entries,
                error,
            });
        });
    }

    pub fn refresh_current(&mut self) {
        if let Some(keyword) = self.search_keyword.clone() {
            self.search(keyword);
        } else {
            self.load_dir(self.cwd.clone());
        }
    }

    pub fn navigate(&mut self, path: String) {
        let path = normalize_remote_path(&path);
        if path == self.cwd && self.load_state != LoadState::Loading {
            self.refresh_current();
            return;
        }
        if self.history_pos + 1 < self.history.len() {
            self.history.truncate(self.history_pos + 1);
        }
        if self.history.get(self.history_pos) != Some(&path) {
            self.history.push(path.clone());
            self.history_pos = self.history.len() - 1;
        }
        self.cwd = path.clone();
        self.load_dir(path);
    }

    pub fn go_back(&mut self) {
        if self.history_pos > 0 {
            self.history_pos -= 1;
            if let Some(path) = self.history.get(self.history_pos).cloned() {
                self.cwd = path.clone();
                self.load_dir(path);
            }
        }
    }

    pub fn go_forward(&mut self) {
        if self.history_pos + 1 < self.history.len() {
            self.history_pos += 1;
            if let Some(path) = self.history.get(self.history_pos).cloned() {
                self.cwd = path.clone();
                self.load_dir(path);
            }
        }
    }

    pub fn go_up(&mut self) {
        let parent = crate::api::parent_remote(&self.cwd);
        if parent != self.cwd {
            self.navigate(parent);
        }
    }

    pub fn search(&mut self, keyword: String) {
        let keyword = keyword.trim().to_owned();
        if keyword.is_empty() {
            self.search_keyword = None;
            self.search_entries.clear();
            self.load_dir(self.cwd.clone());
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.request_seq += 1;
        let request = self.request_seq;
        self.load_state = LoadState::Loading;
        self.load_error = None;
        self.search_keyword = Some(keyword.clone());
        self.selected.clear();
        self.thumbnails.clear();
        self.thumb_pending.clear();
        self.thumb_failed.clear();
        self.spawn_job(move |tx| {
            let result = client.search(&keyword, "/");
            let (entries, error) = match result {
                Ok(entries) => (entries, None),
                Err(error) => (Vec::new(), Some(error.to_string())),
            };
            let _ = tx.send(Event::SearchLoaded {
                request,
                keyword,
                entries,
                error,
            });
        });
    }

    pub fn sort_key(&self) -> SortKey {
        match self.settings.sort_key.as_str() {
            "size" => SortKey::Size,
            "time" => SortKey::Time,
            "kind" => SortKey::Kind,
            _ => SortKey::Name,
        }
    }

    pub fn sort_desc(&self) -> bool {
        self.settings.sort_desc
    }

    pub fn sorted_entries(&self, entries: &[Entry]) -> Vec<Entry> {
        let mut out = entries.to_vec();
        let key = self.sort_key();
        let desc = self.sort_desc();
        out.sort_by(|a, b| {
            let dir = b.is_dir().cmp(&a.is_dir());
            if dir != std::cmp::Ordering::Equal {
                return dir;
            }
            let ordering = match key {
                SortKey::Name => natural_cmp(&a.server_filename, &b.server_filename),
                SortKey::Size => a.size.cmp(&b.size),
                SortKey::Time => a.modified().cmp(&b.modified()),
                SortKey::Kind => a
                    .kind()
                    .label()
                    .cmp(b.kind().label())
                    .then_with(|| natural_cmp(&a.server_filename, &b.server_filename)),
            };
            if desc { ordering.reverse() } else { ordering }
        });
        out
    }

    pub fn visible_entries(&self) -> Vec<Entry> {
        if self.search_keyword.is_some() {
            self.sorted_entries(&self.search_entries)
        } else {
            self.sorted_entries(&self.entries)
        }
    }

    pub fn selected_entries(&self) -> Vec<Entry> {
        self.visible_entries()
            .into_iter()
            .filter(|entry| self.selected.contains(&entry.fs_id))
            .collect()
    }

    pub fn entry_by_id(&self, fs_id: i64) -> Option<Entry> {
        self.visible_entries()
            .into_iter()
            .find(|entry| entry.fs_id == fs_id)
    }

    pub fn request_thumbnails(&mut self) {
        const THUMBNAIL_LIMIT: usize = 120;
        let Some(client) = self.client.clone() else {
            return;
        };
        let in_flight = self.thumbnails.len() + self.thumb_pending.len();
        if in_flight >= THUMBNAIL_LIMIT {
            return;
        }
        let entries: Vec<Entry> = self
            .visible_entries()
            .into_iter()
            .filter(|entry| entry.kind() == FileKind::Image)
            .filter(|entry| !self.thumbnails.contains_key(&entry.fs_id))
            .filter(|entry| !self.thumb_pending.contains(&entry.fs_id))
            .filter(|entry| !self.thumb_failed.contains(&entry.fs_id))
            .take((THUMBNAIL_LIMIT - in_flight).min(36))
            .collect();
        for entry in entries {
            self.thumb_pending.insert(entry.fs_id);
            let client = client.clone();
            let fs_id = entry.fs_id;
            let path = entry.path.clone();
            self.spawn_job(move |tx| {
                let data = client
                    .thumbnail(&path, 320, 240)
                    .ok()
                    .and_then(|bytes| crate::media::decode_image(&bytes, 512));
                let _ = tx.send(Event::Thumbnail { fs_id, data });
            });
        }
    }

    // ------------------------------------------------------------------
    // Sharing
    // ------------------------------------------------------------------

    pub fn load_shares(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.shares_seq += 1;
        let request = self.shares_seq;
        self.shares_state = LoadState::Loading;
        self.shares_error = None;
        self.spawn_job(move |tx| {
            let result = client.my_shares(1, 100);
            let (shares, error) = match result {
                Ok(value) => (parse_share_summaries(&value), None),
                Err(error) => (Vec::new(), Some(error.to_string())),
            };
            let _ = tx.send(Event::SharesLoaded {
                request,
                shares,
                error,
            });
        });
    }

    pub fn open_share_link(&mut self, input: String, pwd: String) {
        let Some((token, parsed_pwd)) = crate::api::parse_share_link(&input) else {
            self.toast("分享链接格式不正确", true);
            return;
        };
        let pwd = if pwd.trim().is_empty() {
            parsed_pwd.unwrap_or_default()
        } else {
            pwd.trim().to_owned()
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        let surl = token.strip_prefix('1').unwrap_or(&token).to_owned();
        self.spawn_job(move |tx| {
            let result = (|| {
                let page = client.share_page_info(&surl)?;
                let mut sekey = String::new();
                if !pwd.is_empty() {
                    let randsk = client.share_verify(&surl, &pwd)?;
                    if !randsk.is_empty() {
                        client.merge_cookie(&format!("BDCLND={randsk}"));
                        sekey = randsk;
                    }
                }
                let (root_path, entries) = if page.page_type == "multi"
                    && page.entries.len() == 1
                    && page.entries[0].is_dir()
                {
                    let root = page.entries[0].clone();
                    let root_path = if root.path.starts_with('/') {
                        root.path.clone()
                    } else {
                        format!("/{}", root.server_filename)
                    };
                    let value = client.share_list(
                        &surl,
                        page.share_id,
                        page.uk,
                        &root_path,
                    )?;
                    (root_path, parse_share_entries(&value))
                } else {
                    ("/".to_owned(), page.entries)
                };
                Ok::<_, crate::api::ApiError>(ShareBrowser {
                    surl,
                    share_id: page.share_id,
                    uk: page.uk,
                    pwd,
                    cwd: root_path.clone(),
                    root_path,
                    entries,
                    root_name: page.title,
                    sekey,
                    history: Vec::new(),
                })
            })();
            match result {
                Ok(browser) => {
                    let _ = tx.send(Event::ShareOpened {
                        request: 0,
                        browser,
                    });
                }
                Err(error) => {
                    let _ = tx.send(Event::Notice {
                        text: format!("打开分享失败：{error}"),
                        error: true,
                    });
                }
            }
        });
    }

    pub fn load_share_browser_dir(&mut self, dir: String) {
        let Some(browser) = self.share_browser.clone() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        self.share_browse_seq += 1;
        let request = self.share_browse_seq;
        self.share_browse_state = LoadState::Loading;
        self.share_browse_error = None;
        let dir = normalize_remote_path(&dir);
        self.spawn_job(move |tx| {
            let result = client
                .share_list(&browser.surl, browser.share_id, browser.uk, &dir)
                .map(|value| parse_share_entries(&value));
            match result {
                Ok(entries) => {
                    let mut browser = browser;
                    browser.cwd = dir;
                    browser.entries = entries;
                    let _ = tx.send(Event::ShareBrowseLoaded {
                        request,
                        browser,
                        error: None,
                    });
                }
                Err(error) => {
                    let _ = tx.send(Event::ShareBrowseLoaded {
                        request,
                        browser,
                        error: Some(error.to_string()),
                    });
                }
            }
        });
    }

    pub fn cancel_share(&mut self, share_id: i64) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.spawn_job(move |tx| {
            let result = client.cancel_share(&[share_id]);
            let failed = result.is_err();
            let message = match result {
                Ok(_) => "已取消该分享".to_owned(),
                Err(error) => format!("取消分享失败：{error}"),
            };
            let _ = tx.send(Event::OperationDone {
                message,
                error: failed,
            });
        });
    }

    // ------------------------------------------------------------------
    // Downloads and uploads
    // ------------------------------------------------------------------

    fn active_downloads(&self) -> usize {
        self.downloads
            .iter()
            .filter(|job| job.status == TaskStatus::Running)
            .count()
    }

    pub fn start_download(&mut self, entry: Entry) {
        if entry.is_dir() {
            self.start_folder_download(entry);
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let dir = PathBuf::from(&self.settings.download_dir);
        if let Err(error) = std::fs::create_dir_all(&dir) {
            self.toast(format!("无法创建下载目录：{error}"), true);
            return;
        }
        let final_path = planned_path(&dir, &entry.server_filename);
        let id = self.next_task_id;
        self.next_task_id += 1;
        let progress = Arc::new(Progress::new(entry.size));
        let control = Arc::new(Control::default());
        let job = DownloadJob {
            id,
            entry: entry.clone(),
            final_path: final_path.clone(),
            progress: progress.clone(),
            control: control.clone(),
            status: TaskStatus::Queued,
            message: "等待开始".to_owned(),
            note: String::new(),
            speed: 0.0,
            last_done: 0,
            last_sample: Instant::now(),
            started: Instant::now(),
        };
        self.downloads.push(job);
        self.pump_downloads();
        let _ = client;
        self.toast(format!("已加入下载队列：{}", entry.server_filename), false);
    }

    fn start_folder_download(&mut self, entry: Entry) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.toast(format!("正在展开文件夹：{}", entry.server_filename), false);
        self.spawn_job(move |tx| {
            let mut files = Vec::new();
            let mut error = None;
            if let Err(api_error) = collect_remote_files(&client, &entry.path, &mut files) {
                error = Some(api_error.to_string());
            }
            let _ = tx.send(Event::DownloadBatch {
                entries: files,
                error,
            });
        });
    }

    pub fn pump_downloads(&mut self) {
        let max_active = self.settings.max_active_downloads.max(1);
        while self.active_downloads() < max_active {
            let Some(index) = self
                .downloads
                .iter()
                .position(|job| job.status == TaskStatus::Queued)
            else {
                break;
            };
            let Some(client) = self.client.clone() else {
                break;
            };
            let job = &mut self.downloads[index];
            job.status = TaskStatus::Running;
            job.message = "连接中".to_owned();
            job.started = Instant::now();
            job.last_sample = Instant::now();
            let request = DownloadRequest {
                id: job.id,
                entry: job.entry.clone(),
                final_path: job.final_path.clone(),
                threads: self.settings.download_threads,
                verify_md5: self.settings.verify_md5,
            };
            let progress = job.progress.clone();
            let control = job.control.clone();
            self.spawn_job(move |tx| {
                crate::download::run_download(client, request, progress, control, tx);
            });
        }
    }

    pub fn start_upload(&mut self, path: PathBuf) {
        let Some(_client) = self.client.clone() else {
            return;
        };
        if !path.exists() {
            self.toast("本地路径不存在", true);
            return;
        }
        let id = self.next_task_id;
        self.next_task_id += 1;
        let progress = Arc::new(Progress::default());
        let control = Arc::new(Control::default());
        let remote_dir = self.cwd.clone();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("未命名")
            .to_owned();
        self.uploads.push(UploadJob {
            id,
            local_path: path.clone(),
            remote_dir: remote_dir.clone(),
            progress: progress.clone(),
            control: control.clone(),
            status: TaskStatus::Running,
            message: "准备上传".to_owned(),
            note: name.clone(),
            speed: 0.0,
            last_done: 0,
            last_sample: Instant::now(),
            started: Instant::now(),
        });
        let Some(client) = self.client.clone() else {
            return;
        };
        let request = UploadRequest {
            id,
            local_path: path,
            remote_dir,
            threads: self.settings.upload_threads,
        };
        self.spawn_job(move |tx| {
            run_upload(client, request, progress, control, tx);
        });
        self.toast(format!("开始上传：{name}"), false);
    }

    pub fn start_uploads(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            self.start_upload(path);
        }
    }

    pub fn pause_download(&mut self, id: u64) {
        if let Some(job) = self.downloads.iter_mut().find(|job| job.id == id) {
            job.control.pause();
            job.status = TaskStatus::Paused;
            job.message = "已暂停".to_owned();
        }
    }

    pub fn resume_download(&mut self, id: u64) {
        if let Some(job) = self.downloads.iter_mut().find(|job| job.id == id) {
            job.control.resume();
            job.status = TaskStatus::Running;
            job.message = "继续下载".to_owned();
            job.last_sample = Instant::now();
            job.last_done = job.progress.done();
        }
    }

    pub fn cancel_download(&mut self, id: u64) {
        if let Some(job) = self.downloads.iter_mut().find(|job| job.id == id) {
            job.control.cancel();
            job.status = TaskStatus::Cancelled;
            job.message = "已取消".to_owned();
        }
    }

    pub fn remove_download(&mut self, id: u64) {
        self.downloads
            .retain(|job| job.id != id || !job.status.is_terminal());
    }

    pub fn retry_download(&mut self, id: u64) {
        if let Some(job) = self.downloads.iter().find(|job| job.id == id).cloned() {
            self.downloads.retain(|candidate| candidate.id != id);
            self.start_download(job.entry);
        }
    }

    pub fn pause_upload(&mut self, id: u64) {
        if let Some(job) = self.uploads.iter_mut().find(|job| job.id == id) {
            job.control.pause();
            job.status = TaskStatus::Paused;
            job.message = "已暂停".to_owned();
        }
    }

    pub fn resume_upload(&mut self, id: u64) {
        if let Some(job) = self.uploads.iter_mut().find(|job| job.id == id) {
            job.control.resume();
            job.status = TaskStatus::Running;
            job.message = "继续上传".to_owned();
            job.last_sample = Instant::now();
            job.last_done = job.progress.done();
        }
    }

    pub fn cancel_upload(&mut self, id: u64) {
        if let Some(job) = self.uploads.iter_mut().find(|job| job.id == id) {
            job.control.cancel();
            job.status = TaskStatus::Cancelled;
            job.message = "已取消".to_owned();
        }
    }

    pub fn remove_upload(&mut self, id: u64) {
        self.uploads
            .retain(|job| job.id != id || !job.status.is_terminal());
    }

    // ------------------------------------------------------------------
    // Preview
    // ------------------------------------------------------------------

    pub fn open_preview(&mut self, entry: Entry) {
        if entry.is_dir() {
            self.navigate(entry.path.clone());
            return;
        }
        let kind = entry.kind();
        if !matches!(
            kind,
            FileKind::Image
                | FileKind::Pdf
                | FileKind::Markdown
                | FileKind::Text
                | FileKind::Code
                | FileKind::Other
                | FileKind::Audio
                | FileKind::Video
        ) {
            self.open_details(entry);
            return;
        }
        self.preview = Some(PreviewState {
            entry: entry.clone(),
            content: None,
            loading: true,
            error: None,
            zoom: 1.0,
            font_size: 14.0,
            pdf_page: 0,
            pdf_pages: 0,
            pdf_bytes: None,
            pdf_loading: false,
            pdf_fitted: false,
            text_offset: 0,
            text_limit: preview_page_size(kind),
            total: entry.size,
        });
        self.page = Page::Preview;
        self.begin_preview_load(0);
    }

    fn begin_preview_load(&mut self, offset: u64) {
        let Some(preview) = self.preview.clone() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        let entry = preview.entry.clone();
        let kind = entry.kind();
        let limit = if preview.text_limit == 0 {
            preview_page_size(kind)
        } else {
            preview.text_limit
        };
        self.preview_seq += 1;
        let request = self.preview_seq;
        if let Some(preview) = &mut self.preview {
            preview.loading = true;
            preview.error = None;
            preview.text_offset = offset;
            preview.text_limit = limit;
        }
        eprintln!("[preview] begin {} offset={offset} limit={limit}", entry.path);
        self.spawn_job(move |tx| {
            eprintln!("[preview] worker start {}", entry.path);
            let result = load_preview(&client, &entry, kind, offset, limit);
            eprintln!("[preview] worker done {:?}", result.as_ref().map(|_| "ok").map_err(|e| e.clone()));
            match result {
                Ok(PreviewLoad::Content(content)) => {
                    let _ = tx.send(Event::PreviewLoaded {
                        request,
                        content,
                        entry,
                    });
                }
                Ok(PreviewLoad::PdfDocument { pages, bytes }) => {
                    let _ = tx.send(Event::PdfDocument {
                        request,
                        pages,
                        bytes,
                    });
                }
                Err(error) => {
                    let _ = tx.send(Event::PreviewFailed {
                        request,
                        message: error,
                    });
                }
            }
        });
    }

    pub fn open_details(&mut self, entry: Entry) {
        self.dialog = Some(DialogState {
            kind: DialogKind::Details,
            targets: vec![entry],
            title: "详细信息".to_owned(),
            message: String::new(),
            period: 0,
            public: true,
        });
        self.page = Page::Dialog;
    }

    // ------------------------------------------------------------------
    // Events
    // ------------------------------------------------------------------

    fn poll_events(&mut self, ui: &mut Ui) {
        while let Ok(event) = self.rx.try_recv() {
            if matches!(event, Event::JobDone) {
                self.pending = self.pending.saturating_sub(1);
                continue;
            }
            self.handle_event(event, ui);
        }
    }

    fn handle_event(&mut self, event: Event, ui: &mut Ui) {
        match event {
            Event::JobDone => {}
            Event::Notice { text, error } => self.toast(text, error),
            Event::SessionChecked { result } => match result {
                Ok((user, quota, session)) => {
                    self.user = user;
                    self.quota = quota;
                    self.install_session(session);
                    self.toast(
                        format!("欢迎回来，{}", self.user.display_name()),
                        false,
                    );
                }
                Err(error) => {
                    self.page = Page::Login;
                    self.login.error = Some(error.clone());
                    self.toast(format!("会话已失效：{error}"), true);
                    self.qr_started = false;
                }
            },
            Event::LoginQr { image, message } => {
                self.login.qr_image = Some(image);
                self.login.qr_message = message;
                self.login.qr_state = QrState::Waiting;
                self.login.busy = false;
            }
            Event::LoginQrStatus { state, message } => {
                self.login.qr_state = state;
                self.login.qr_message = message;
            }
            Event::LoginOk {
                session,
                user,
                quota,
            } => {
                self.user = user;
                self.quota = quota;
                self.login.busy = false;
                self.install_session(session);
                self.toast("登录成功", false);
            }
            Event::LoginFailed { message, captcha } => {
                self.login.busy = false;
                self.login.error = Some(message.clone());
                self.login.captcha = captcha;
                self.toast(message, true);
            }
            Event::DirLoaded {
                request,
                path,
                entries,
                error,
            } => {
                if request != self.request_seq {
                    return;
                }
                self.load_state = if error.is_some() {
                    LoadState::Failed
                } else {
                    LoadState::Ready
                };
                self.load_error = error;
                self.entries = entries;
                self.loaded_path = path;
                self.last_dir_refresh = Instant::now();
                self.request_thumbnails();
                ui.request_repaint();
            }
            Event::SearchLoaded {
                request,
                keyword,
                entries,
                error,
            } => {
                if request != self.request_seq {
                    return;
                }
                if let Some(message) = error {
                    self.load_state = LoadState::Failed;
                    self.load_error = Some(message.clone());
                    self.toast(message, true);
                } else {
                    self.load_state = LoadState::Ready;
                    self.search_keyword = Some(keyword);
                    self.search_entries = entries;
                    self.request_thumbnails();
                }
                ui.request_repaint();
            }
            Event::Thumbnail { fs_id, data } => {
                self.thumb_pending.remove(&fs_id);
                match data {
                    Some(data) => {
                        self.thumbnails.insert(fs_id, data);
                    }
                    None => {
                        self.thumb_failed.insert(fs_id);
                    }
                }
            }
            Event::PreviewLoaded {
                request,
                content,
                entry,
            } => {
                if request != self.preview_seq {
                    return;
                }
                if let Some(preview) = &mut self.preview {
                    if preview.entry.fs_id == entry.fs_id {
                        if let PreviewContent::Text { total, limit, .. }
                        | PreviewContent::Markdown { total, limit, .. } = &content
                        {
                            preview.total = *total;
                            preview.text_limit = *limit;
                        }
                        preview.content = Some(content);
                        preview.loading = false;
                        preview.error = None;
                    }
                }
            }
            Event::PreviewFailed { request, message } => {
                if request == self.preview_seq
                    && let Some(preview) = &mut self.preview
                {
                    preview.loading = false;
                    preview.error = Some(message);
                }
            }
            Event::PdfDocument {
                request,
                pages,
                bytes,
            } => {
                if request != self.preview_seq {
                    return;
                }
                if let Some(preview) = &mut self.preview {
                    preview.pdf_pages = pages;
                    preview.pdf_bytes = Some(bytes.clone());
                    preview.pdf_loading = true;
                }
                self.render_pdf_page(request, 0);
            }
            Event::PdfPageRendered {
                request,
                page,
                pages,
                image,
            } => {
                if request != self.preview_seq {
                    return;
                }
                if let Some(preview) = &mut self.preview {
                    if !preview.pdf_fitted && page == 0 {
                        // Open the document fitted to the viewport (like a
                        // reader's "fit page") so the first view shows the
                        // whole page. The page was rasterized at a fixed base
                        // width, so scaling the widget is free; the zoom
                        // buttons then rerender at the requested scale.
                        let viewport = ui.viewport();
                        let available_width = (viewport.width - 224.0 - 48.0).max(200.0);
                        let available_height = (viewport.height - 194.0 - 48.0).max(200.0);
                        let fit = (available_width / image.width() as f32)
                            .min(available_height / image.height() as f32)
                            .clamp(0.25, 1.0);
                        if fit.is_finite() && fit > 0.0 {
                            preview.zoom = fit;
                            preview.pdf_fitted = true;
                        }
                    }
                    preview.pdf_page = page;
                    preview.pdf_pages = pages;
                    preview.pdf_loading = false;
                    preview.loading = false;
                    preview.content = Some(PreviewContent::Pdf { page, pages, image });
                }
            }
            Event::DownloadFinished {
                id,
                ok,
                message,
                path,
            } => {
                if let Some(job) = self.downloads.iter_mut().find(|job| job.id == id) {
                    if ok {
                        job.status = TaskStatus::Completed;
                        job.message = message.clone();
                        if let Some(path) = path {
                            job.final_path = PathBuf::from(path);
                        }
                    } else if message.contains("取消") {
                        job.status = TaskStatus::Cancelled;
                        job.message = message.clone();
                    } else if job.status != TaskStatus::Cancelled {
                        job.status = TaskStatus::Failed;
                        job.message = message.clone();
                    }
                }
                self.pump_downloads();
                if !ok {
                    self.toast(message, true);
                }
            }
            Event::DownloadBatch { entries, error } => {
                if let Some(error) = error {
                    self.toast(format!("展开文件夹失败：{error}"), true);
                } else if entries.is_empty() {
                    self.toast("文件夹内没有可下载的文件", false);
                } else {
                    let count = entries.len();
                    for entry in entries {
                        self.start_download(entry);
                    }
                    self.toast(format!("已加入 {count} 个下载任务"), false);
                }
            }
            Event::UploadFinished { id, ok, message } => {
                if let Some(job) = self.uploads.iter_mut().find(|job| job.id == id) {
                    job.status = if ok {
                        TaskStatus::Completed
                    } else if message.contains("取消") {
                        TaskStatus::Cancelled
                    } else {
                        TaskStatus::Failed
                    };
                    job.message = message.clone();
                }
                if ok {
                    self.refresh_quota();
                    if self.page == Page::Files {
                        self.refresh_current();
                    }
                } else {
                    self.toast(message, true);
                }
            }
            Event::TaskProgressNote { id, text } => {
                if let Some(job) = self.uploads.iter_mut().find(|job| job.id == id) {
                    job.note = text;
                } else if let Some(job) = self.downloads.iter_mut().find(|job| job.id == id) {
                    job.note = text;
                }
            }
            Event::QuotaUpdated { quota } => self.quota = quota,
            Event::SharesLoaded {
                request,
                shares,
                error,
            } => {
                if request != self.shares_seq {
                    return;
                }
                self.my_shares = shares;
                self.shares_state = if error.is_some() {
                    LoadState::Failed
                } else {
                    LoadState::Ready
                };
                self.shares_error = error;
            }
            Event::ShareCreated {
                link,
                pwd,
                message,
            } => {
                self.dialog = None;
                self.page = Page::Shares;
                self.nav = Nav::Shares;
                self.toast(message, false);
                let text = if pwd.is_empty() {
                    link
                } else {
                    format!("{link} 提取码 {pwd}")
                };
                self.copy_text(&text);
                self.load_shares();
            }
            Event::ShareOpened { request: _, browser } => {
                self.selected.clear();
                self.active = browser.entries.first().map(|entry| entry.fs_id);
                if let Some(first) = self.active {
                    self.selected.insert(first);
                }
                self.share_browser = Some(browser);
                self.share_browse_state = LoadState::Ready;
                self.page = Page::ShareBrowser;
                self.nav = Nav::Shares;
                self.share_browse_error = None;
            }
            Event::ShareBrowseLoaded {
                request,
                browser,
                error,
            } => {
                if request != self.share_browse_seq {
                    return;
                }
                self.active = browser.entries.first().map(|entry| entry.fs_id);
                self.selected.clear();
                if let Some(first) = self.active {
                    self.selected.insert(first);
                }
                self.share_browser = Some(browser);
                self.share_browse_state = if error.is_some() {
                    LoadState::Failed
                } else {
                    LoadState::Ready
                };
                self.share_browse_error = error;
            }
            Event::OperationDone { message, error } => {
                self.toast(message, error);
                if !error {
                    self.dialog = None;
                    if self.page == Page::Dialog {
                        self.page = Page::Files;
                        self.nav = Nav::Files;
                    }
                    if self.page == Page::Files {
                        self.refresh_current();
                    }
                    self.refresh_quota();
                }
            }
        }
    }

    fn refresh_quota(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.spawn_job(move |tx| {
            if let Ok(quota) = client.quota() {
                let _ = tx.send(Event::QuotaUpdated { quota });
            }
        });
    }

    // ------------------------------------------------------------------
    // Actions
    // ------------------------------------------------------------------

    fn apply_actions(&mut self) {
        let actions = std::mem::take(&mut self.actions);
        for action in actions {
            self.apply_action(action);
        }
    }

    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Navigate(path) => self.navigate(path),
            Action::Back => self.go_back(),
            Action::Forward => self.go_forward(),
            Action::Up => self.go_up(),
            Action::Refresh => self.refresh_current(),
            Action::Select { fs_id, toggle } => {
                if toggle {
                    if !self.selected.remove(&fs_id) {
                        self.selected.insert(fs_id);
                    }
                } else {
                    self.selected.clear();
                    self.selected.insert(fs_id);
                }
                self.active = Some(fs_id);
            }
            Action::Open(entry) => {
                if entry.is_dir() {
                    self.navigate(entry.path.clone());
                } else {
                    self.open_preview(entry);
                }
            }
            Action::Download(entries) => {
                for entry in entries {
                    self.start_download(entry);
                }
            }
            Action::ShareDialog(entries) => {
                self.open_share_dialog(entries);
            }
            Action::DeleteDialog(entries) => {
                self.dialog = Some(DialogState {
                    kind: DialogKind::Delete,
                    targets: entries,
                    title: "删除文件".to_owned(),
                    message: "删除后可在回收站中找回（如果已开启回收站）。".to_owned(),
                    period: 0,
                    public: true,
                });
                self.page = Page::Dialog;
            }
            Action::RenameDialog(entry) => {
                if let Some(inputs) = &self.inputs {
                    inputs.rename.set(entry.server_filename.clone());
                }
                self.dialog = Some(DialogState {
                    kind: DialogKind::Rename,
                    targets: vec![entry],
                    title: "重命名".to_owned(),
                    message: String::new(),
                    period: 0,
                    public: true,
                });
                self.page = Page::Dialog;
            }
            Action::MoveDialog(entries) => {
                if let Some(inputs) = &self.inputs {
                    inputs.move_dest.set(self.cwd.clone());
                }
                self.dialog = Some(DialogState {
                    kind: DialogKind::Move,
                    targets: entries,
                    title: "移动到".to_owned(),
                    message: "输入目标目录的完整路径。".to_owned(),
                    period: 0,
                    public: true,
                });
                self.page = Page::Dialog;
            }
            Action::CopyDialog(entries) => {
                if let Some(inputs) = &self.inputs {
                    inputs.move_dest.set(self.cwd.clone());
                }
                self.dialog = Some(DialogState {
                    kind: DialogKind::Copy,
                    targets: entries,
                    title: "复制到".to_owned(),
                    message: "输入目标目录的完整路径。".to_owned(),
                    period: 0,
                    public: true,
                });
                self.page = Page::Dialog;
            }
            Action::NewFolder => {
                if let Some(inputs) = &self.inputs {
                    inputs.new_folder.set(String::new());
                }
                self.dialog = Some(DialogState {
                    kind: DialogKind::NewFolder,
                    targets: Vec::new(),
                    title: "新建文件夹".to_owned(),
                    message: String::new(),
                    period: 0,
                    public: true,
                });
                self.page = Page::Dialog;
            }
            Action::Details(entry) => self.open_details(entry),
            Action::Actions(entry) => {
                self.dialog = Some(DialogState {
                    kind: DialogKind::Actions,
                    targets: vec![entry],
                    title: "文件操作".to_owned(),
                    message: String::new(),
                    period: 0,
                    public: true,
                });
                self.page = Page::Dialog;
            }
            Action::Sort(key) => {
                if self.sort_key() == key {
                    self.settings.sort_desc = !self.settings.sort_desc;
                } else {
                    self.settings.sort_key = match key {
                        SortKey::Name => "name",
                        SortKey::Size => "size",
                        SortKey::Time => "time",
                        SortKey::Kind => "kind",
                    }
                    .to_owned();
                    self.settings.sort_desc = false;
                }
                save_settings(&self.settings);
                if self.search_keyword.is_some() {
                    self.search_entries = self.sorted_entries(&self.search_entries);
                } else {
                    self.load_dir(self.cwd.clone());
                }
            }
            Action::ToggleGrid => {
                self.settings.grid_view = !self.settings.grid_view;
                save_settings(&self.settings);
                if self.settings.grid_view {
                    self.request_thumbnails();
                }
            }
            Action::SetPage(page) => {
                self.page = page;
                match page {
                    Page::Files => self.nav = Nav::Files,
                    Page::Transfers => self.nav = Nav::Transfers,
                    Page::Shares => self.nav = Nav::Shares,
                    Page::Settings => self.nav = Nav::Settings,
                    _ => {}
                }
                if page == Page::Transfers {
                    self.transfer_tab = TransferTab::Downloads;
                }
                if page == Page::Shares && self.shares_state == LoadState::Idle {
                    self.load_shares();
                }
                if page == Page::Settings
                    && let Some(inputs) = &self.inputs
                {
                    inputs.settings_dir.set(self.settings.download_dir.clone());
                }
                if page == Page::Files {
                    self.refresh_current();
                }
            }
            Action::SetNav(nav) => {
                self.nav = nav;
                self.page = match nav {
                    Nav::Files => Page::Files,
                    Nav::Transfers => Page::Transfers,
                    Nav::Shares => Page::Shares,
                    Nav::Settings => Page::Settings,
                };
                if nav == Nav::Shares && self.shares_state == LoadState::Idle {
                    self.load_shares();
                }
            }
            Action::SaveSettings => {
                if let Some(inputs) = &self.inputs {
                    let dir = inputs.settings_dir.get();
                    if !dir.trim().is_empty() {
                        self.settings.download_dir = dir;
                    }
                }
                save_settings(&self.settings);
                self.toast("设置已保存", false);
            }
            Action::Logout => self.logout(),
            Action::StartUpload(path) => self.start_upload(path),
            Action::OpenPicker => {
                let start = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".to_owned()));
                self.open_picker(start);
            }
            Action::PickerEnter(path) => self.open_picker(path),
            Action::PickerToggle(path) => {
                if let Some(picker) = &mut self.picker
                    && !picker.selected.remove(&path)
                {
                    picker.selected.insert(path);
                }
            }
            Action::PickerUpload => {
                if let Some(picker) = &self.picker {
                    let paths: Vec<PathBuf> = picker.selected.iter().cloned().collect();
                    if paths.is_empty() {
                        self.toast("请先选择要上传的文件", true);
                    } else {
                        self.page = Page::Files;
                        self.nav = Nav::Files;
                        self.start_uploads(paths);
                    }
                }
            }
            Action::PickerUploadDir(path) => {
                self.page = Page::Files;
                self.nav = Nav::Files;
                self.start_upload(path);
            }
            Action::OpenShareLink => {
                let (link, pwd) = match &self.inputs {
                    Some(inputs) => (inputs.share_link.get(), inputs.share_link_pwd.get()),
                    None => return,
                };
                self.open_share_link(link, pwd);
            }
            Action::OpenShareSummary(summary) => {
                self.open_share_link(summary.link(), summary.pwd.clone());
            }
            Action::ShareBrowserEnter(path) => {
                if let Some(browser) = &self.share_browser
                    && browser.cwd == path
                {
                    return;
                }
                self.load_share_browser_dir(path);
            }
            Action::RefreshShares => self.load_shares(),
            Action::CancelShare(share_id) => self.cancel_share(share_id),
            Action::CopyText(text) => self.copy_text(&text),
            Action::PauseDownload(id) => self.pause_download(id),
            Action::ResumeDownload(id) => self.resume_download(id),
            Action::CancelDownload(id) => self.cancel_download(id),
            Action::RemoveDownload(id) => self.remove_download(id),
            Action::RetryDownload(id) => self.retry_download(id),
            Action::OpenDownloadFolder(id) => {
                if let Some(job) = self.downloads.iter().find(|job| job.id == id) {
                    let dir = job
                        .final_path
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| PathBuf::from(&self.settings.download_dir));
                    open_path(&dir);
                }
            }
            Action::OpenDownloadFile(id) => {
                if let Some(job) = self.downloads.iter().find(|job| job.id == id) {
                    open_path(&job.final_path);
                }
            }
            Action::ClearFinishedTasks => {
                self.downloads.retain(|job| !job.status.is_terminal());
                self.uploads.retain(|job| !job.status.is_terminal());
                self.toast("已清理完成的任务", false);
            }
            Action::PauseUpload(id) => self.pause_upload(id),
            Action::ResumeUpload(id) => self.resume_upload(id),
            Action::CancelUpload(id) => self.cancel_upload(id),
            Action::RemoveUpload(id) => self.remove_upload(id),
            Action::OpenUploadFolder(id) => {
                if let Some(job) = self.uploads.iter().find(|job| job.id == id) {
                    if let Some(parent) = job.local_path.parent() {
                        open_path(parent);
                    }
                }
            }
            Action::ClosePreview => {
                self.page = Page::Files;
                self.nav = Nav::Files;
                self.preview = None;
            }
            Action::PdfPrev => {
                if let Some(preview) = &mut self.preview
                    && preview.pdf_page > 0
                {
                    let page = preview.pdf_page - 1;
                    preview.pdf_page = page;
                    let request = self.preview_seq;
                    self.render_pdf_page(request, page);
                }
            }
            Action::PdfNext => {
                let next = self
                    .preview
                    .as_ref()
                    .map(|preview| (preview.pdf_page + 1, preview.pdf_pages));
                if let Some((page, pages)) = next
                    && page < pages
                {
                    if let Some(preview) = &mut self.preview {
                        preview.pdf_page = page;
                    }
                    let request = self.preview_seq;
                    self.render_pdf_page(request, page);
                }
            }
            Action::PdfZoom(zoom) => {
                if let Some(preview) = &mut self.preview {
                    preview.pdf_fitted = true;
                    preview.zoom = (preview.zoom + zoom).clamp(0.2, 3.0);
                }
                let page = self.preview.as_ref().map(|preview| preview.pdf_page).unwrap_or(0);
                let request = self.preview_seq;
                self.render_pdf_page(request, page);
            }
            Action::PreviewFontSize(delta) => {
                if let Some(preview) = &mut self.preview {
                    preview.font_size = (preview.font_size + delta).clamp(10.0, 26.0);
                }
            }
            Action::PreviewPagePrev => {
                let (offset, limit) = self
                    .preview
                    .as_ref()
                    .map(|preview| (preview.text_offset, preview.text_limit))
                    .unwrap_or((0, 0));
                if offset > 0 {
                    let next = offset.saturating_sub(limit.max(1));
                    self.begin_preview_load(next);
                }
            }
            Action::PreviewPageNext => {
                let (offset, limit, total) = self
                    .preview
                    .as_ref()
                    .map(|preview| {
                        (preview.text_offset, preview.text_limit, preview.total)
                    })
                    .unwrap_or((0, 0, 0));
                if limit > 0 && offset.saturating_add(limit) < total {
                    self.begin_preview_load(offset + limit);
                }
            }
            Action::TransferShare(entries) => {
                self.transfer_share_entries(entries);
            }
            Action::ShareBrowserOpen(entry) => {
                if entry.is_dir() {
                    self.load_share_browser_dir(entry.path.clone());
                } else {
                    self.toast("分享内文件暂不支持直接预览，可先保存到我的网盘", false);
                }
            }
            Action::ShareBrowserUp => {
                if let Some(browser) = &self.share_browser {
                    if browser.cwd == browser.root_path {
                        return;
                    }
                    let up = crate::api::parent_remote(&browser.cwd);
                    let root = crate::api::normalize_remote_path(&browser.root_path);
                    let up_norm = crate::api::normalize_remote_path(&up);
                    let target = if up_norm == root || up_norm.starts_with(&format!("{root}/")) {
                        up
                    } else {
                        browser.root_path.clone()
                    };
                    if target != browser.cwd {
                        self.load_share_browser_dir(target);
                    }
                }
            }
            Action::CloseDialog => {
                self.dialog = None;
                self.page = Page::Files;
                self.nav = Nav::Files;
            }
            Action::ConfirmDialog => self.confirm_dialog(),
            Action::ToggleLoginTab(tab) => {
                self.login.tab = tab;
                self.login.error = None;
                if tab == LoginTab::Qr && !self.qr_started {
                    self.start_qr_login();
                }
            }
            Action::LoginQrRefresh => {
                self.qr_started = false;
                self.login.qr_image = None;
                self.start_qr_login();
            }
            Action::LoginAccount => self.login_account(),
            Action::LoginCookie => self.login_cookie(),
            Action::LogoutAndExit => {
                self.logout();
                self.page = Page::Login;
            }
        }
    }

    fn transfer_share_entries(&mut self, entries: Vec<Entry>) {
        let Some(browser) = self.share_browser.clone() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        if entries.is_empty() {
            self.toast("请选择要保存的文件", true);
            return;
        }
        let fs_ids: Vec<i64> = entries.iter().map(|entry| entry.fs_id).collect();
        let count = fs_ids.len();
        let share_id = browser.share_id;
        let uk = browser.uk;
        let sekey = if browser.sekey.is_empty() {
            crate::models::cookie_value(&client.cookie(), "BDCLND").unwrap_or_default()
        } else {
            browser.sekey.clone()
        };
        self.spawn_job(move |tx| {
            let dest = format!(
                "/共享转存-{}",
                chrono::Local::now().format("%Y%m%d-%H%M%S")
            );
            let _ = client.create_folder(&dest);
            let result = client.share_transfer(share_id, uk, &fs_ids, &dest, &sekey);
            let failed = result.is_err();
            let message = match result {
                Ok(_) => format!("已保存 {count} 个项目到 {dest}"),
                Err(error) => format!("保存失败：{error}"),
            };
            let _ = tx.send(Event::OperationDone {
                message,
                error: failed,
            });
        });
    }

    fn open_share_dialog(&mut self, entries: Vec<Entry>) {
        if let Some(inputs) = &self.inputs {
            inputs.share_pwd.set(random_pwd());
        }
        self.dialog = Some(DialogState {
            kind: DialogKind::Share,
            targets: entries,
            title: "创建分享".to_owned(),
            message: String::new(),
            period: 0,
            public: true,
        });
        self.page = Page::Dialog;
    }

    fn confirm_dialog(&mut self) {
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        let inputs = self.inputs.clone();
        match dialog.kind {
            DialogKind::NewFolder => {
                let Some(inputs) = inputs else { return };
                let name = inputs.new_folder.get().trim().to_owned();
                if name.is_empty() {
                    self.toast("请输入文件夹名称", true);
                    return;
                }
                let path = join_remote(&self.cwd, &sanitize_file_name(&name));
                self.spawn_job(move |tx| {
                    let result = client.create_folder(&path);
                    let _ = tx.send(Event::OperationDone {
                        message: match result {
                            Ok(_) => format!("已创建文件夹 {path}"),
                            Err(ref error) => format!("创建失败：{error}"),
                        },
                        error: result.is_err(),
                    });
                });
            }
            DialogKind::Rename => {
                let Some(inputs) = inputs else { return };
                let Some(target) = dialog.targets.first() else {
                    return;
                };
                let new_name = inputs.rename.get().trim().to_owned();
                if new_name.is_empty() {
                    self.toast("请输入新名称", true);
                    return;
                }
                let path = target.path.clone();
                self.spawn_job(move |tx| {
                    let result = client.rename(&path, &new_name);
                    let _ = tx.send(Event::OperationDone {
                        message: match result {
                            Ok(_) => format!("已重命名为 {new_name}"),
                            Err(ref error) => format!("重命名失败：{error}"),
                        },
                        error: result.is_err(),
                    });
                });
            }
            DialogKind::Delete => {
                let paths: Vec<String> = dialog
                    .targets
                    .iter()
                    .map(|entry| entry.path.clone())
                    .collect();
                self.spawn_job(move |tx| {
                    let result = client.delete(&paths);
                    let _ = tx.send(Event::OperationDone {
                        message: match result {
                            Ok(_) => format!("已删除 {} 个项目", paths.len()),
                            Err(ref error) => format!("删除失败：{error}"),
                        },
                        error: result.is_err(),
                    });
                });
            }
            DialogKind::Share => {
                let Some(inputs) = inputs else { return };
                let paths: Vec<String> = dialog
                    .targets
                    .iter()
                    .map(|entry| entry.path.clone())
                    .collect();
                let pwd = inputs.share_pwd.get().trim().to_owned();
                let period = dialog.period;
                let public = dialog.public;
                self.spawn_job(move |tx| {
                    match client.share_set(&paths, &pwd, period, public) {
                        Ok(value) => {
                            let link = value
                                .get("link")
                                .and_then(|value| value.as_str())
                                .unwrap_or("")
                                .to_owned();
                            let returned_pwd = value
                                .get("pwd")
                                .or_else(|| value.get("passwd"))
                                .and_then(|value| value.as_str())
                                .unwrap_or(&pwd)
                                .to_owned();
                            if link.is_empty() {
                                let _ = tx.send(Event::OperationDone {
                                    message: "分享创建成功，但未返回链接".to_owned(),
                                    error: true,
                                });
                            } else {
                                let _ = tx.send(Event::ShareCreated {
                                    link,
                                    pwd: returned_pwd,
                                    message: "分享创建成功，链接已复制".to_owned(),
                                });
                            }
                        }
                        Err(error) => {
                            let _ = tx.send(Event::OperationDone {
                                message: format!("创建分享失败：{error}"),
                                error: true,
                            });
                        }
                    }
                });
            }
            DialogKind::Move | DialogKind::Copy => {
                let Some(inputs) = inputs else { return };
                let dest = normalize_remote_path(&inputs.move_dest.get());
                let paths: Vec<String> = dialog
                    .targets
                    .iter()
                    .map(|entry| entry.path.clone())
                    .collect();
                let is_move = dialog.kind == DialogKind::Move;
                self.spawn_job(move |tx| {
                    let result = if is_move {
                        client.move_paths(&paths, &dest)
                    } else {
                        client.copy_paths(&paths, &dest)
                    };
                    let action = if is_move { "移动" } else { "复制" };
                    let _ = tx.send(Event::OperationDone {
                        message: match result {
                            Ok(_) => format!("已{action}到 {dest}"),
                            Err(ref error) => format!("{action}失败：{error}"),
                        },
                        error: result.is_err(),
                    });
                });
            }
            DialogKind::Actions | DialogKind::Details => {}
        }
    }

    // ------------------------------------------------------------------
    // Login
    // ------------------------------------------------------------------

    pub fn start_qr_login(&mut self) {
        if self.qr_started || self.client.is_some() {
            return;
        }
        self.qr_started = true;
        self.login.busy = true;
        self.login.error = None;
        self.login.qr_state = QrState::Loading;
        self.login.qr_message = "正在生成二维码…".to_owned();
        let tx = self.tx.clone();
        self.pending += 1;
        let guard_tx = tx.clone();
        crate::auth::start_qr_login(tx, guard_tx, self.session.as_ref().map(|s| s.cookie.clone()));
    }

    fn login_cookie(&mut self) {
        let Some(inputs) = &self.inputs else {
            return;
        };
        let cookie = crate::models::normalize_cookie(&inputs.cookie.get());
        if !cookie.contains("BDUSS=") {
            self.login.error = Some("Cookie 中缺少 BDUSS".to_owned());
            return;
        }
        self.login.busy = true;
        self.login.error = None;
        let tx = self.tx.clone();
        self.pending += 1;
        let guard_tx = tx.clone();
        crate::auth::login_with_cookie(tx, guard_tx, cookie);
    }

    fn login_account(&mut self) {
        let Some(inputs) = &self.inputs else {
            return;
        };
        let account = inputs.account.get().trim().to_owned();
        let password = inputs.password.get();
        let vcode = inputs.vcode.get();
        if account.is_empty() || password.is_empty() {
            self.login.error = Some("请输入账号和密码".to_owned());
            return;
        }
        self.login.busy = true;
        self.login.error = None;
        let tx = self.tx.clone();
        self.pending += 1;
        let guard_tx = tx.clone();
        crate::auth::login_with_password(tx, guard_tx, account, password, vcode);
    }

    // ------------------------------------------------------------------
    // Local picker
    // ------------------------------------------------------------------

    fn open_picker(&mut self, path: PathBuf) {
        if !path.is_dir() {
            self.toast("不是有效的目录", true);
            return;
        }
        let mut entries = Vec::new();
        let mut error = None;
        match std::fs::read_dir(&path) {
            Ok(read_dir) => {
                for entry in read_dir.flatten() {
                    let path = entry.path();
                    let Ok(meta) = entry.metadata() else {
                        continue;
                    };
                    let name = entry.file_name().to_string_lossy().to_string();
                    let modified = meta
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|duration| duration.as_secs() as i64)
                        .unwrap_or(0);
                    entries.push(LocalEntry {
                        path,
                        name,
                        is_dir: meta.is_dir(),
                        size: meta.len(),
                        modified,
                    });
                }
            }
            Err(read_error) => error = Some(read_error.to_string()),
        }
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| natural_cmp(&a.name, &b.name))
        });
        let selected = self
            .picker
            .as_ref()
            .map(|picker| picker.selected.clone())
            .unwrap_or_default();
        self.picker = Some(PickerState {
            cwd: path,
            entries,
            selected,
            error,
        });
        self.page = Page::LocalPicker;
        self.nav = Nav::Files;
    }

    // ------------------------------------------------------------------
    // PDF
    // ------------------------------------------------------------------

    pub fn render_pdf_page(&mut self, request: u64, page: usize) {
        let (bytes, pages, zoom) = match &self.preview {
            Some(preview) => match preview.pdf_bytes.clone() {
                Some(bytes) => (bytes, preview.pdf_pages, preview.zoom),
                None => return,
            },
            None => return,
        };
        if page >= pages || request != self.preview_seq {
            return;
        }
        // Each page render gets its own token: zooming or paging twice in a
        // row starts overlapping renders, and only the newest one may land in
        // the preview.
        self.preview_seq += 1;
        let request = self.preview_seq;
        if let Some(preview) = &mut self.preview {
            preview.pdf_loading = true;
        }
        let width = (PDF_BASE_WIDTH * zoom).clamp(240.0, 3600.0) as u32;
        self.spawn_job(move |tx| {
            // hayro is pure Rust but can still panic on malformed PDFs. Catch
            // it so a broken document reports an error instead of leaving the
            // preview stuck on its loading spinner forever.
            let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::media::render_pdf(&bytes, page, width)
            }))
            .unwrap_or_else(|_| Err("PDF 渲染器异常，无法显示该页面".to_owned()));
            match rendered {
                Ok((image, pages)) => {
                    let _ = tx.send(Event::PdfPageRendered {
                        request,
                        page,
                        pages,
                        image,
                    });
                }
                Err(error) => {
                    let _ = tx.send(Event::PreviewFailed { request, message: error });
                }
            }
        });
    }

    // ------------------------------------------------------------------
    // Frame
    // ------------------------------------------------------------------

    fn tick(&mut self) {
        for job in &mut self.downloads {
            if job.status == TaskStatus::Running {
                let done = job.progress.done();
                let elapsed = job.last_sample.elapsed().as_secs_f64();
                if elapsed >= 0.4 {
                    job.speed = smooth_speed(
                        job.speed,
                        done.saturating_sub(job.last_done) as f64 / elapsed,
                    );
                    job.last_done = done;
                    job.last_sample = Instant::now();
                }
                if job.progress.fraction() >= 0.999 {
                    job.message = "正在校验…".to_owned();
                }
            }
        }
        for job in &mut self.uploads {
            if job.status == TaskStatus::Running {
                let done = job.progress.done();
                let elapsed = job.last_sample.elapsed().as_secs_f64();
                if elapsed >= 0.4 {
                    job.speed = smooth_speed(
                        job.speed,
                        done.saturating_sub(job.last_done) as f64 / elapsed,
                    );
                    job.last_done = done;
                    job.last_sample = Instant::now();
                }
            }
        }
        let now = self.now;
        self.toasts.retain(|toast| now - toast.born < 5.0);
    }

    fn needs_repaint(&self) -> bool {
        if self.pending > 0 {
            return true;
        }
        if !self.toasts.is_empty() {
            return true;
        }
        if self.login.busy {
            return true;
        }
        if self
            .downloads
            .iter()
            .any(|job| job.status == TaskStatus::Running)
            || self
                .uploads
                .iter()
                .any(|job| job.status == TaskStatus::Running)
        {
            return true;
        }
        false
    }

}

impl App for BaiduApp {
    fn view(&mut self, ui: &mut Ui) {
        self.now = ui.time();
        self.ensure_inputs(ui);
        self.poll_events(ui);
        self.apply_actions();
        if let Some(keyword) = self.pending_search.take() {
            self.search(keyword);
        }
        self.pump_downloads();
        self.tick();
        // Entering the login page on the QR tab must start the code request.
        // Previously only an explicit tab click or the refresh button did it,
        // so the card sat on "正在生成二维码" until the user refreshed.
        if self.page == Page::Login
            && self.login.tab == LoginTab::Qr
            && !self.qr_started
            && !self.login.busy
        {
            self.start_qr_login();
        }

        let theme = self.theme.clone();
        ui.size_full().background(theme.background);
        match self.page {
            Page::Splash => ui.keyed("page:splash", |ui| self.splash(ui)),
            Page::Login => ui.keyed("page:login", |ui| crate::ui::login::view(self, ui)),
            _ => ui.keyed("page:main", |ui| self.main_shell(ui)),
        }
        if !self.actions.is_empty() || self.needs_repaint() {
            ui.request_repaint();
        }
    }
}

impl BaiduApp {
    fn splash(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        ui.center(|ui| {
            ui.size_full().gap(16.0);
            ui.add(Image::from_shared(self.icons.logo()).size(96.0, 96.0));
            ui.add(
                Label::new("百度网盘")
                    .font_size(26.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(theme.text),
            );
            components::spinner_text(ui, &theme, "正在恢复登录状态", self.now);
            ui.add(
                Label::new("pure Rust · kasumi")
                    .font_size(11.5)
                    .color(theme.text_subtle),
            );
        });
    }

    fn main_shell(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        ui.stack(|ui| {
            ui.size_full();
            ui.row(|ui| {
                ui.size_full();
                self.sidebar(ui);
                ui.column(|ui| {
                    ui.grow(1.0)
                        .height_full()
                        .min_width(0.0)
                        .background(theme.background);
                    self.topbar(ui);
                    ui.column(|ui| {
                        ui.grow(1.0)
                            .width_full()
                            .min_height(0.0)
                            .background(theme.background);
                        let page_key = match self.page {
                            Page::Files => "files",
                            Page::Transfers => "transfers",
                            Page::Shares => "shares",
                            Page::Settings => "settings",
                            Page::Preview => "preview",
                            Page::OpenShare => "open-share",
                            Page::ShareBrowser => "share-browser",
                            Page::LocalPicker => "picker",
                            Page::Dialog => "dialog",
                            _ => "other",
                        };
                        ui.keyed(("content", page_key), |ui| self.page_content(ui));
                    });
                    self.statusbar(ui);
                });
            });
            self.toasts_overlay(ui);
        });
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let active_tasks = self
            .downloads
            .iter()
            .filter(|job| job.status.is_active())
            .count() as u32
            + self
                .uploads
                .iter()
                .filter(|job| job.status.is_active())
                .count() as u32;
        let nav = self.nav;
        let mut clicked: Option<Nav> = None;
        ui.column(|ui| {
            ui.width(Length::Px(224.0))
                .height_full()
                .padding_xy(14.0, 16.0)
                .gap(6.0)
                .background(Color::hex(crate::theme::SIDEBAR_BG))
                .border(1.0, theme.border);
            self.user_card(ui);
            ui.row(|ui| {
                ui.height(8.0);
            });
            let responses = [
                (Nav::Files, "folder", "我的文件"),
                (Nav::Shares, "share", "我的分享"),
                (Nav::Transfers, "transfer", "传输列表"),
                (Nav::Settings, "setting", "设置"),
            ];
            for (nav_item, icon_name, label) in responses {
                let icon = self.icons.ui(icon_name);
                let badge = (nav_item == Nav::Transfers).then_some(active_tasks);
                let response = components::nav_item(
                    ui,
                    &theme,
                    icon,
                    label,
                    nav == nav_item,
                    badge,
                );
                if response.clicked() {
                    clicked = Some(nav_item);
                }
            }
            ui.row(|ui| {
                ui.height(10.0);
            });
            components::separator(ui, &theme);
            ui.row(|ui| {
                ui.height(6.0);
            });
            ui.add(
                Label::new("快捷操作")
                    .font_size(10.5)
                    .weight(FontWeight::SEMIBOLD)
                    .color(theme.text_subtle),
            );
            if components::action_button(
                ui,
                &theme,
                Some(self.icons.ui("upload")),
                "上传文件",
                ActionStyle::Ghost,
                true,
            )
            .clicked()
            {
                self.actions.push(Action::OpenPicker);
            }
            if components::action_button(
                ui,
                &theme,
                Some(self.icons.ui("add-folder")),
                "新建文件夹",
                ActionStyle::Ghost,
                true,
            )
            .clicked()
            {
                self.actions.push(Action::NewFolder);
            }
            if components::action_button(
                ui,
                &theme,
                Some(self.icons.ui("qr-code")),
                "打开分享",
                ActionStyle::Ghost,
                true,
            )
            .clicked()
            {
                self.actions.push(Action::SetPage(Page::OpenShare));
            }
            ui.spacer();
            components::separator(ui, &theme);
            ui.row(|ui| {
                ui.width_full().align_items(Align::Center).gap(8.0);
                ui.add(Image::from_shared(self.icons.ui("cloud")).size(16.0, 16.0));
                ui.column(|ui| {
                    ui.grow(1.0).basis(Length::Px(0.0)).min_width(0.0).gap(2.0);
                    ui.add(
                        Label::new("Baidu Netdisk RS")
                            .font_size(10.5)
                            .color(theme.text_muted)
                            .wrap(TextWrap::None),
                    );
                    ui.add(
                        Label::new(format!(
                            "{} / {}",
                            human_size(self.quota.used),
                            human_size(self.quota.total)
                        ))
                        .font_size(9.5)
                        .color(theme.text_subtle)
                        .wrap(TextWrap::None),
                    );
                });
            });
        });
        if let Some(nav) = clicked {
            self.actions.push(Action::SetNav(nav));
        }
    }

    fn user_card(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let user = self.user.clone();
        let quota = self.quota.clone();
        ui.column(|ui| {
            ui.width_full()
                .padding(12.0)
                .gap(10.0)
                .radius(14.0)
                .background(theme.panel)
                .border(1.0, theme.border);
            ui.row(|ui| {
                ui.width_full().align_items(Align::Center).gap(10.0);
                ui.center(|ui| {
                    ui.width(38.0)
                        .height(38.0)
                        .radius(19.0)
                        .background(theme.accent);
                    ui.add(
                        Label::new(user.initial())
                            .font_size(16.0)
                            .weight(FontWeight::SEMIBOLD)
                            .color(theme.on_accent),
                    );
                });
                ui.column(|ui| {
                    ui.grow(1.0).min_width(0.0).gap(4.0);
                    ui.add(
                        Label::new(user.display_name())
                            .font_size(13.5)
                            .weight(FontWeight::MEDIUM)
                            .color(theme.text)
                            .wrap(TextWrap::None),
                    );
                    let (bg, fg) = if user.vip_type > 0 {
                        (Color::hex("#fff4d6"), Color::hex("#b7791f"))
                    } else {
                        (theme.surface_hover, theme.text_subtle)
                    };
                    components::pill(ui, user.vip_label(), bg, fg);
                });
            });
            ui.column(|ui| {
                ui.width_full().gap(5.0);
                components::progress_bar(
                    ui,
                    &theme,
                    quota.used_fraction(),
                    6.0,
                    theme.accent,
                );
                ui.row(|ui| {
                    ui.width_full().align_items(Align::Center);
                    ui.add(
                        Label::new(format!(
                            "已用 {} · 剩余 {}",
                            human_size(quota.used),
                            human_size(
                                quota
                                    .free
                                    .max(quota.total.saturating_sub(quota.used))
                            )
                        ))
                        .font_size(9.5)
                        .color(theme.text_subtle)
                        .wrap(TextWrap::None),
                    );
                });
            });
        });
    }

    fn topbar(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        ui.row(|ui| {
            ui.width_full()
                .flex_shrink(0.0)
                .height(Length::Px(64.0))
                .padding_edges(Edges {
                    left: 20.0,
                    right: 20.0,
                    top: 10.0,
                    bottom: 10.0,
                })
                .gap(12.0)
                .align_items(Align::Center)
                .background(theme.panel)
                .border(1.0, theme.border);
            let logo = ui.row(|ui| {
                ui.clickable().gap(9.0).align_items(Align::Center);
                ui.add(Image::from_shared(self.icons.logo()).size(34.0, 34.0));
                ui.column(|ui| {
                    ui.gap(0.0);
                    ui.add(
                        Label::new("百度网盘")
                            .font_size(16.0)
                            .weight(FontWeight::SEMIBOLD)
                            .color(theme.text),
                    );
                    ui.add(
                        Label::new("Rust 桌面客户端")
                            .font_size(9.5)
                            .color(theme.text_subtle),
                    );
                });
            });
            if logo.clicked() {
                self.actions.push(Action::SetNav(Nav::Files));
            }
            ui.row(|ui| {
                ui.width(Length::Px(20.0));
            });
            let inputs = self.inputs();
            let search = inputs.search.clone();
            let search_response = ui.add(
                TextInput::new(&search)
                    .placeholder("搜索我的文件…")
                    .width(Length::Px(320.0))
                    .height(36.0),
            );
            if search_response.focused() && ui.key_pressed(&Key::Enter) {
                self.actions.push(Action::SetNav(Nav::Files));
                let app_search = search.get();
                if !app_search.trim().is_empty() {
                    // Queued actions run after the frame; the search itself is
                    // started from `view` once navigation has been applied.
                    self.pending_search = Some(app_search);
                } else {
                    self.pending_search = None;
                }
            }
            if components::icon_button(ui, &theme, self.icons.ui("search"), 34.0).clicked() {
                let keyword = search.get();
                if !keyword.trim().is_empty() {
                    self.pending_search = Some(keyword);
                    self.actions.push(Action::SetNav(Nav::Files));
                }
            }
            ui.spacer();
            let active = self
                .downloads
                .iter()
                .filter(|job| job.status.is_active())
                .count()
                + self
                    .uploads
                    .iter()
                    .filter(|job| job.status.is_active())
                    .count();
            let transfer = ui.row(|ui| {
                let hovered = ui.is_hovered();
                ui.clickable()
                    .height(36.0)
                    .padding_xy(10.0, 0.0)
                    .gap(6.0)
                    .align_items(Align::Center)
                    .radius(9.0)
                    .background(if hovered {
                        theme.surface_hover
                    } else {
                        Color::TRANSPARENT
                    });
                ui.add(Image::from_shared(self.icons.ui("transfer")).size(18.0, 18.0));
                ui.add(
                    Label::new("传输")
                        .font_size(12.5)
                        .color(theme.text_muted),
                );
                if active > 0 {
                    components::pill(
                        ui,
                        active.to_string(),
                        theme.accent,
                        theme.on_accent,
                    );
                }
            });
            if transfer.clicked() {
                self.actions.push(Action::SetNav(Nav::Transfers));
            }
            if components::icon_button(ui, &theme, self.icons.ui("setting"), 36.0).clicked() {
                self.actions.push(Action::SetNav(Nav::Settings));
            }
            let user = ui.row(|ui| {
                let hovered = ui.is_hovered();
                ui.clickable()
                    .height(38.0)
                    .padding_xy(6.0, 0.0)
                    .gap(8.0)
                    .align_items(Align::Center)
                    .radius(10.0)
                    .background(if hovered {
                        theme.surface_hover
                    } else {
                        Color::TRANSPARENT
                    });
                ui.center(|ui| {
                    ui.width(30.0)
                        .height(30.0)
                        .radius(15.0)
                        .background(theme.accent);
                    ui.add(
                        Label::new(self.user.initial())
                            .font_size(13.0)
                            .weight(FontWeight::SEMIBOLD)
                            .color(theme.on_accent),
                    );
                });
                ui.column(|ui| {
                    ui.gap(1.0);
                    ui.add(
                        Label::new(self.user.display_name())
                            .font_size(12.0)
                            .color(theme.text)
                            .wrap(TextWrap::None),
                    );
                    ui.add(
                        Label::new(self.user.vip_label())
                            .font_size(9.5)
                            .color(theme.text_subtle),
                    );
                });
            });
            if user.clicked() {
                self.actions.push(Action::SetNav(Nav::Settings));
            }
        });
    }

    fn statusbar(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let count = self.visible_entries().len();
        let selected = self.selected.len();
        let active = self
            .downloads
            .iter()
            .filter(|job| job.status == TaskStatus::Running)
            .map(|job| job.speed)
            .sum::<f64>()
            + self
                .uploads
                .iter()
                .filter(|job| job.status == TaskStatus::Running)
                .map(|job| job.speed)
                .sum::<f64>();
        ui.row(|ui| {
            ui.width_full()
                .flex_shrink(0.0)
                .height(Length::Px(30.0))
                .padding_xy(18.0, 0.0)
                .gap(12.0)
                .align_items(Align::Center)
                .background(theme.panel)
                .border(1.0, theme.border);
            ui.add(
                Label::new(format!("{count} 个项目"))
                    .font_size(10.5)
                    .color(theme.text_subtle),
            );
            if selected > 0 {
                ui.add(
                    Label::new(format!("已选择 {selected} 项"))
                        .font_size(10.5)
                        .color(theme.accent),
                );
            }
            ui.spacer();
            if active > 0.0 {
                ui.row(|ui| {
                    ui.gap(5.0).align_items(Align::Center);
                    ui.add(
                        Image::from_shared(self.icons.ui("download")).size(13.0, 13.0),
                    );
                    ui.add(
                        Label::new(human_speed(active))
                            .font_size(10.5)
                            .color(theme.text_muted),
                    );
                });
                ui.row(|ui| {
                    ui.width(1.0).height(12.0).background(theme.border);
                });
            }
            ui.add(
                Label::new(format!(
                    "{} / {}",
                    human_size(self.quota.used),
                    human_size(self.quota.total)
                ))
                .font_size(10.5)
                .color(theme.text_subtle),
            );
            if self.pending > 0 {
                components::spinner_text(ui, &theme, "后台任务", self.now);
            }
        });
    }

    fn toasts_overlay(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let viewport = ui.viewport();
        let toasts = self.toasts.clone();
        for (index, toast_data) in toasts.iter().enumerate().take(4) {
            let x = (viewport.width - 360.0).max(12.0);
            let y = (viewport.height - 60.0 - index as f32 * 58.0).max(12.0);
            ui.column(|ui| {
                ui.absolute(x, y);
                components::toast(
                    ui,
                    &theme,
                    &toast_data.text,
                    toast_data.error,
                    340.0,
                );
            });
        }
    }

    fn page_content(&mut self, ui: &mut Ui) {
        match self.page {
            Page::Files => crate::ui::files::view(self, ui),
            Page::Transfers => crate::ui::transfers::view(self, ui),
            Page::Shares => crate::ui::shares::view(self, ui),
            Page::Settings => crate::ui::settings::view(self, ui),
            Page::Preview => crate::ui::preview::view(self, ui),
            Page::OpenShare => crate::ui::shares::open_share_view(self, ui),
            Page::ShareBrowser => crate::ui::shares::browser_view(self, ui),
            Page::LocalPicker => crate::ui::picker::view(self, ui),
            Page::Dialog => crate::ui::dialog::view(self, ui),
            Page::Splash | Page::Login => {}
        }
    }
}

// ----------------------------------------------------------------------
// Helpers used by the views
// ----------------------------------------------------------------------

pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek(), right.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) => {
                if x.is_ascii_digit() && y.is_ascii_digit() {
                    let mut left_num = String::new();
                    while left.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                        left_num.push(left.next().unwrap());
                    }
                    let mut right_num = String::new();
                    while right.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                        right_num.push(right.next().unwrap());
                    }
                    let left_value = left_num.parse::<u128>().unwrap_or(0);
                    let right_value = right_num.parse::<u128>().unwrap_or(0);
                    match left_value.cmp(&right_value) {
                        std::cmp::Ordering::Equal => {}
                        ordering => return ordering,
                    }
                } else {
                    let left_char = left.next().unwrap().to_lowercase().next().unwrap_or('\0');
                    let right_char = right.next().unwrap().to_lowercase().next().unwrap_or('\0');
                    match left_char.cmp(&right_char) {
                        std::cmp::Ordering::Equal => {}
                        ordering => return ordering,
                    }
                }
            }
        }
    }
}

pub fn open_path(path: &Path) {
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

fn random_pwd() -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..4)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect()
}

fn load_settings() -> Settings {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_settings(settings: &Settings) {
    let _ = std::fs::create_dir_all(config_dir());
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(settings_path(), text);
    }
}

fn load_session() -> Option<SessionFile> {
    if let Some(session) = std::fs::read_to_string(session_path())
        .ok()
        .and_then(|text| serde_json::from_str::<SessionFile>(&text).ok())
        .filter(|session| !session.cookie.is_empty())
    {
        return Some(session);
    }
    // First-run convenience: adopt the session created by the reference
    // `bdpan.py` CLI if it exists.
    let home = std::env::var("HOME").ok()?;
    let legacy = std::path::PathBuf::from(home).join(".config/bdpan/session.json");
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(legacy).ok()?).ok()?;
    let cookie = value.get("cookie").and_then(|value| value.as_str())?.to_owned();
    if cookie.is_empty() {
        return None;
    }
    let uk = value.get("uk").and_then(|value| value.as_i64()).unwrap_or(0);
    let name = value
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_owned();
    Some(SessionFile {
        cookie,
        bdstoken: String::new(),
        user: crate::models::UserInfoOwned {
            uk,
            name,
            ..Default::default()
        },
        saved_at: 0,
    })
}

fn save_session(session: &SessionFile) {
    let _ = std::fs::create_dir_all(config_dir());
    if let Ok(text) = serde_json::to_string_pretty(session) {
        let _ = std::fs::write(session_path(), text);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                session_path(),
                std::fs::Permissions::from_mode(0o600),
            );
        }
    }
}

fn collect_remote_files(
    client: &ApiClient,
    path: &str,
    out: &mut Vec<Entry>,
) -> crate::api::Result<()> {
    let entries = client.list_all(path, "name", false)?;
    for entry in entries {
        if entry.is_dir() {
            collect_remote_files(client, &entry.path, out)?;
        } else {
            out.push(entry);
        }
    }
    Ok(())
}

pub enum PreviewLoad {
    Content(PreviewContent),
    PdfDocument { pages: usize, bytes: Arc<Vec<u8>> },
}

/// Text-heavy files are read one page at a time. Shaping a megabyte of text in
/// one label takes seconds in cosmic-text, so the reader stays at a size that
/// opens instantly.
pub fn preview_page_size(kind: FileKind) -> u64 {
    match kind {
        FileKind::Markdown => 96 * 1024,
        _ => 48 * 1024,
    }
}

pub fn load_preview(
    client: &ApiClient,
    entry: &Entry,
    kind: FileKind,
    offset: u64,
    limit: u64,
) -> std::result::Result<PreviewLoad, String> {
    let dlink = client.dlink(entry).map_err(|error| error.to_string())?;
    eprintln!("[preview] dlink ok");
    match kind {
        FileKind::Markdown | FileKind::Text | FileKind::Code | FileKind::Other => {
            let (bytes, total) = client
                .fetch_range(&dlink, offset, limit as usize)
                .map_err(|error| error.to_string())?;
            eprintln!("[preview] bytes={} total={}", bytes.len(), total);
            let text = decode_text(&bytes);
            if kind == FileKind::Markdown {
                Ok(PreviewLoad::Content(PreviewContent::Markdown {
                    text,
                    total,
                    offset,
                    limit,
                }))
            } else {
                Ok(PreviewLoad::Content(PreviewContent::Text {
                    text,
                    total,
                    offset,
                    limit,
                    encoding: if offset == 0 {
                        "自动识别 UTF-8 / GB18030".to_owned()
                    } else {
                        format!("第 {} 页", offset / limit.max(1) + 1)
                    },
                }))
            }
        }
        FileKind::Image => {
            let (bytes, _) = client
                .fetch_all(&dlink, 64 * 1024 * 1024)
                .map_err(|error| error.to_string())?;
            let image = crate::media::decode_image(&bytes, 2200)
                .ok_or_else(|| "无法解码该图片".to_owned())?;
            let width = image.width();
            let height = image.height();
            Ok(PreviewLoad::Content(PreviewContent::Image {
                image,
                width,
                height,
            }))
        }
        FileKind::Pdf => {
            let (bytes, _) = client
                .fetch_all(&dlink, 128 * 1024 * 1024)
                .map_err(|error| error.to_string())?;
            let bytes = Arc::new(bytes);
            let pages = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::media::pdf_page_count(&bytes)
            }))
            .map_err(|_| "PDF 解析器异常，无法读取该文件".to_owned())?
            .map_err(|error| error.to_string())?;
            Ok(PreviewLoad::PdfDocument { pages, bytes })
        }
        FileKind::Video | FileKind::Audio => Ok(PreviewLoad::Content(
            PreviewContent::Unsupported {
                message: "流式播放将在后续版本接入。可以先把文件下载到本地，再用系统播放器打开。"
                    .to_owned(),
            },
        )),
        _ => Ok(PreviewLoad::Content(PreviewContent::Unsupported {
            message: "该格式暂不支持内置预览，可下载后使用本地应用打开。".to_owned(),
        })),
    }
}

pub fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).to_string();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (text, _, _) = encoding_rs::UTF_16LE.decode(bytes);
        return text.to_string();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (text, _, _) = encoding_rs::UTF_16BE.decode(bytes);
        return text.to_string();
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    let (text, _, _) = encoding_rs::GB18030.decode(bytes);
    text.to_string()
}

/// Blends the freshly sampled throughput into the displayed speed.
///
/// Baidu often delivers a multi-connection download in tight bursts: a raw
/// 0.4-second sample alternates between 0 B/s and several MB/s, which makes the
/// transfer card flicker between "0 B/s" and a high value even though the
/// progress bar is moving. Decaying an idle sample instead of zeroing it keeps
/// the readout steady without hiding a genuinely stalled download for long.
pub fn smooth_speed(previous: f64, instant: f64) -> f64 {
    let next = if instant > 0.0 {
        if previous > 0.0 {
            previous * 0.6 + instant * 0.4
        } else {
            instant
        }
    } else {
        previous * 0.55
    };
    if next < 512.0 { 0.0 } else { next }
}

pub fn file_kind(entry: &Entry) -> FileKind {
    entry.kind()
}

pub fn format_time(seconds: i64) -> String {
    format_timestamp(seconds)
}
