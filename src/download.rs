//! Hand-written multi-threaded segmented downloader.
//!
//! A download is split into byte ranges; every range is fetched with its own
//! HTTP connection and written at its offset with `pwrite`. Progress is kept
//! in a sidecar file so an interrupted download resumes from the exact byte
//! offsets of every segment.

use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc::Sender};
use std::time::{Duration, Instant};

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::api::{ApiClient, Result};
use crate::events::Event;
use crate::jobs::{Control, Progress};
use crate::models::{Entry, human_size};

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub id: u64,
    pub entry: Entry,
    pub final_path: PathBuf,
    pub threads: usize,
    pub verify_md5: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PartInfo {
    start: u64,
    end: u64,
    done: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PartMeta {
    total: u64,
    parts: Vec<PartInfo>,
}

pub fn part_paths(final_path: &Path) -> (PathBuf, PathBuf) {
    let part = PathBuf::from(format!("{}.bdpart", final_path.display()));
    let meta = PathBuf::from(format!("{}.json", part.display()));
    (part, meta)
}

/// Picks a final path that does not collide with an unrelated existing file.
/// A matching sidecar means the existing `.bdpart` is ours and may be resumed.
pub fn planned_path(dir: &Path, file_name: &str) -> PathBuf {
    let safe = sanitize_file_name(file_name);
    let candidate = dir.join(&safe);
    let (part, meta) = part_paths(&candidate);
    if !candidate.exists() || (part.exists() && meta.exists()) {
        return candidate;
    }
    let stem = candidate
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(&safe);
    let ext = candidate
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default();
    for index in 1..10_000 {
        let next = dir.join(format!("{stem} ({index}){ext}"));
        let (part, meta) = part_paths(&next);
        if !next.exists() || (part.exists() && meta.exists()) {
            return next;
        }
    }
    dir.join(format!("{stem}-{}{ext}", std::process::id()))
}

pub fn sanitize_file_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if matches!(ch, '/' | '\\' | '\0') {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    let out = out.trim().to_owned();
    if out.is_empty() {
        "未命名文件".to_owned()
    } else {
        out
    }
}

pub fn run_download(
    client: Arc<ApiClient>,
    request: DownloadRequest,
    progress: Arc<Progress>,
    control: Arc<Control>,
    tx: Sender<Event>,
) {
    let outcome = download_inner(client, &request, progress, control);
    match outcome {
        Ok(DownloadOutcome::Completed { bytes, resumed }) => {
            let note = if resumed {
                format!("下载完成（{}，含断点续传）", human_size(bytes))
            } else {
                format!("下载完成（{}）", human_size(bytes))
            };
            let _ = tx.send(Event::DownloadFinished {
                id: request.id,
                ok: true,
                message: note,
                path: Some(request.final_path.display().to_string()),
            });
            let _ = tx.send(Event::Notice {
                text: format!("{} 已下载完成", request.entry.server_filename),
                error: false,
            });
        }
        Ok(DownloadOutcome::Cancelled) => {
            let _ = tx.send(Event::DownloadFinished {
                id: request.id,
                ok: false,
                message: "已取消，已下载部分保留以便续传".to_owned(),
                path: None,
            });
        }
        Err(error) => {
            let _ = tx.send(Event::DownloadFinished {
                id: request.id,
                ok: false,
                message: error.to_string(),
                path: None,
            });
        }
    }
}

enum DownloadOutcome {
    Completed { bytes: u64, resumed: bool },
    Cancelled,
}

fn download_inner(
    client: Arc<ApiClient>,
    request: &DownloadRequest,
    progress: Arc<Progress>,
    control: Arc<Control>,
) -> Result<DownloadOutcome> {
    let entry = &request.entry;
    let final_path = &request.final_path;
    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| crate::api::ApiError::new(-1, format!("创建下载目录失败：{error}")))?;
    }
    let (part_path, meta_path) = part_paths(final_path);

    progress.set_phase(crate::jobs::PHASE_CONNECTING);
    let dlink = client.dlink(entry)?;
    let mut total = entry.size;
    let (probed_total, range_supported) = match client.probe_size(&dlink) {
        Ok((size, partial)) => (size, partial),
        Err(_) => (total, true),
    };
    if probed_total > 0 {
        total = probed_total;
    }
    progress.set_total(total);
    progress.set_phase(crate::jobs::PHASE_DOWNLOADING);

    if total == 0 {
        File::create(final_path)
            .map_err(|error| crate::api::ApiError::new(-1, format!("创建文件失败：{error}")))?;
        progress.set_done(0);
        progress.set_parts(1, 1);
        return Ok(DownloadOutcome::Completed {
            bytes: 0,
            resumed: false,
        });
    }

    let mut resumed = false;
    let mut parts: Vec<PartInfo> = Vec::new();
    if part_path.exists() && meta_path.exists() {
        if let Ok(text) = std::fs::read_to_string(&meta_path)
            && let Ok(meta) = serde_json::from_str::<PartMeta>(&text)
            && meta.total == total
            && !meta.parts.is_empty()
            && meta
                .parts
                .iter()
                .all(|part| part.start < part.end && part.end <= total && part.done <= part.end - part.start)
            && meta.parts.first().is_some_and(|part| part.start == 0)
            && meta.parts.last().is_some_and(|part| part.end == total)
        {
            parts = meta.parts;
            resumed = parts.iter().any(|part| part.done > 0);
        }
    }
    if parts.is_empty() {
        let segments = segment_count(total, request.threads, range_supported);
        parts = (0..segments)
            .map(|index| {
                let start = total * index as u64 / segments as u64;
                let end = total * (index + 1) as u64 / segments as u64;
                PartInfo {
                    start,
                    end,
                    done: 0,
                }
            })
            .collect();
        if part_path.exists() {
            let _ = std::fs::remove_file(&part_path);
        }
    }

    {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&part_path)
            .map_err(|error| crate::api::ApiError::new(-1, format!("打开临时文件失败：{error}")))?;
        file.set_len(total)
            .map_err(|error| crate::api::ApiError::new(-1, format!("预分配文件失败：{error}")))?;
    }

    let already: u64 = parts.iter().map(|part| part.done).sum();
    progress.set_done(already);
    progress.set_parts(
        parts.iter().filter(|p| p.done == p.end - p.start).count() as u32,
        parts.len() as u32,
    );
    write_meta(&meta_path, total, &parts);

    let lengths: Vec<u64> = parts.iter().map(|part| part.end - part.start).collect();
    let done_slots: Arc<Vec<AtomicU64>> = Arc::new(
        parts
            .iter()
            .map(|part| AtomicU64::new(part.done))
            .collect(),
    );
    let cancelled = Arc::new(AtomicBool::new(false));
    let fatal: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let alive = Arc::new(AtomicUsize::new(parts.len()));

    let mut handles = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let part = part.clone();
        let client = client.clone();
        let url = dlink.clone();
        let part_path = part_path.clone();
        let progress = progress.clone();
        let control = control.clone();
        let cancelled = cancelled.clone();
        let fatal = fatal.clone();
        let alive = alive.clone();
        let done_slot = done_slots.clone();
        let part_len = lengths[index];
        let index = index;
        handles.push(std::thread::spawn(move || {
            let result = download_segment(
                &client,
                &url,
                &part_path,
                part.start,
                part.end,
                &done_slot[index],
                part_len,
                &progress,
                &control,
                &cancelled,
            );
            if let Err(error) = result {
                if let Ok(mut guard) = fatal.lock()
                    && guard.is_none()
                {
                    *guard = Some(format!(
                        "分片 {} 下载失败：{error}",
                        index + 1
                    ));
                }
                cancelled.store(true, Ordering::Relaxed);
            }
            alive.fetch_sub(1, Ordering::Relaxed);
        }));
    }

    let started = Instant::now();
    let mut last_meta = Instant::now();
    loop {
        let remaining = alive.load(Ordering::Relaxed);
        if remaining == 0 {
            break;
        }
        if control.is_cancelled() {
            cancelled.store(true, Ordering::Relaxed);
        }
        if last_meta.elapsed() >= Duration::from_secs(1) {
            let parts_now: Vec<PartInfo> = parts
                .iter()
                .enumerate()
                .map(|(index, part)| PartInfo {
                    start: part.start,
                    end: part.end,
                    done: done_slots[index].load(Ordering::Relaxed),
                })
                .collect();
            write_meta(&meta_path, total, &parts_now);
            last_meta = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    for handle in handles {
        let _ = handle.join();
    }

    if control.is_cancelled() {
        let parts_now: Vec<PartInfo> = parts
            .iter()
            .enumerate()
            .map(|(index, part)| PartInfo {
                start: part.start,
                end: part.end,
                done: done_slots[index].load(Ordering::Relaxed),
            })
            .collect();
        write_meta(&meta_path, total, &parts_now);
        return Ok(DownloadOutcome::Cancelled);
    }
    if let Some(message) = fatal.lock().ok().and_then(|guard| guard.clone()) {
        let parts_now: Vec<PartInfo> = parts
            .iter()
            .enumerate()
            .map(|(index, part)| PartInfo {
                start: part.start,
                end: part.end,
                done: done_slots[index].load(Ordering::Relaxed),
            })
            .collect();
        write_meta(&meta_path, total, &parts_now);
        return Err(crate::api::ApiError::new(-1, message));
    }

    let downloaded: u64 = done_slots
        .iter()
        .map(|slot| slot.load(Ordering::Relaxed))
        .sum();
    if downloaded < total {
        return Err(crate::api::ApiError::new(
            -1,
            format!(
                "下载不完整：{} / {}",
                human_size(downloaded),
                human_size(total)
            ),
        ));
    }
    if request.verify_md5 && is_md5(&entry.md5) {
        let actual = md5_file(&part_path)?;
        if !actual.eq_ignore_ascii_case(&entry.md5) {
            return Err(crate::api::ApiError::new(
                -1,
                format!("MD5 校验失败：本地 {actual} / 云端 {}", entry.md5),
            ));
        }
    }
    std::fs::rename(&part_path, final_path)
        .map_err(|error| crate::api::ApiError::new(-1, format!("重命名文件失败：{error}")))?;
    let _ = std::fs::remove_file(&meta_path);
    progress.set_done(total);
    progress.set_parts(parts.len() as u32, parts.len() as u32);
    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    let _ = elapsed;
    Ok(DownloadOutcome::Completed {
        bytes: total,
        resumed,
    })
}

#[allow(clippy::too_many_arguments)]
fn download_segment(
    client: &ApiClient,
    url: &str,
    part_path: &Path,
    start: u64,
    end: u64,
    done_slot: &AtomicU64,
    part_len: u64,
    progress: &Progress,
    control: &Control,
    cancelled: &AtomicBool,
) -> Result<()> {
    let mut failures = 0u32;
    loop {
        if !control.wait_while_paused() || cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        let done = done_slot.load(Ordering::Relaxed);
        if done >= part_len {
            return Ok(());
        }
        let absolute_start = start + done;
        let response = client.open_range(url, absolute_start, Some(end - 1));
        let mut response = match response {
            Ok(response) => {
                failures = 0;
                response
            }
            Err(error) => {
                failures += 1;
                if failures > 6 {
                    return Err(error);
                }
                let delay = (300u64 * failures as u64).min(4_000);
                if !crate::jobs::sleep_cancellable(control, delay) {
                    return Ok(());
                }
                continue;
            }
        };
        if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && done == part_len {
            return Ok(());
        }
        if !response.status().is_success() {
            failures += 1;
            if failures > 6 {
                return Err(crate::api::ApiError::new(
                    response.status().as_u16() as i64,
                    format!("HTTP {}", response.status().as_u16()),
                ));
            }
            if !crate::jobs::sleep_cancellable(control, 800) {
                return Ok(());
            }
            continue;
        }
        let range_ignored = response.status() == reqwest::StatusCode::OK && absolute_start > 0;
        if range_ignored {
            return Err(crate::api::ApiError::new(
                -1,
                "服务器忽略了断点范围，请重新下载",
            ));
        }
        let file = OpenOptions::new()
            .write(true)
            .open(part_path)
            .map_err(|error| crate::api::ApiError::new(-1, format!("打开临时文件失败：{error}")))?;
        let mut offset = absolute_start;
        let mut buffer = vec![0u8; 128 * 1024];
        let mut got_data = false;
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }
            if control.is_paused() {
                break;
            }
            let read = match response.read(&mut buffer) {
                Ok(0) => 0,
                Ok(read) => read,
                Err(error) => {
                    if !got_data {
                        failures += 1;
                        if failures > 6 {
                            return Err(crate::api::ApiError::network(format!(
                                "读取下载流失败：{error}"
                            )));
                        }
                    }
                    break;
                }
            };
            if read == 0 {
                break;
            }
            let allowed = ((end - offset) as usize).min(read);
            file.write_all_at(&buffer[..allowed], offset)
                .map_err(|error| crate::api::ApiError::new(-1, format!("写入文件失败：{error}")))?;
            offset += allowed as u64;
            done_slot.fetch_add(allowed as u64, Ordering::Relaxed);
            progress.add_done(allowed as u64);
            got_data = true;
            if offset >= end {
                return Ok(());
            }
        }
        drop(file);
        if !got_data && done_slot.load(Ordering::Relaxed) < part_len {
            failures += 1;
            if failures > 6 {
                return Err(crate::api::ApiError::new(-1, "下载流提前结束"));
            }
            if !crate::jobs::sleep_cancellable(control, 700) {
                return Ok(());
            }
        }
    }
}

fn segment_count(total: u64, threads: usize, range_supported: bool) -> usize {
    if !range_supported || threads <= 1 {
        return 1;
    }
    let threads = threads.clamp(1, 16);
    let minimum = 1024 * 1024u64;
    let by_size = (total / minimum).max(1) as usize;
    threads.min(by_size).max(1)
}

fn write_meta(path: &Path, total: u64, parts: &[PartInfo]) {
    let meta = PartMeta {
        total,
        parts: parts.to_vec(),
    };
    if let Ok(text) = serde_json::to_string(&meta) {
        let _ = std::fs::write(path, text);
    }
}

fn is_md5(value: &str) -> bool {
    value.len() == 32 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn md5_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .map_err(|error| crate::api::ApiError::new(-1, format!("打开文件失败：{error}")))?;
    let mut hasher = Md5::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| crate::api::ApiError::new(-1, format!("读取文件失败：{error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Run a download while routing segment requests through `client`.
pub fn spawn_download(
    client: Arc<ApiClient>,
    request: DownloadRequest,
    progress: Arc<Progress>,
    control: Arc<Control>,
    tx: Sender<Event>,
) {
    std::thread::spawn(move || {
        run_download(client, request, progress, control, tx);
    });
}
