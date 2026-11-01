//! Three-stage uploader: precreate -> parallel superfile2 parts -> commit.

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc::Sender};
use std::time::Duration;

use md5::{Digest, Md5};

use crate::api::{ApiClient, ApiError, Result, join_remote, normalize_remote_path};
use crate::events::Event;
use crate::jobs::{Control, PHASE_HASHING, PHASE_UPLOADING, Progress};
use crate::models::human_size;

pub const PART_SIZE: usize = 4 * 1024 * 1024;
const DEFAULT_UPLOAD_HOST: &str = "c.pcs.baidu.com";

#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub id: u64,
    pub local_path: PathBuf,
    pub remote_dir: String,
    pub threads: usize,
}

pub fn run_upload(
    client: Arc<ApiClient>,
    request: UploadRequest,
    progress: Arc<Progress>,
    control: Arc<Control>,
    tx: Sender<Event>,
) {
    match upload_inner(&client, &request, &progress, &control, &tx) {
        Ok(summary) => {
            let _ = tx.send(Event::UploadFinished {
                id: request.id,
                ok: true,
                message: summary.clone(),
            });
            let _ = tx.send(Event::Notice {
                text: summary,
                error: false,
            });
        }
        Err(UploadError::Cancelled) => {
            let _ = tx.send(Event::UploadFinished {
                id: request.id,
                ok: false,
                message: "已取消上传".to_owned(),
            });
        }
        Err(UploadError::Api(error)) => {
            let _ = tx.send(Event::UploadFinished {
                id: request.id,
                ok: false,
                message: error.to_string(),
            });
        }
    }
}

enum UploadError {
    Api(ApiError),
    Cancelled,
}

impl From<ApiError> for UploadError {
    fn from(value: ApiError) -> Self {
        Self::Api(value)
    }
}

impl std::fmt::Display for UploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(error) => write!(f, "{error}"),
            Self::Cancelled => write!(f, "已取消"),
        }
    }
}

fn upload_inner(
    client: &Arc<ApiClient>,
    request: &UploadRequest,
    progress: &Arc<Progress>,
    control: &Arc<Control>,
    tx: &Sender<Event>,
) -> std::result::Result<String, UploadError> {
    let local = &request.local_path;
    let meta = std::fs::metadata(local).map_err(|error| {
        ApiError::new(-1, format!("读取本地文件失败：{error}"))
    })?;
    let remote_dir = normalize_remote_path(&request.remote_dir);

    if meta.is_dir() {
        return upload_folder(client, request, progress, control, tx);
    }
    if !meta.is_file() {
        return Err(UploadError::Api(ApiError::new(-1, "不支持的本地路径")));
    }
    let file_name = local
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("未命名文件");
    let existing = remote_names(client, &remote_dir).unwrap_or_default();
    let remote_path = join_remote(&remote_dir, &unique_name(file_name, &existing));
    upload_one(client, local, &remote_path, request.threads, progress, control)?;
    Ok(format!(
        "已上传 {} -> {}",
        file_name, remote_path
    ))
}

fn upload_folder(
    client: &Arc<ApiClient>,
    request: &UploadRequest,
    progress: &Arc<Progress>,
    control: &Arc<Control>,
    tx: &Sender<Event>,
) -> std::result::Result<String, UploadError> {
    let root = &request.local_path;
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("未命名文件夹")
        .to_owned();
    let base_remote = join_remote(&normalize_remote_path(&request.remote_dir), &root_name);
    let mut files = Vec::new();
    collect_files(root, Path::new(""), &mut files)
        .map_err(|error| ApiError::new(-1, format!("读取本地目录失败：{error}")))?;
    let total_bytes: u64 = files.iter().map(|(_, size, _)| *size).sum();
    progress.set_total(total_bytes);
    progress.set_done(0);
    progress.set_parts(0, files.len() as u32);

    let mut created: HashSet<String> = HashSet::new();
    let mut uploaded = 0usize;
    for (index, (relative, size, file_path)) in files.iter().enumerate() {
        if control.is_cancelled() {
            return Err(UploadError::Cancelled);
        }
        let dir_rel = relative.parent().unwrap_or(Path::new(""));
        let mut remote_dir = base_remote.clone();
        for component in dir_rel.components() {
            if let std::path::Component::Normal(name) = component {
                remote_dir = join_remote(&remote_dir, &name.to_string_lossy());
                if created.insert(remote_dir.clone()) {
                    let _ = client.create_folder(&remote_dir);
                }
            }
        }
        if created.insert(remote_dir.clone()) {
            let _ = client.create_folder(&remote_dir);
        }
        let name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file");
        let existing = remote_names(client, &remote_dir).unwrap_or_default();
        let remote_path = join_remote(&remote_dir, &unique_name(name, &existing));
        let _ = tx.send(Event::TaskProgressNote {
            id: request.id,
            text: format!("{}/{} · {}", index + 1, files.len(), name),
        });
        upload_one(
            client,
            file_path,
            &remote_path,
            request.threads,
            progress,
            control,
        )?;
        uploaded += 1;
        progress.set_done(
            files[..=index]
                .iter()
                .map(|(_, size, _)| *size)
                .sum::<u64>(),
        );
        progress.set_parts(uploaded as u32, files.len() as u32);
        let _ = size;
    }
    Ok(format!(
        "文件夹已上传：{} 个文件 · {}",
        uploaded,
        human_size(total_bytes)
    ))
}

fn collect_files(
    root: &Path,
    relative: &Path,
    out: &mut Vec<(PathBuf, u64, PathBuf)>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)?.flatten() {
        let path = entry.path();
        let rel = relative.join(entry.file_name());
        let meta = entry.metadata()?;
        if meta.is_dir() {
            collect_files(&path, &rel, out)?;
        } else if meta.is_file() {
            out.push((rel, meta.len(), path));
        }
    }
    Ok(())
}

struct UploadedFile;

fn upload_one(
    client: &Arc<ApiClient>,
    local_path: &Path,
    remote_path: &str,
    threads: usize,
    progress: &Arc<Progress>,
    control: &Arc<Control>,
) -> std::result::Result<UploadedFile, UploadError> {
    let size = std::fs::metadata(local_path)
        .map_err(|error| ApiError::new(-1, format!("读取文件失败：{error}")))?
        .len();
    progress.set_phase(PHASE_HASHING);
    progress.set_total(size);
    progress.set_done(0);
    let blocks = hash_parts(local_path, size, progress, control)?;
    if control.is_cancelled() {
        return Err(UploadError::Cancelled);
    }

    let pre = client.precreate(remote_path, size, &blocks, 3)?;
    if pre.get("return_type").and_then(|value| value.as_i64()) == Some(2)
        && pre.get("fs_id").and_then(|value| value.as_i64()).unwrap_or(0) != 0
    {
        progress.set_done(size);
        progress.set_phase(PHASE_UPLOADING);
        progress.set_parts(blocks.len() as u32, blocks.len() as u32);
        return Ok(UploadedFile);
    }
    let upload_id = pre
        .get("uploadid")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_owned();
    if upload_id.is_empty() {
        return Err(UploadError::Api(ApiError::new(
            -2,
            format!("precreate 未返回 uploadid：{pre}"),
        )));
    }
    let host = pre
        .get("upload_server")
        .or_else(|| pre.get("upload_server_host"))
        .and_then(|value| value.as_str())
        .unwrap_or(DEFAULT_UPLOAD_HOST)
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned();

    progress.set_phase(PHASE_UPLOADING);
    progress.set_done(0);
    progress.set_parts(0, blocks.len() as u32);
    let next = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicU64::new(0));
    let parts_done = Arc::new(AtomicU32::new(0));
    let cancelled = Arc::new(AtomicBool::new(false));
    let error_slot: Arc<std::sync::Mutex<Option<ApiError>>> =
        Arc::new(std::sync::Mutex::new(None));
    let worker_count = threads.clamp(1, 8).min(blocks.len().max(1));
    let mut handles = Vec::new();
    for _ in 0..worker_count {
        let client = client.clone();
        let remote_path = remote_path.to_owned();
        let upload_id = upload_id.clone();
        let host = host.clone();
        let blocks = blocks.clone();
        let local_path = local_path.to_path_buf();
        let next = next.clone();
        let done = done.clone();
        let progress = progress.clone();
        let control = control.clone();
        let cancelled = cancelled.clone();
        let error_slot = error_slot.clone();
        let parts_done = parts_done.clone();
        handles.push(std::thread::spawn(move || {
            loop {
                if !control.wait_while_paused() || cancelled.load(Ordering::Relaxed) {
                    return;
                }
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= blocks.len() {
                    return;
                }
                let offset = index * PART_SIZE;
                let length = ((blocks.len() - index) * PART_SIZE)
                    .min(PART_SIZE)
                    .min((progress.total().saturating_sub(offset as u64)) as usize);
                let mut buffer = vec![0u8; length];
                let file = match File::open(&local_path) {
                    Ok(file) => file,
                    Err(error) => {
                        set_error(
                            &error_slot,
                            ApiError::new(-1, format!("打开本地文件失败：{error}")),
                        );
                        cancelled.store(true, Ordering::Relaxed);
                        return;
                    }
                };
                if let Err(error) = file.read_exact_at(&mut buffer, offset as u64) {
                    set_error(
                        &error_slot,
                        ApiError::new(-1, format!("读取本地分片失败：{error}")),
                    );
                    cancelled.store(true, Ordering::Relaxed);
                    return;
                }
                match client.upload_part(
                    &host,
                    &remote_path,
                    &upload_id,
                    index as u32,
                    offset as u64,
                    &buffer,
                ) {
                    Ok(response) => {
                        let remote_md5 = response
                            .get("md5")
                            .and_then(|value| value.as_str())
                            .unwrap_or("");
                        if !remote_md5.is_empty() && !remote_md5.eq_ignore_ascii_case(&blocks[index])
                        {
                            set_error(
                                &error_slot,
                                ApiError::new(
                                    -1,
                                    format!("分片 {} 校验失败", index + 1),
                                ),
                            );
                            cancelled.store(true, Ordering::Relaxed);
                            return;
                        }
                        done.fetch_add(buffer.len() as u64, Ordering::Relaxed);
                        progress.add_done(buffer.len() as u64);
                        let completed = parts_done.fetch_add(1, Ordering::Relaxed) + 1;
                        progress.set_parts(completed, blocks.len() as u32);
                    }
                    Err(error) => {
                        set_error(&error_slot, error);
                        cancelled.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            }
        }));
    }
    for handle in handles {
        let _ = handle.join();
    }
    if control.is_cancelled() || cancelled.load(Ordering::Relaxed) {
        if let Some(error) = error_slot.lock().ok().and_then(|guard| guard.clone()) {
            return Err(UploadError::Api(error));
        }
        return Err(UploadError::Cancelled);
    }
    if let Some(error) = error_slot.lock().ok().and_then(|guard| guard.clone()) {
        return Err(UploadError::Api(error));
    }
    std::thread::sleep(Duration::from_millis(60));
    client.commit(remote_path, size, &blocks, &upload_id, 3)?;
    Ok(UploadedFile)
}

fn hash_parts(
    local_path: &Path,
    size: u64,
    progress: &Progress,
    control: &Control,
) -> std::result::Result<Vec<String>, UploadError> {
    if size == 0 {
        let empty = format!("{:x}", Md5::new().finalize());
        return Ok(vec![empty]);
    }
    let mut file = File::open(local_path)
        .map_err(|error| ApiError::new(-1, format!("打开文件失败：{error}")))?;
    let mut blocks = Vec::new();
    let mut buffer = vec![0u8; PART_SIZE];
    loop {
        if control.is_cancelled() {
            return Err(UploadError::Cancelled);
        }
        if !control.wait_while_paused() {
            return Err(UploadError::Cancelled);
        }
        let read = file
            .read(&mut buffer)
            .map_err(|error| ApiError::new(-1, format!("读取文件失败：{error}")))?;
        if read == 0 {
            break;
        }
        let mut hasher = Md5::new();
        hasher.update(&buffer[..read]);
        blocks.push(format!("{:x}", hasher.finalize()));
        progress.add_done(read as u64);
    }
    if blocks.is_empty() {
        blocks.push(format!("{:x}", Md5::new().finalize()));
    }
    Ok(blocks)
}

fn remote_names(client: &ApiClient, dir: &str) -> Result<HashSet<String>> {
    let entries = client.list_all(dir, "name", false)?;
    Ok(entries
        .into_iter()
        .map(|entry| entry.server_filename)
        .collect())
}

fn unique_name(name: &str, existing: &HashSet<String>) -> String {
    if !existing.contains(name) {
        return name.to_owned();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), format!(".{ext}")),
        _ => (name.to_owned(), String::new()),
    };
    for index in 1..10_000 {
        let candidate = format!("{stem} ({index}){ext}");
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    format!("{stem}-{}{ext}", std::process::id())
}

fn set_error(slot: &std::sync::Mutex<Option<ApiError>>, error: ApiError) {
    if let Ok(mut guard) = slot.lock()
        && guard.is_none()
    {
        *guard = Some(error);
    }
}
