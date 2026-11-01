//! Background job primitives shared by the downloader, uploader and API
//! workers.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

pub const PHASE_HASHING: u8 = 0;
pub const PHASE_UPLOADING: u8 = 1;
pub const PHASE_DOWNLOADING: u8 = 2;
/// Download worker is resolving the direct link / probing the remote size.
pub const PHASE_CONNECTING: u8 = 3;

#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub stored: AtomicU64,
    pub phase: AtomicU32,
    pub parts_done: AtomicU32,
    pub parts_total: AtomicU32,
}

impl Progress {
    pub fn new(total: u64) -> Self {
        Self {
            total: AtomicU64::new(total),
            ..Self::default()
        }
    }

    pub fn done(&self) -> u64 {
        self.done.load(Ordering::Relaxed)
    }

    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    pub fn fraction(&self) -> f32 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            (self.done() as f64 / total as f64).clamp(0.0, 1.0) as f32
        }
    }

    pub fn set_phase(&self, phase: u8) {
        self.phase.store(u32::from(phase), Ordering::Relaxed);
    }

    pub fn phase(&self) -> u8 {
        self.phase.load(Ordering::Relaxed) as u8
    }

    pub fn add_done(&self, bytes: u64) {
        self.done.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn set_done(&self, bytes: u64) {
        self.done.store(bytes, Ordering::Relaxed);
    }

    pub fn set_total(&self, bytes: u64) {
        self.total.store(bytes, Ordering::Relaxed);
    }

    pub fn set_parts(&self, done: u32, total: u32) {
        self.parts_done.store(done, Ordering::Relaxed);
        self.parts_total.store(total, Ordering::Relaxed);
    }
}

#[derive(Debug, Default)]
pub struct Control {
    pub pause: AtomicBool,
    pub cancel: AtomicBool,
}

impl Control {
    pub fn pause(&self) {
        self.pause.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.pause.store(false, Ordering::Relaxed);
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.resume();
    }

    pub fn is_paused(&self) -> bool {
        self.pause.load(Ordering::Relaxed)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Blocks while paused. Returns `false` when the job was cancelled.
    pub fn wait_while_paused(&self) -> bool {
        while self.is_paused() {
            if self.is_cancelled() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(80));
        }
        !self.is_cancelled()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "排队中",
            Self::Running => "进行中",
            Self::Paused => "已暂停",
            Self::Completed => "已完成",
            Self::Failed => "失败",
            Self::Cancelled => "已取消",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Paused)
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Sleeps in small slices so cancellation is responsive.
pub fn sleep_cancellable(control: &Control, millis: u64) -> bool {
    let mut left = millis;
    while left > 0 {
        if control.is_cancelled() {
            return false;
        }
        let step = left.min(100);
        std::thread::sleep(Duration::from_millis(step));
        left -= step;
    }
    !control.is_cancelled()
}
