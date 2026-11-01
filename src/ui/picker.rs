//! In-app local file picker for uploads.

use std::path::PathBuf;

use kasumi::prelude::*;
use kasumi::Image;
use kasumi_core::{Color, Length, TextWrap};

use crate::app::{Action, BaiduApp, format_time};
use crate::models::human_size;
use crate::ui::components::{self, ActionStyle};

pub fn view(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let Some(picker) = app.picker.clone() else {
        ui.center(|ui| {
            ui.width_full().height_full();
            ui.add(
                Label::new("没有打开本地文件选择器")
                    .font_size(13.0)
                    .color(theme.text_subtle),
            );
        });
        return;
    };
    let mut actions: Vec<Action> = Vec::new();
    let mut next_selected = picker.selected.clone();
    ui.column(|ui| {
        ui.width_full()
            .height_full()
            .padding_edges(Edges {
                left: 20.0,
                right: 20.0,
                top: 14.0,
                bottom: 12.0,
            })
            .gap(10.0);
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(8.0);
            if components::icon_button(ui, &theme, app.icons.ui("back"), 34.0).clicked() {
                actions.push(Action::SetNav(crate::app::Nav::Files));
            }
            ui.column(|ui| {
                ui.gap(2.0);
                ui.add(
                    Label::new("选择要上传的文件")
                        .font_size(20.0)
                        .weight(FontWeight::SEMIBOLD)
                        .color(theme.text),
                );
                ui.add(
                    Label::new(format!(
                        "上传目标：{}  ·  当前目录：{}",
                        app.cwd,
                        picker.cwd.display()
                    ))
                    .font_size(10.5)
                    .color(theme.text_subtle),
                );
            });
            ui.spacer();
            if components::icon_button(ui, &theme, app.icons.ui("arrow-up"), 34.0).clicked()
                && let Some(parent) = picker.cwd.parent()
            {
                actions.push(Action::PickerEnter(parent.to_path_buf()));
            }
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("folder")),
                "上传当前文件夹",
                ActionStyle::Secondary,
                true,
            )
            .clicked()
            {
                actions.push(Action::PickerUploadDir(picker.cwd.clone()));
            }
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("upload")),
                format!("上传选中 ({})", picker.selected.len()),
                ActionStyle::Primary,
                !picker.selected.is_empty(),
            )
            .clicked()
            {
                actions.push(Action::PickerUpload);
            }
        });
        components::separator(ui, &theme);
        ui.row(|ui| {
            ui.width_full()
                .padding_xy(10.0, 6.0)
                .radius(8.0)
                .background(theme.surface)
                .align_items(Align::Center)
                .gap(2.0)
                .clip();
            if components::icon_button(ui, &theme, app.icons.ui("folder"), 24.0).clicked() {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_owned());
                actions.push(Action::PickerEnter(PathBuf::from(home)));
            }
            for segment in local_segments(&picker.cwd) {
                let path = segment.0.clone();
                let label = segment.1;
                ui.add(
                    Label::new("›")
                        .font_size(10.5)
                        .color(theme.text_subtle),
                );
                if ui
                    .row(|ui| {
                        ui.clickable().padding_xy(4.0, 2.0).radius(5.0);
                        ui.add(
                            Label::new(label)
                                .font_size(11.0)
                                .color(theme.text_muted)
                                .wrap(TextWrap::None),
                        );
                    })
                    .clicked()
                {
                    actions.push(Action::PickerEnter(path));
                }
            }
        });
        if let Some(error) = picker.error.clone() {
            ui.add(
                Label::new(error)
                    .font_size(11.0)
                    .color(theme.danger),
            );
        }
        let entries = picker.entries.clone();
        let selected = picker.selected.clone();
        let icons = &app.icons;
        let mut row_actions: Vec<Action> = Vec::new();
        ui.scroll(
            ScrollView::vertical().grow(1.0).smooth(true),
            |ui| {
                ui.width_full().gap(2.0);
                for entry in &entries {
                    let is_selected = selected.contains(&entry.path);
                    let response = ui.row(|ui| {
                            let hovered = ui.is_hovered();
                            ui.clickable()
                                .width_full()
                                .height(44.0)
                                .padding_xy(10.0, 0.0)
                                .gap(10.0)
                                .align_items(Align::Center)
                                .radius(8.0)
                                .background(if is_selected {
                                    theme.accent.fade(0.10)
                                } else if hovered {
                                    theme.surface_hover
                                } else {
                                    Color::TRANSPARENT
                                });
                            ui.center(|ui| {
                                ui.width(20.0).height(20.0).radius(5.0).border(
                                    1.0,
                                    if is_selected {
                                        theme.accent
                                    } else {
                                        theme.border_strong
                                    },
                                );
                                if is_selected {
                                    ui.add(
                                        Label::new("✓")
                                            .font_size(11.0)
                                            .weight(FontWeight::BOLD)
                                            .color(theme.accent),
                                    );
                                }
                            });
                            ui.add(
                                Image::from_shared(
                                    if entry.is_dir {
                                        icons.file_icon(crate::models::FileKind::Folder)
                                    } else {
                                        icons.file_icon(
                                            crate::models::FileKind::from_extension(
                                                entry.path
                                                    .extension()
                                                    .and_then(|ext| ext.to_str())
                                                    .unwrap_or(""),
                                            ),
                                        )
                                    },
                                )
                                .size(26.0, 26.0),
                            );
                            ui.column(|ui| {
                                ui.grow(1.0).basis(Length::Px(0.0)).min_width(0.0);
                                ui.add(
                                    Label::new(entry.name.clone())
                                        .font_size(12.5)
                                        .color(theme.text)
                                        .wrap(TextWrap::None),
                                );
                            });
                            ui.add(
                                Label::new(if entry.is_dir {
                                    "文件夹".to_owned()
                                } else {
                                    human_size(entry.size)
                                })
                                .font_size(10.5)
                                .color(theme.text_subtle)
                                .width(Length::Px(100.0))
                                .align(TextAlign::End),
                            );
                            ui.add(
                                Label::new(format_time(entry.modified))
                                    .font_size(10.0)
                                    .color(theme.text_subtle)
                                    .width(Length::Px(150.0))
                                    .align(TextAlign::End),
                            );
                            if entry.is_dir
                                && components::action_button(
                                    ui,
                                    &theme,
                                    None,
                                    "进入",
                                    ActionStyle::Ghost,
                                    true,
                                )
                                .clicked()
                            {
                                row_actions.push(Action::PickerEnter(entry.path.clone()));
                            }
                    });
                    if response.clicked() {
                        if entry.is_dir {
                            actions.push(Action::PickerEnter(entry.path.clone()));
                        } else if !next_selected.remove(&entry.path) {
                            next_selected.insert(entry.path.clone());
                        }
                    }
                }
            },
        );
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center);
            ui.add(
                Label::new(format!("已选择 {} 个文件", next_selected.len()))
                    .font_size(10.5)
                    .color(theme.text_muted),
            );
            ui.spacer();
            if components::action_button(
                ui,
                &theme,
                None,
                "取消",
                ActionStyle::Ghost,
                true,
            )
            .clicked()
            {
                actions.push(Action::SetNav(crate::app::Nav::Files));
            }
        });
        actions.extend(row_actions);
    });
    if let Some(picker) = &mut app.picker {
        picker.selected = next_selected;
    }
    app.actions.extend(actions);
}

fn local_segments(path: &std::path::Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut accumulated = PathBuf::new();
    for component in path.components() {
        let name = match component {
            std::path::Component::RootDir => "/".to_owned(),
            std::path::Component::Normal(name) => name.to_string_lossy().to_string(),
            _ => continue,
        };
        accumulated.push(match component {
            std::path::Component::RootDir => std::path::Path::new("/"),
            _ => std::path::Path::new(&name),
        });
        out.push((accumulated.clone(), name));
    }
    out
}
