//! Messages sent from background workers to the UI thread.

use std::sync::Arc;

use kasumi_core::ImageData;

use crate::models::{Entry, Quota, SessionFile, UserInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrState {
    Loading,
    Waiting,
    Scanned,
    Confirmed,
    Expired,
    Failed,
}

#[derive(Debug, Clone)]
pub enum PreviewContent {
    Text {
        text: String,
        total: u64,
        offset: u64,
        limit: u64,
        encoding: String,
    },
    Markdown {
        text: String,
        total: u64,
        offset: u64,
        limit: u64,
    },
    Image {
        image: Arc<ImageData>,
        width: u32,
        height: u32,
    },
    Pdf {
        page: usize,
        pages: usize,
        image: Arc<ImageData>,
    },
    Unsupported {
        message: String,
    },
}

#[derive(Clone)]
pub enum Event {
    /// Emitted automatically when a worker thread exits.
    JobDone,
    Notice {
        text: String,
        error: bool,
    },
    SessionChecked {
        result: Result<(UserInfo, Quota, SessionFile), String>,
    },
    LoginQr {
        image: Arc<ImageData>,
        message: String,
    },
    LoginQrStatus {
        state: QrState,
        message: String,
    },
    LoginOk {
        session: SessionFile,
        user: UserInfo,
        quota: Quota,
    },
    LoginFailed {
        message: String,
        captcha: Option<Arc<ImageData>>,
    },
    DirLoaded {
        request: u64,
        path: String,
        entries: Vec<Entry>,
        error: Option<String>,
    },
    SearchLoaded {
        request: u64,
        keyword: String,
        entries: Vec<Entry>,
        error: Option<String>,
    },
    Thumbnail {
        fs_id: i64,
        data: Option<Arc<ImageData>>,
    },
    PreviewLoaded {
        request: u64,
        content: PreviewContent,
        entry: Entry,
    },
    PdfDocument {
        request: u64,
        pages: usize,
        bytes: Arc<Vec<u8>>,
    },
    DownloadBatch {
        entries: Vec<Entry>,
        error: Option<String>,
    },
    PreviewFailed {
        request: u64,
        message: String,
    },
    PdfPageRendered {
        request: u64,
        page: usize,
        pages: usize,
        image: Arc<ImageData>,
    },
    DownloadFinished {
        id: u64,
        ok: bool,
        message: String,
        path: Option<String>,
    },
    UploadFinished {
        id: u64,
        ok: bool,
        message: String,
    },
    TaskProgressNote {
        id: u64,
        text: String,
    },
    QuotaUpdated {
        quota: Quota,
    },
    SharesLoaded {
        request: u64,
        shares: Vec<crate::share::ShareSummary>,
        error: Option<String>,
    },
    ShareCreated {
        link: String,
        pwd: String,
        message: String,
    },
    ShareOpened {
        request: u64,
        browser: crate::share::ShareBrowser,
    },
    ShareBrowseLoaded {
        request: u64,
        browser: crate::share::ShareBrowser,
        error: Option<String>,
    },
    OperationDone {
        message: String,
        error: bool,
    },
}
