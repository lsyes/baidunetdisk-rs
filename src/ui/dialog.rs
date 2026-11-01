//! Page-level dialogs: create folder, rename, delete, share, move/copy,
//! per-file actions and details.

use kasumi::prelude::*;
use kasumi::Image;
use kasumi_core::{Color, Length, TextWrap};

use crate::app::{Action, BaiduApp, DialogKind};
use crate::models::{Entry, FileKind, human_size, format_timestamp};
use crate::ui::components::{self, ActionStyle};

pub fn view(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let Some(dialog) = app.dialog.clone() else {
        app.page = crate::app::Page::Files;
        return;
    };
    let inputs = app.inputs().clone();
    let mut actions: Vec<Action> = Vec::new();
    ui.column(|ui| {
        ui.width_full()
            .height_full()
            .padding(24.0)
            .gap(10.0);
        ui.row(|ui| {
            if components::icon_button(ui, &theme, app.icons.ui("back"), 34.0).clicked() {
                actions.push(Action::CloseDialog);
            }
            ui.add(
                Label::new(dialog.title.clone())
                    .font_size(20.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(theme.text),
            );
        });
        ui.center(|ui| {
            ui.grow(1.0).width_full();
            match dialog.kind {
                DialogKind::NewFolder => new_folder(app, ui, &theme, &inputs, &mut actions),
                DialogKind::Rename => rename(app, ui, &theme, &inputs, &mut actions),
                DialogKind::Delete => delete(app, ui, &theme, &dialog, &mut actions),
                DialogKind::Share => share(app, ui, &theme, &inputs, &dialog, &mut actions),
                DialogKind::Move | DialogKind::Copy => {
                    move_copy(ui, &theme, &inputs, &dialog, &mut actions)
                }
                DialogKind::Actions => actions_dialog(app, ui, &theme, &dialog, &mut actions),
                DialogKind::Details => details(app, ui, &theme, &dialog, &mut actions),
            }
        });
    });
    app.actions.extend(actions);
}

fn new_folder(
    app: &mut BaiduApp,
    ui: &mut Ui,
    theme: &Theme,
    inputs: &crate::app::Inputs,
    actions: &mut Vec<Action>,
) {
    ui.column(|ui| {
        ui.width(440.0).padding(22.0).gap(13.0).radius(16.0).background(theme.panel).border(1.0, theme.border);
        ui.add(
            Label::new("在当前位置创建一个新文件夹")
                .font_size(12.0)
                .color(theme.text_subtle),
        );
        ui.add(
            Label::new(format!("位置：{}", app.cwd))
                .font_size(10.5)
                .color(theme.text_subtle)
                .family(FontFamily::Monospace),
        );
        components::field_label(ui, theme, "文件夹名称");
        ui.add(
            TextInput::new(&inputs.new_folder)
                .placeholder("例如：我的资料")
                .width(Length::Percent(100.0))
                .height(38.0),
        );
        dialog_buttons(theme, ui, "创建", false, actions);
    });
}

fn rename(
    app: &mut BaiduApp,
    ui: &mut Ui,
    theme: &Theme,
    inputs: &crate::app::Inputs,
    actions: &mut Vec<Action>,
) {
    let target = app.dialog.as_ref().and_then(|dialog| dialog.targets.first().cloned());
    ui.column(|ui| {
        ui.width(460.0).padding(22.0).gap(13.0).radius(16.0).background(theme.panel).border(1.0, theme.border);
        if let Some(target) = &target {
            ui.row(|ui| {
                ui.gap(9.0).align_items(Align::Center);
                ui.add(Image::from_shared(app.icons.file_icon(target.kind())).size(28.0, 28.0));
                ui.add(
                    Label::new(target.server_filename.clone())
                        .font_size(12.5)
                        .color(theme.text_muted)
                        .wrap(TextWrap::None),
                );
            });
            components::field_label(ui, theme, "新名称");
            ui.add(
                TextInput::new(&inputs.rename)
                    .placeholder("输入新名称")
                    .width(Length::Percent(100.0))
                    .height(38.0),
            );
        } else {
            ui.add(Label::new("没有选择文件").font_size(12.0).color(theme.danger));
        }
        dialog_buttons(theme, ui, "重命名", false, actions);
    });
}

fn delete(
    _app: &mut BaiduApp,
    ui: &mut Ui,
    theme: &Theme,
    dialog: &crate::app::DialogState,
    actions: &mut Vec<Action>,
) {
    ui.column(|ui| {
        ui.width(480.0).padding(22.0).gap(13.0).radius(16.0).background(theme.panel).border(1.0, theme.border);
        ui.row(|ui| {
            ui.width_full()
                .padding(10.0)
                .gap(8.0)
                .radius(9.0)
                .background(theme.danger.fade(0.08))
                .align_items(Align::Start);
            ui.add(
                Label::new("!")
                    .font_size(15.0)
                    .weight(FontWeight::BOLD)
                    .color(theme.danger),
            );
            ui.add(
                Label::new(format!(
                    "确定要删除以下 {} 个项目吗？",
                    dialog.targets.len()
                ))
                .font_size(12.5)
                .weight(FontWeight::MEDIUM)
                .color(theme.danger),
            );
        });
        ui.column(|ui| {
            ui.width_full()
                .padding(12.0)
                .gap(5.0)
                .radius(9.0)
                .background(theme.surface);
            for entry in dialog.targets.iter().take(12) {
                ui.add(
                    Label::new(entry.path.clone())
                        .font_size(11.0)
                        .color(theme.text_muted)
                        .family(FontFamily::Monospace)
                        .wrap(TextWrap::None),
                );
            }
            if dialog.targets.len() > 12 {
                ui.add(
                    Label::new(format!("…以及另外 {} 个项目", dialog.targets.len() - 12))
                        .font_size(10.5)
                        .color(theme.text_subtle),
                );
            }
        });
        ui.add(
            Label::new("删除后文件会进入网盘回收站，可在网页端恢复。")
                .font_size(10.5)
                .color(theme.text_subtle),
        );
        dialog_buttons(theme, ui, "删除", true, actions);
    });
}

fn share(
    app: &mut BaiduApp,
    ui: &mut Ui,
    theme: &Theme,
    inputs: &crate::app::Inputs,
    dialog: &crate::app::DialogState,
    actions: &mut Vec<Action>,
) {
    let mut period = dialog.period;
    let mut public = dialog.public;
    ui.column(|ui| {
        ui.width(500.0).padding(22.0).gap(13.0).radius(16.0).background(theme.panel).border(1.0, theme.border);
        ui.add(
            Label::new(format!("分享 {} 个项目", dialog.targets.len()))
                .font_size(12.5)
                .color(theme.text_muted),
        );
        components::field_label(ui, theme, "提取码（4 位，留空表示无提取码）");
        ui.row(|ui| {
            ui.width_full().gap(8.0).align_items(Align::Center);
            ui.add(
                TextInput::new(&inputs.share_pwd)
                    .placeholder("例如 a1b2")
                    .width(Length::Px(180.0))
                    .height(38.0),
            );
            if components::action_button(ui, theme, None, "随机生成", ActionStyle::Ghost, true).clicked() {
                inputs.share_pwd.set(random_pwd());
            }
        });
        components::field_label(ui, theme, "有效期");
        ui.row(|ui| {
            ui.gap(8.0);
            for (label, value) in [("永久有效", 0u32), ("7 天", 1), ("30 天", 2)] {
                let active = period == value;
                let hovered = ui.is_hovered();
                let response = ui.row(|ui| {
                    ui.clickable()
                        .padding_xy(12.0, 7.0)
                        .radius(8.0)
                        .background(if active {
                            theme.accent.fade(0.12)
                        } else if hovered {
                            theme.surface_hover
                        } else {
                            theme.surface
                        })
                        .border(
                            1.0,
                            if active { theme.accent.fade(0.5) } else { theme.border },
                        );
                    ui.add(
                        Label::new(label)
                            .font_size(11.5)
                            .weight(if active {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .color(if active { theme.accent } else { theme.text_muted }),
                    );
                });
                if response.clicked() {
                    period = value;
                    if let Some(dialog) = &mut app.dialog {
                        dialog.period = value;
                    }
                }
            }
        });
        if components::checkbox(ui, theme, public, "公开分享（任何人可通过链接访问）", true) {
            public = !public;
            if let Some(dialog) = &mut app.dialog {
                dialog.public = public;
            }
        }
        ui.add(
            Label::new("创建成功后会生成 https://pan.baidu.com/s/1… 链接并自动复制到剪贴板。")
                .font_size(10.5)
                .color(theme.text_subtle),
        );
        dialog_buttons(theme, ui, "创建分享", false, actions);
    });
}

fn move_copy(
    ui: &mut Ui,
    theme: &Theme,
    inputs: &crate::app::Inputs,
    dialog: &crate::app::DialogState,
    actions: &mut Vec<Action>,
) {
    let is_move = dialog.kind == DialogKind::Move;
    ui.column(|ui| {
        ui.width(480.0).padding(22.0).gap(13.0).radius(16.0).background(theme.panel).border(1.0, theme.border);
        ui.add(
            Label::new(format!(
                "{} {} 个项目到目标目录",
                if is_move { "移动" } else { "复制" },
                dialog.targets.len()
            ))
            .font_size(12.5)
            .color(theme.text_muted),
        );
        components::field_label(ui, theme, "目标目录（绝对路径）");
        ui.add(
            TextInput::new(&inputs.move_dest)
                .placeholder("/目标文件夹")
                .width(Length::Percent(100.0))
                .height(38.0),
        );
        ui.add(
            Label::new("可先进入目标文件夹，再执行移动/复制；也可以直接输入完整路径。")
                .font_size(10.5)
                .color(theme.text_subtle),
        );
        dialog_buttons(
            theme,
            ui,
            if is_move { "移动" } else { "复制" },
            false,
            actions,
        );
    });
}

fn actions_dialog(
    app: &mut BaiduApp,
    ui: &mut Ui,
    theme: &Theme,
    dialog: &crate::app::DialogState,
    actions: &mut Vec<Action>,
) {
    let Some(target) = dialog.targets.first().cloned() else {
        return;
    };
    ui.column(|ui| {
        ui.width(440.0).padding(22.0).gap(13.0).radius(16.0).background(theme.panel).border(1.0, theme.border);
        ui.row(|ui| {
            ui.gap(10.0).align_items(Align::Center);
            ui.add(Image::from_shared(app.icons.file_icon(target.kind())).size(34.0, 34.0));
            ui.column(|ui| {
                ui.gap(2.0);
                ui.add(
                    Label::new(target.server_filename.clone())
                        .font_size(13.0)
                        .weight(FontWeight::MEDIUM)
                        .color(theme.text)
                        .wrap(TextWrap::None),
                );
                ui.add(
                    Label::new(format!(
                        "{} · {}",
                        target.kind().label(),
                        if target.is_dir() {
                            "文件夹".to_owned()
                        } else {
                            human_size(target.size)
                        }
                    ))
                    .font_size(10.0)
                    .color(theme.text_subtle),
                );
            });
        });
        components::separator(ui, theme);
        let mut close = |app: &mut BaiduApp, actions: &mut Vec<Action>, action: Action| {
            app.dialog = None;
            app.page = crate::app::Page::Files;
            app.nav = crate::app::Nav::Files;
            actions.push(action);
        };
        if components::action_button(
            ui,
            theme,
            Some(app.icons.ui(if target.is_dir() { "folder" } else { "open" })),
            if target.is_dir() { "打开文件夹" } else { "预览" },
            ActionStyle::Ghost,
            true,
        )
        .clicked()
        {
            close(app, actions, Action::Open(target.clone()));
        }
        if !target.is_dir()
            && components::action_button(
                ui,
                theme,
                Some(app.icons.ui("download")),
                "下载",
                ActionStyle::Ghost,
                true,
            )
            .clicked()
        {
            close(app, actions, Action::Download(vec![target.clone()]));
        }
        if components::action_button(
            ui,
            theme,
            Some(app.icons.ui("share")),
            "分享",
            ActionStyle::Ghost,
            true,
        )
        .clicked()
        {
            close(app, actions, Action::ShareDialog(vec![target.clone()]));
        }
        if components::action_button(
            ui,
            theme,
            Some(app.icons.ui("rename")),
            "重命名",
            ActionStyle::Ghost,
            true,
        )
        .clicked()
        {
            close(app, actions, Action::RenameDialog(target.clone()));
        }
        if components::action_button(
            ui,
            theme,
            Some(app.icons.ui("move")),
            "移动到…",
            ActionStyle::Ghost,
            true,
        )
        .clicked()
        {
            close(app, actions, Action::MoveDialog(vec![target.clone()]));
        }
        if components::action_button(
            ui,
            theme,
            Some(app.icons.ui("copy")),
            "复制到…",
            ActionStyle::Ghost,
            true,
        )
        .clicked()
        {
            close(app, actions, Action::CopyDialog(vec![target.clone()]));
        }
        if components::action_button(
            ui,
            theme,
            Some(app.icons.ui("info")),
            "详细信息",
            ActionStyle::Ghost,
            true,
        )
        .clicked()
        {
            close(app, actions, Action::Details(target.clone()));
        }
        components::separator(ui, theme);
        if components::action_button(
            ui,
            theme,
            Some(app.icons.ui("delete")),
            "删除",
            ActionStyle::Danger,
            true,
        )
        .clicked()
        {
            close(app, actions, Action::DeleteDialog(vec![target.clone()]));
        }
        if components::action_button(ui, theme, None, "关闭", ActionStyle::Secondary, true).clicked() {
            actions.push(Action::CloseDialog);
        }
    });
}

fn details(
    app: &mut BaiduApp,
    ui: &mut Ui,
    theme: &Theme,
    dialog: &crate::app::DialogState,
    actions: &mut Vec<Action>,
) {
    let Some(target) = dialog.targets.first().cloned() else {
        return;
    };
    ui.column(|ui| {
        ui.width(540.0).padding(22.0).gap(13.0).radius(16.0).background(theme.panel).border(1.0, theme.border);
        ui.row(|ui| {
            ui.gap(12.0).align_items(Align::Center);
            ui.add(Image::from_shared(app.icons.file_icon(target.kind())).size(44.0, 44.0));
            ui.column(|ui| {
                ui.gap(2.0);
                ui.add(
                    Label::new(target.server_filename.clone())
                        .font_size(14.0)
                        .weight(FontWeight::MEDIUM)
                        .color(theme.text),
                );
                ui.add(
                    Label::new(target.path.clone())
                        .font_size(10.5)
                        .color(theme.text_subtle)
                        .family(FontFamily::Monospace),
                );
            });
        });
        components::separator(ui, theme);
        components::key_value(ui, theme, "类型", target.kind().label());
        components::key_value(
            ui,
            theme,
            "大小",
            &if target.is_dir() {
                "文件夹".to_owned()
            } else {
                human_size(target.size)
            },
        );
        components::key_value(
            ui,
            theme,
            "修改时间",
            &format_timestamp(target.modified()),
        );
        components::key_value(ui, theme, "创建时间", &format_timestamp(target.server_ctime));
        components::key_value(ui, theme, "fs_id", &target.fs_id.to_string());
        components::key_value(
            ui,
            theme,
            "MD5",
            if target.md5.is_empty() || !looks_md5(&target.md5) {
                "—"
            } else {
                &target.md5
            },
        );
        components::key_value(ui, theme, "路径", &target.path);
        ui.row(|ui| {
            ui.gap(8.0).align_items(Align::Center);
            if components::action_button(
                ui,
                theme,
                Some(app.icons.ui("copy")),
                "复制路径",
                ActionStyle::Ghost,
                true,
            )
            .clicked()
            {
                actions.push(Action::CopyText(target.path.clone()));
            }
            if !target.is_dir() {
                if components::action_button(
                    ui,
                    theme,
                    Some(app.icons.ui("download")),
                    "下载",
                    ActionStyle::Primary,
                    true,
                )
                .clicked()
                {
                    actions.push(Action::Download(vec![target.clone()]));
                }
                if looks_md5(&target.md5)
                    && components::action_button(
                        ui,
                        theme,
                        None,
                        "复制 MD5",
                        ActionStyle::Ghost,
                        true,
                    )
                    .clicked()
                {
                    actions.push(Action::CopyText(target.md5.clone()));
                }
            }
            ui.spacer();
            if components::action_button(ui, theme, None, "关闭", ActionStyle::Secondary, true).clicked() {
                actions.push(Action::CloseDialog);
            }
        });
    });
}

fn dialog_buttons(
    theme: &Theme,
    ui: &mut Ui,
    primary: &str,
    danger: bool,
    actions: &mut Vec<Action>,
) {
    ui.row(|ui| {
        ui.width_full().gap(8.0).justify(Justify::End);
        if components::action_button(ui, theme, None, "取消", ActionStyle::Secondary, true).clicked() {
            actions.push(Action::CloseDialog);
        }
        if components::action_button(
            ui,
            theme,
            None,
            primary,
            if danger {
                ActionStyle::Danger
            } else {
                ActionStyle::Primary
            },
            true,
        )
        .clicked()
        {
            actions.push(Action::ConfirmDialog);
        }
    });
}

fn looks_md5(value: &str) -> bool {
    value.len() == 32 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn random_pwd() -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..4)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect()
}

