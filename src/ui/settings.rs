//! Settings, account and about page.

use kasumi::prelude::*;
use kasumi::Image;
use kasumi_core::{Color, Length, TextWrap};

use crate::app::{Action, BaiduApp};
use crate::models::human_size;
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
            .gap(10.0);
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(10.0);
            ui.column(|ui| {
                ui.gap(2.0);
                ui.add(
                    Label::new("设置")
                        .font_size(22.0)
                        .weight(FontWeight::SEMIBOLD)
                        .color(theme.text),
                );
                ui.add(
                    Label::new("下载、上传与账号")
                        .font_size(11.0)
                        .color(theme.text_subtle),
                );
            });
            ui.spacer();
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("check")),
                "保存设置",
                ActionStyle::Primary,
                true,
            )
            .clicked()
            {
                app.actions.push(Action::SaveSettings);
            }
        });
        ui.scroll(
            ScrollView::vertical().grow(1.0).smooth(true),
            |ui| {
                ui.width_full().gap(12.0);
                account_card(app, ui);
                download_card(app, ui);
                upload_card(app, ui);
                appearance_card(app, ui);
                about_card(app, ui);
            },
        );
    });
}

fn card_shell(ui: &mut Ui, theme: &Theme, title: &str, subtitle: &str, body: impl FnOnce(&mut Ui)) {
    ui.column(|ui| {
        ui.width_full()
            .padding(18.0)
            .gap(13.0)
            .radius(14.0)
            .background(theme.panel)
            .border(1.0, theme.border);
        ui.column(|ui| {
            ui.gap(2.0);
            ui.add(
                Label::new(title)
                    .font_size(15.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(theme.text),
            );
            ui.add(
                Label::new(subtitle)
                    .font_size(10.5)
                    .color(theme.text_subtle),
            );
        });
        components::separator(ui, theme);
        body(ui);
    });
}

fn account_card(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let user = app.user.clone();
    let quota = app.quota.clone();
    let mut actions: Vec<Action> = Vec::new();
    card_shell(ui, &theme, "账号", "登录状态与网盘容量", |ui| {
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(14.0);
            ui.center(|ui| {
                ui.width(52.0)
                    .height(52.0)
                    .radius(26.0)
                    .background(theme.accent);
                ui.add(
                    Label::new(user.initial())
                        .font_size(20.0)
                        .weight(FontWeight::SEMIBOLD)
                        .color(theme.on_accent),
                );
            });
            ui.column(|ui| {
                ui.gap(4.0);
                ui.row(|ui| {
                    ui.gap(8.0).align_items(Align::Center);
                    ui.add(
                        Label::new(user.display_name())
                            .font_size(15.0)
                            .weight(FontWeight::MEDIUM)
                            .color(theme.text),
                    );
                    components::pill(
                        ui,
                        user.vip_label(),
                        if user.vip_type > 0 {
                            Color::hex("#fff4d6")
                        } else {
                            theme.surface_hover
                        },
                        if user.vip_type > 0 {
                            Color::hex("#b7791f")
                        } else {
                            theme.text_subtle
                        },
                    );
                });
                ui.add(
                    Label::new(format!("UK {} · 普通网页接口 250528", user.uk))
                        .font_size(10.5)
                        .color(theme.text_subtle)
                        .family(FontFamily::Monospace),
                );
            });
            ui.spacer();
            let logout = if app.logout_armed {
                "确认退出登录"
            } else {
                "退出登录"
            };
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("quit")),
                logout,
                ActionStyle::Danger,
                true,
            )
            .clicked()
            {
                if app.logout_armed {
                    actions.push(Action::Logout);
                } else {
                    app.logout_armed = true;
                    app.toast("再次点击确认退出登录", false);
                }
            }
        });
        ui.column(|ui| {
            ui.width_full().gap(5.0);
            components::progress_bar(ui, &theme, quota.used_fraction(), 7.0, theme.accent);
            ui.row(|ui| {
                ui.width_full();
                ui.add(
                    Label::new(format!(
                        "已用 {} / {}",
                        human_size(quota.used),
                        human_size(quota.total)
                    ))
                    .font_size(10.5)
                    .color(theme.text_muted),
                );
                ui.spacer();
                ui.add(
                    Label::new(format!("剩余 {}", human_size(quota.free)))
                        .font_size(10.5)
                        .color(theme.text_subtle),
                );
            });
        });
    });
    app.actions.extend(actions);
}

fn download_card(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let inputs = app.inputs().clone();
    let mut actions: Vec<Action> = Vec::new();
    card_shell(ui, &theme, "下载", "多线程分段、断点续传", |ui| {
        components::field_label(ui, &theme, "默认下载目录");
        ui.add(
            TextInput::new(&inputs.settings_dir)
                .placeholder("/home/you/下载/百度网盘")
                .width(Length::Percent(100.0))
                .height(38.0),
        );
        ui.row(|ui| {
            ui.width_full().gap(10.0).align_items(Align::Center);
            ui.column(|ui| {
                ui.grow(1.0).basis(Length::Px(0.0)).gap(3.0);
                ui.add(
                    Label::new("每个任务的下载线程数")
                        .font_size(12.0)
                        .color(theme.text),
                );
                ui.add(
                    Label::new("按 1 MB 最小分片切分，多连接并发写入同一个文件")
                        .font_size(10.0)
                        .color(theme.text_subtle),
                );
            });
            if stepper_minus(ui, &theme, app.settings.download_threads > 1) {
                app.settings.download_threads =
                    app.settings.download_threads.saturating_sub(1).max(1);
            }
            stepper_value(ui, &theme, app.settings.download_threads);
            if stepper_plus(ui, &theme, app.settings.download_threads < 16) {
                app.settings.download_threads =
                    (app.settings.download_threads + 1).min(16);
            }
        });
        ui.row(|ui| {
            ui.width_full().gap(10.0).align_items(Align::Center);
            ui.column(|ui| {
                ui.grow(1.0).basis(Length::Px(0.0)).gap(3.0);
                ui.add(
                    Label::new("同时运行的下载任务")
                        .font_size(12.0)
                        .color(theme.text),
                );
                ui.add(
                    Label::new("其余任务会保持排队，完成后自动开始")
                        .font_size(10.0)
                        .color(theme.text_subtle),
                );
            });
            if stepper_minus(ui, &theme, app.settings.max_active_downloads > 1) {
                app.settings.max_active_downloads =
                    app.settings.max_active_downloads.saturating_sub(1).max(1);
            }
            stepper_value(ui, &theme, app.settings.max_active_downloads);
            if stepper_plus(ui, &theme, app.settings.max_active_downloads < 5) {
                app.settings.max_active_downloads =
                    (app.settings.max_active_downloads + 1).min(5);
            }
        });
        if components::checkbox(
            ui,
            &theme,
            app.settings.verify_md5,
            "下载完成后校验 MD5（较慢，仅对已知 MD5 的文件生效）",
            true,
        ) {
            app.settings.verify_md5 = !app.settings.verify_md5;
        }
        if components::action_button(
            ui,
            &theme,
            Some(app.icons.ui("folder")),
            "打开下载目录",
            ActionStyle::Ghost,
            true,
        )
        .clicked()
        {
            let dir = std::path::PathBuf::from(&app.settings.download_dir);
            let _ = std::fs::create_dir_all(&dir);
            crate::app::open_path(&dir);
        }
    });
    app.actions.extend(actions);
}

fn upload_card(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    card_shell(ui, &theme, "上传", "precreate → 并行分片 → commit", |ui| {
        ui.row(|ui| {
            ui.width_full().gap(10.0).align_items(Align::Center);
            ui.column(|ui| {
                ui.grow(1.0).basis(Length::Px(0.0)).gap(3.0);
                ui.add(
                    Label::new("每个文件的上传线程数")
                        .font_size(12.0)
                        .color(theme.text),
                );
                ui.add(
                    Label::new("分片大小固定 4 MB，block_list 使用全部分片 MD5")
                        .font_size(10.0)
                        .color(theme.text_subtle),
                );
            });
            if stepper_minus(ui, &theme, app.settings.upload_threads > 1) {
                app.settings.upload_threads =
                    app.settings.upload_threads.saturating_sub(1).max(1);
            }
            stepper_value(ui, &theme, app.settings.upload_threads);
            if stepper_plus(ui, &theme, app.settings.upload_threads < 8) {
                app.settings.upload_threads = (app.settings.upload_threads + 1).min(8);
            }
        });
    });
}

fn appearance_card(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    card_shell(ui, &theme, "外观", "文件列表的显示方式", |ui| {
        if components::checkbox(ui, &theme, app.settings.grid_view, "网格视图（显示图片缩略图）", true) {
            app.settings.grid_view = !app.settings.grid_view;
            app.actions.push(Action::SaveSettings);
        }
        ui.add(
            Label::new("列表视图更紧凑；网格视图会为图片请求 320×240 的缩略图。")
                .font_size(10.5)
                .color(theme.text_subtle),
        );
    });
}

fn about_card(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    card_shell(ui, &theme, "关于", "第三方客户端 · 个人学习用途", |ui| {
        ui.row(|ui| {
            ui.gap(12.0).align_items(Align::Center);
            ui.add(Image::from_shared(app.icons.logo()).size(46.0, 46.0));
            ui.column(|ui| {
                ui.gap(2.0);
                ui.add(
                    Label::new("百度网盘 Rust 客户端")
                        .font_size(13.5)
                        .weight(FontWeight::MEDIUM)
                        .color(theme.text),
                );
                ui.add(
                    Label::new("v0.1.0 · kasumi GUI · wgpu")
                        .font_size(10.5)
                        .color(theme.text_subtle)
                        .family(FontFamily::Monospace),
                );
            });
        });
        ui.add(
            Label::new(
                "本程序使用百度网盘 8.9.7 Linux 客户端公开使用的网页接口，图标与品牌素材来自原客户端。非官方产品，请仅用于个人账号与学习研究；接口可能随时变化。",
            )
            .font_size(11.0)
            .line_height(18.0)
            .color(theme.text_muted),
        );
        ui.row(|ui| {
            ui.gap(8.0);
            components::pill(ui, "API 8.9.7", theme.surface, theme.text_subtle);
            components::pill(ui, "BDUSS 本地保存", theme.surface, theme.text_subtle);
            components::pill(ui, "不收集数据", theme.surface, theme.text_subtle);
        });
    });
}

fn stepper_value(ui: &mut Ui, theme: &Theme, value: usize) {
    ui.center(|ui| {
        ui.width(Length::Px(46.0))
            .height(30.0)
            .radius(8.0)
            .background(theme.surface)
            .border(1.0, theme.border);
        ui.add(
            Label::new(value.to_string())
                .font_size(12.5)
                .weight(FontWeight::MEDIUM)
                .color(theme.text),
        );
    });
}

fn stepper_plus(ui: &mut Ui, theme: &Theme, enabled: bool) -> bool {
    stepper_button(ui, theme, "+", enabled)
}

fn stepper_minus(ui: &mut Ui, theme: &Theme, enabled: bool) -> bool {
    stepper_button(ui, theme, "−", enabled)
}

fn stepper_button(ui: &mut Ui, theme: &Theme, glyph: &str, enabled: bool) -> bool {
    if !enabled {
        return false;
    }
    let hovered = ui.is_hovered();
    let response = ui.row(|ui| {
        ui.clickable()
            .width(30.0)
            .height(30.0)
            .align_items(Align::Center)
            .justify(Justify::Center)
            .radius(8.0)
            .background(if hovered {
                theme.surface_hover
            } else {
                theme.surface
            })
            .border(1.0, theme.border);
        ui.add(
            Label::new(glyph.to_owned())
                .font_size(15.0)
                .weight(FontWeight::MEDIUM)
                .color(theme.text_muted),
        );
    });
    response.clicked()
}
