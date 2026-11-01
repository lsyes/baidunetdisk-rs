//! Transfer list: downloads, uploads, per-task controls and aggregate speed.

use kasumi::prelude::*;
use kasumi::Image;
use kasumi_core::{Color, Length, TextWrap};

use crate::app::{Action, BaiduApp, DownloadJob, TransferTab, UploadJob};
use crate::jobs::{PHASE_DOWNLOADING, PHASE_HASHING, TaskStatus};
use crate::models::{FileKind, human_size, human_speed};
use crate::ui::components::{self, ActionStyle};

pub fn view(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    ui.column(|ui| {
        ui.width_full()
            .height_full()
            .padding_edges(Edges {
                left: 20.0,
                right: 20.0,
                top: 14.0,
                bottom: 10.0,
            })
            .gap(12.0);
        header(app, ui);
        summary(app, ui);
        ui.scroll(
            ScrollView::vertical().grow(1.0).smooth(true),
            |ui| {
                ui.width_full().gap(10.0);
                match app.transfer_tab {
                    TransferTab::Downloads => {
                        if app.downloads.is_empty() {
                            components::empty_state(
                                ui,
                                &theme,
                                app.icons.ui("download"),
                                "还没有下载任务",
                                "在“我的文件”里选择文件，点击下载即可开始",
                            );
                        }
                        let jobs = app.downloads.clone();
                        for job in jobs {
                            download_card(app, ui, &job);
                        }
                    }
                    TransferTab::Uploads => {
                        if app.uploads.is_empty() {
                            components::empty_state(
                                ui,
                                &theme,
                                app.icons.ui("upload"),
                                "还没有上传任务",
                                "点击“上传文件”，选择一个本地文件开始",
                            );
                        }
                        let jobs = app.uploads.clone();
                        for job in jobs {
                            upload_card(app, ui, &job);
                        }
                    }
                }
            },
        );
    });
}

fn header(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let tab = app.transfer_tab;
    ui.row(|ui| {
        ui.width_full().align_items(Align::Center).gap(10.0);
        ui.column(|ui| {
            ui.gap(2.0);
            ui.add(
                Label::new("传输列表")
                    .font_size(22.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(theme.text),
            );
            ui.add(
                Label::new("多线程下载 · 分片续传 · 队列并发")
                    .font_size(11.0)
                    .color(theme.text_subtle),
            );
        });
        ui.spacer();
        tab_button(app, ui, "下载", TransferTab::Downloads, tab);
        tab_button(app, ui, "上传", TransferTab::Uploads, tab);
        ui.row(|ui| {
            ui.width(8.0);
        });
        if components::action_button(
            ui,
            &theme,
            Some(app.icons.ui("delete")),
            "清理已完成",
            ActionStyle::Ghost,
            true,
        )
        .clicked()
        {
            app.actions.push(Action::ClearFinishedTasks);
        }
        if components::action_button(
            ui,
            &theme,
            Some(app.icons.ui("folder")),
            "打开下载目录",
            ActionStyle::Secondary,
            true,
        )
        .clicked()
        {
            let dir = std::path::PathBuf::from(&app.settings.download_dir);
            let _ = std::fs::create_dir_all(&dir);
            crate::app::open_path(&dir);
        }
    });
}

fn tab_button(app: &mut BaiduApp, ui: &mut Ui, label: &str, value: TransferTab, current: TransferTab) {
    let theme = app.theme.clone();
    let active = value == current;
    let hovered = ui.is_hovered();
    let count = match value {
        TransferTab::Downloads => app.downloads.iter().filter(|job| job.status.is_active()).count(),
        TransferTab::Uploads => app.uploads.iter().filter(|job| job.status.is_active()).count(),
    };
    let response = ui.row(|ui| {
        ui.clickable()
            .height(34.0)
            .padding_xy(14.0, 0.0)
            .gap(6.0)
            .align_items(Align::Center)
            .radius(9.0)
            .background(if active {
                theme.accent.fade(0.12)
            } else if hovered {
                theme.surface_hover
            } else {
                Color::TRANSPARENT
            });
        ui.add(
            Label::new(label)
                .font_size(13.0)
                .weight(if active { FontWeight::MEDIUM } else { FontWeight::NORMAL })
                .color(if active { theme.accent } else { theme.text_muted }),
        );
        if count > 0 {
            components::pill(ui, count.to_string(), theme.accent, theme.on_accent);
        }
    });
    if response.clicked() {
        app.transfer_tab = value;
    }
}

fn summary(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let downloads = app.downloads.clone();
    let uploads = app.uploads.clone();
    let active_downloads = downloads.iter().filter(|job| job.status == TaskStatus::Running).count();
    let active_uploads = uploads.iter().filter(|job| job.status == TaskStatus::Running).count();
    let queued = downloads.iter().filter(|job| job.status == TaskStatus::Queued).count()
        + uploads.iter().filter(|job| job.status == TaskStatus::Queued).count();
    let speed: f64 = downloads
        .iter()
        .filter(|job| job.status == TaskStatus::Running)
        .map(|job| job.speed)
        .sum::<f64>()
        + uploads
            .iter()
            .filter(|job| job.status == TaskStatus::Running)
            .map(|job| job.speed)
            .sum::<f64>();
    let completed = downloads.iter().filter(|job| job.status == TaskStatus::Completed).count()
        + uploads.iter().filter(|job| job.status == TaskStatus::Completed).count();
    ui.row(|ui| {
        ui.width_full().gap(10.0);
        stat(ui, &theme, "运行中", &format!("{active_downloads} 下载 · {active_uploads} 上传"));
        stat(ui, &theme, "当前速度", &human_speed(speed));
        stat(ui, &theme, "排队", &queued.to_string());
        stat(ui, &theme, "已完成", &completed.to_string());
        ui.row(|ui| {});
    });
}

fn stat(ui: &mut Ui, theme: &Theme, label: &str, value: &str) {
    ui.column(|ui| {
        ui.grow(1.0)
            .basis(Length::Px(0.0))
            .padding_xy(14.0, 10.0)
            .gap(3.0)
            .radius(11.0)
            .background(theme.panel)
            .border(1.0, theme.border);
        ui.add(
            Label::new(label)
                .font_size(10.0)
                .color(theme.text_subtle),
        );
        ui.add(
            Label::new(value.to_owned())
                .font_size(15.0)
                .weight(FontWeight::MEDIUM)
                .color(theme.text)
                .wrap(TextWrap::None),
        );
    });
}

fn download_card(app: &mut BaiduApp, ui: &mut Ui, job: &DownloadJob) {
    let theme = app.theme.clone();
    let icon = match app.thumbnails.get(&job.entry.fs_id) {
        Some(thumb) => thumb.clone(),
        None => app.icons.file_icon(job.entry.kind()),
    };
    // Read the counters once and derive both the bar and the caption from the
    // same snapshot, so a chunk arriving mid-frame cannot make the fill and
    // the "x / y (z%)" label disagree.
    let reported_total = job.progress.total();
    let done = job.progress.done();
    let total = if reported_total > 0 {
        reported_total
    } else {
        job.entry.size
    };
    let fraction = if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0) as f32
    };
    let phase_text = match job.status {
        TaskStatus::Queued => "排队中".to_owned(),
        TaskStatus::Paused => "已暂停".to_owned(),
        TaskStatus::Running => {
            if job.message.starts_with("正在") {
                // Near completion the tick sets "正在校验…"; report it here
                // instead of leaving the card on the connect/download phase.
                job.message.clone()
            } else if job.progress.phase() == PHASE_DOWNLOADING {
                "下载中".to_owned()
            } else {
                "连接中".to_owned()
            }
        }
        TaskStatus::Completed => "已完成".to_owned(),
        TaskStatus::Failed => "下载失败".to_owned(),
        TaskStatus::Cancelled => "已取消".to_owned(),
    };
    let mut actions: Vec<Action> = Vec::new();
    ui.keyed(job.id, |ui| {
        ui.column(|ui| {
            ui.width_full()
                .padding(14.0)
                .gap(9.0)
                .radius(13.0)
                .background(theme.panel)
                .border(1.0, theme.border);
            ui.row(|ui| {
                ui.width_full().align_items(Align::Center).gap(11.0);
                ui.add(
                    Image::from_shared(icon)
                        .size(36.0, 36.0)
                        .fit(ImageFit::Contain),
                );
                ui.column(|ui| {
                    ui.grow(1.0).basis(Length::Px(0.0)).min_width(0.0).gap(2.0);
                    ui.add(
                        Label::new(job.entry.server_filename.clone())
                            .font_size(13.5)
                            .weight(FontWeight::MEDIUM)
                            .color(theme.text)
                            .wrap(TextWrap::None),
                    );
                    ui.add(
                        Label::new(format!(
                            "{}  ·  {}  ·  {} 线程",
                            job.entry.path,
                            phase_text,
                            app.settings.download_threads
                        ))
                        .font_size(10.5)
                        .color(theme.text_subtle)
                        .wrap(TextWrap::None),
                    );
                });
                if job.status == TaskStatus::Running && job.speed > 0.0 {
                    ui.add(
                        Label::new(human_speed(job.speed))
                            .font_size(12.0)
                            .weight(FontWeight::MEDIUM)
                            .color(theme.accent),
                    );
                }
                status_pill(ui, &theme, job.status);
            });
            components::progress_bar(
                ui,
                &theme,
                fraction,
                7.0,
                match job.status {
                    TaskStatus::Completed => theme.success,
                    TaskStatus::Failed => theme.danger,
                    TaskStatus::Paused => theme.text_subtle,
                    _ => theme.accent,
                },
            );
            ui.row(|ui| {
                ui.width_full().align_items(Align::Center).gap(10.0);
                ui.add(
                    Label::new(format!(
                        "{} / {}  ({:.0}%)",
                        human_size(done),
                        human_size(total),
                        fraction * 100.0
                    ))
                    .font_size(10.5)
                    .color(theme.text_muted),
                );
                if !job.message.is_empty() && job.status.is_terminal() {
                    ui.add(
                        Label::new(job.message.clone())
                            .font_size(10.0)
                            .color(if job.status == TaskStatus::Failed {
                                theme.danger
                            } else {
                                theme.text_subtle
                            })
                            .wrap(TextWrap::None),
                    );
                }
                ui.spacer();
                task_buttons(ui, &theme, job, &mut actions);
            });
        });
    });
    app.actions.extend(actions);
}

fn upload_card(app: &mut BaiduApp, ui: &mut Ui, job: &UploadJob) {
    let theme = app.theme.clone();
    let name = job
        .local_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("未命名")
        .to_owned();
    let kind = std::path::Path::new(&name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(FileKind::from_extension)
        .unwrap_or(FileKind::Other);
    let icon = app.icons.file_icon(kind);
    let fraction = job.progress.fraction();
    let done = job.progress.done();
    let total = job.progress.total();
    let phase = if job.status == TaskStatus::Running {
        if job.progress.phase() == PHASE_HASHING {
            "计算分片 MD5"
        } else {
            "上传中"
        }
    } else if job.status == TaskStatus::Completed {
        "已完成"
    } else {
        job.status.label()
    };
    let mut actions: Vec<Action> = Vec::new();
    ui.keyed(job.id, |ui| {
        ui.column(|ui| {
            ui.width_full()
                .padding(14.0)
                .gap(9.0)
                .radius(13.0)
                .background(theme.panel)
                .border(1.0, theme.border);
            ui.row(|ui| {
                ui.width_full().align_items(Align::Center).gap(11.0);
                ui.add(
                    Image::from_shared(icon)
                        .size(36.0, 36.0)
                        .fit(ImageFit::Contain),
                );
                ui.column(|ui| {
                    ui.grow(1.0).basis(Length::Px(0.0)).min_width(0.0).gap(2.0);
                    ui.add(
                        Label::new(name.clone())
                            .font_size(13.5)
                            .weight(FontWeight::MEDIUM)
                            .color(theme.text)
                            .wrap(TextWrap::None),
                    );
                    ui.add(
                        Label::new(format!(
                            "{} → {}{}",
                            job.local_path.display(),
                            job.remote_dir,
                            job.note
                        ))
                        .font_size(10.5)
                        .color(theme.text_subtle)
                        .wrap(TextWrap::None),
                    );
                });
                if job.status == TaskStatus::Running && job.speed > 0.0 {
                    ui.add(
                        Label::new(human_speed(job.speed))
                            .font_size(12.0)
                            .weight(FontWeight::MEDIUM)
                            .color(theme.accent),
                    );
                }
                status_pill(ui, &theme, job.status);
            });
            components::progress_bar(
                ui,
                &theme,
                fraction,
                7.0,
                match job.status {
                    TaskStatus::Completed => theme.success,
                    TaskStatus::Failed => theme.danger,
                    TaskStatus::Paused => theme.text_subtle,
                    _ => theme.accent,
                },
            );
            ui.row(|ui| {
                ui.width_full().align_items(Align::Center).gap(10.0);
                ui.add(
                    Label::new(format!(
                        "{phase}  {} / {}  ({:.0}%)",
                        human_size(done),
                        human_size(total),
                        fraction * 100.0
                    ))
                    .font_size(10.5)
                    .color(theme.text_muted),
                );
                ui.spacer();
                if job.status == TaskStatus::Running {
                    if components::action_button(
                        ui,
                        &theme,
                        None,
                        "暂停",
                        ActionStyle::Ghost,
                        true,
                    )
                    .clicked()
                    {
                        actions.push(Action::PauseUpload(job.id));
                    }
                    if components::action_button(
                        ui,
                        &theme,
                        None,
                        "取消",
                        ActionStyle::Danger,
                        true,
                    )
                    .clicked()
                    {
                        actions.push(Action::CancelUpload(job.id));
                    }
                } else if job.status == TaskStatus::Paused {
                    if components::action_button(
                        ui,
                        &theme,
                        None,
                        "继续",
                        ActionStyle::Primary,
                        true,
                    )
                    .clicked()
                    {
                        actions.push(Action::ResumeUpload(job.id));
                    }
                    if components::action_button(
                        ui,
                        &theme,
                        None,
                        "取消",
                        ActionStyle::Danger,
                        true,
                    )
                    .clicked()
                    {
                        actions.push(Action::CancelUpload(job.id));
                    }
                } else {
                    if components::action_button(
                        ui,
                        &theme,
                        None,
                        "打开所在文件夹",
                        ActionStyle::Ghost,
                        true,
                    )
                    .clicked()
                    {
                        actions.push(Action::OpenUploadFolder(job.id));
                    }
                    if job.status.is_terminal()
                        && components::action_button(
                            ui,
                            &theme,
                            None,
                            "移除",
                            ActionStyle::Ghost,
                            true,
                        )
                        .clicked()
                    {
                        actions.push(Action::RemoveUpload(job.id));
                    }
                }
            });
        });
    });
    app.actions.extend(actions);
}

fn task_buttons(
    ui: &mut Ui,
    theme: &Theme,
    job: &DownloadJob,
    actions: &mut Vec<Action>,
) {
    match job.status {
        TaskStatus::Queued => {
            if components::action_button(ui, theme, None, "取消", ActionStyle::Danger, true).clicked() {
                actions.push(Action::CancelDownload(job.id));
            }
        }
        TaskStatus::Running => {
            if components::action_button(ui, theme, None, "暂停", ActionStyle::Ghost, true).clicked() {
                actions.push(Action::PauseDownload(job.id));
            }
            if components::action_button(ui, theme, None, "取消", ActionStyle::Danger, true).clicked() {
                actions.push(Action::CancelDownload(job.id));
            }
        }
        TaskStatus::Paused => {
            if components::action_button(ui, theme, None, "继续", ActionStyle::Primary, true).clicked() {
                actions.push(Action::ResumeDownload(job.id));
            }
            if components::action_button(ui, theme, None, "取消", ActionStyle::Danger, true).clicked() {
                actions.push(Action::CancelDownload(job.id));
            }
        }
        TaskStatus::Completed => {
            if components::action_button(ui, theme, None, "打开文件", ActionStyle::Ghost, true).clicked() {
                actions.push(Action::OpenDownloadFile(job.id));
            }
            if components::action_button(ui, theme, None, "打开文件夹", ActionStyle::Ghost, true).clicked() {
                actions.push(Action::OpenDownloadFolder(job.id));
            }
            if components::action_button(ui, theme, None, "移除", ActionStyle::Ghost, true).clicked() {
                actions.push(Action::RemoveDownload(job.id));
            }
        }
        TaskStatus::Failed | TaskStatus::Cancelled => {
            if components::action_button(ui, theme, None, "重试", ActionStyle::Primary, true).clicked() {
                actions.push(Action::RetryDownload(job.id));
            }
            if components::action_button(ui, theme, None, "打开文件夹", ActionStyle::Ghost, true).clicked() {
                actions.push(Action::OpenDownloadFolder(job.id));
            }
            if components::action_button(ui, theme, None, "移除", ActionStyle::Ghost, true).clicked() {
                actions.push(Action::RemoveDownload(job.id));
            }
        }
    }
}

fn status_pill(ui: &mut Ui, theme: &Theme, status: TaskStatus) {
    let (background, foreground) = match status {
        TaskStatus::Running => (theme.accent.fade(0.12), theme.accent),
        TaskStatus::Queued => (theme.surface_hover, theme.text_muted),
        TaskStatus::Paused => (Color::hex("#fff4d6"), Color::hex("#b7791f")),
        TaskStatus::Completed => (theme.success.fade(0.12), theme.success),
        TaskStatus::Failed => (theme.danger.fade(0.10), theme.danger),
        TaskStatus::Cancelled => (theme.surface_hover, theme.text_subtle),
    };
    components::pill(ui, status.label(), background, foreground);
}
