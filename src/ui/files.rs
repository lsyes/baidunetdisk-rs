//! File browser: breadcrumb, toolbar, list/grid views and context menus.

use kasumi::prelude::*;
use kasumi::Image;
use kasumi_core::{Color, Key, Length, TextWrap};

use crate::app::{
    Action, BaiduApp, LoadState, SortKey, format_time, natural_cmp,
};
use crate::models::{Entry, human_size};
use crate::ui::components::{self, ActionStyle};

pub fn view(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    ui.column(|ui| {
        ui.width_full().height_full().min_height(0.0);
        toolbar(app, ui);
        if let Some(keyword) = app.search_keyword.clone() {
            search_banner(app, ui, &keyword);
        }
        ui.column(|ui| {
            ui.grow(1.0).width_full().min_height(0.0);
            match app.load_state {
                LoadState::Loading if app.visible_entries().is_empty() => {
                    loading(app, ui);
                }
                LoadState::Failed => error_state(app, ui),
                _ if app.visible_entries().is_empty() => empty(app, ui),
                _ => {
                    if app.settings.grid_view {
                        grid_view(app, ui);
                    } else {
                        list_view(app, ui);
                    }
                }
            }
        });
    });
}

fn toolbar(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let selection = selection_entries(app);
    let has_selection = !selection.is_empty();
    ui.column(|ui| {
        ui.width_full()
            .padding_edges(Edges {
                left: 20.0,
                right: 20.0,
                top: 10.0,
                bottom: 4.0,
            })
            .gap(10.0)
            .background(theme.background);
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(8.0);
            if components::icon_button(ui, &theme, app.icons.ui("back"), 34.0).clicked() {
                app.actions.push(Action::Back);
            }
            if components::icon_button(ui, &theme, app.icons.ui("forward"), 34.0).clicked() {
                app.actions.push(Action::Forward);
            }
            if components::icon_button(ui, &theme, app.icons.ui("arrow-up"), 34.0).clicked() {
                app.actions.push(Action::Up);
            }
            breadcrumb(app, ui);
            ui.spacer();
            if app.load_state == LoadState::Loading {
                components::spinner_text(ui, &theme, "加载中", app.now);
            }
            if components::icon_button(ui, &theme, app.icons.ui("refresh"), 34.0).clicked() {
                app.actions.push(Action::Refresh);
            }
            let view_icon = if app.settings.grid_view {
                "view-list"
            } else {
                "view-norm"
            };
            if components::icon_button(ui, &theme, app.icons.ui(view_icon), 34.0).clicked() {
                app.actions.push(Action::ToggleGrid);
            }
        });
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(8.0);
            if components::action_button(
                ui,
                &theme,
                None,
                "上传文件",
                ActionStyle::Primary,
                app.client.is_some(),
            )
            .clicked()
            {
                app.actions.push(Action::OpenPicker);
            }
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("add-folder")),
                "新建文件夹",
                ActionStyle::Secondary,
                app.client.is_some(),
            )
            .clicked()
            {
                app.actions.push(Action::NewFolder);
            }
            ui.row(|ui| {
                ui.width(6.0);
            });
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("download")),
                "下载",
                ActionStyle::Ghost,
                has_selection,
            )
            .clicked()
            {
                app.actions.push(Action::Download(selection.clone()));
            }
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("share")),
                "分享",
                ActionStyle::Ghost,
                has_selection,
            )
            .clicked()
            {
                app.actions.push(Action::ShareDialog(selection.clone()));
            }
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("delete")),
                "删除",
                ActionStyle::Danger,
                has_selection,
            )
            .clicked()
            {
                app.actions.push(Action::DeleteDialog(selection.clone()));
            }
            ui.spacer();
            if has_selection {
                ui.add(
                    Label::new(format!("已选择 {} 项", selection.len()))
                        .font_size(11.0)
                        .color(theme.accent),
                );
                if components::action_button(
                    ui,
                    &theme,
                    None,
                    "取消选择",
                    ActionStyle::Ghost,
                    true,
                )
                .clicked()
                {
                    app.selected.clear();
                }
            } else {
                ui.add(
                    Label::new("单击选择 · 再次单击打开 · 右键更多")
                        .font_size(10.5)
                        .color(theme.text_subtle),
                );
            }
        });
    });
}

fn breadcrumb(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let cwd = app.cwd.clone();
    let mut target: Option<String> = None;
    ui.row(|ui| {
        ui.height(34.0)
            .grow(1.0)
            .basis(Length::Px(0.0))
            .min_width(0.0)
            .align_items(Align::Center)
            .gap(2.0)
            .clip();
        let root = ui.row(|ui| {
            let hovered = ui.is_hovered();
            ui.clickable()
                .padding_xy(7.0, 5.0)
                .radius(6.0)
                .gap(5.0)
                .align_items(Align::Center)
                .background(if hovered {
                    theme.surface_hover
                } else {
                    Color::TRANSPARENT
                });
            ui.add(Image::from_shared(app.icons.ui("folder")).size(15.0, 15.0));
            ui.add(
                Label::new("全部文件")
                    .font_size(12.5)
                    .color(if cwd == "/" {
                        theme.text
                    } else {
                        theme.text_muted
                    })
                    .wrap(TextWrap::None),
            );
        });
        if root.clicked() && cwd != "/" {
            target = Some("/".to_owned());
        }
        let mut accumulated = String::new();
        for segment in cwd.split('/').filter(|segment| !segment.is_empty()) {
            accumulated.push('/');
            accumulated.push_str(segment);
            ui.add(
                Label::new("›")
                    .font_size(12.0)
                    .color(theme.text_subtle),
            );
            let path = accumulated.clone();
            let is_last = path == cwd;
            let item = ui.row(|ui| {
                let hovered = ui.is_hovered();
                ui.clickable()
                    .padding_xy(7.0, 5.0)
                    .radius(6.0)
                    .background(if hovered {
                        theme.surface_hover
                    } else {
                        Color::TRANSPARENT
                    });
                ui.add(
                    Label::new(segment.to_owned())
                        .font_size(12.5)
                        .weight(if is_last {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .color(if is_last { theme.text } else { theme.text_muted })
                        .wrap(TextWrap::None),
                );
            });
            if item.clicked() && !is_last {
                target = Some(path);
            }
        }
    });
    if let Some(path) = target {
        app.actions.push(Action::Navigate(path));
    }
}

fn search_banner(app: &mut BaiduApp, ui: &mut Ui, keyword: &str) {
    let theme = app.theme.clone();
    let count = app.search_entries.len();
    ui.row(|ui| {
        ui.width_full()
            .padding_xy(20.0, 4.0)
            .align_items(Align::Center)
            .gap(8.0);
        ui.add(Image::from_shared(app.icons.ui("search")).size(14.0, 14.0));
        ui.add(
            Label::new(format!("“{keyword}” 的搜索结果 · {count} 个"))
                .font_size(11.5)
                .color(theme.text_muted),
        );
        if components::action_button(
            ui,
            &theme,
            None,
            "清除搜索",
            ActionStyle::Ghost,
            true,
        )
        .clicked()
        {
            if let Some(inputs) = &app.inputs {
                inputs.search.set(String::new());
            }
            app.search_keyword = None;
            app.actions.push(Action::Refresh);
        }
    });
}

fn list_view(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    header_row(app, ui);
    let entries = app.visible_entries();
    let selected = app.selected.clone();
    let thumbnails = app.thumbnails.clone();
    let icons = &app.icons;
    let now = app.now;
    let last_click = app.last_click;
    let mut next_last_click = last_click;
    let mut next_selected = selected.clone();
    let mut next_active = app.active;
    let mut actions: Vec<Action> = Vec::new();
    let sort_key = app.sort_key();
    let sort_desc = app.sort_desc();
    ui.scroll(
        ScrollView::vertical().grow(1.0).smooth(true),
        |ui| {
            ui.width_full().padding_xy(20.0, 2.0).gap(2.0);
            if entries.is_empty() {
                return;
            }
            for entry in &entries {
                let is_selected = selected.contains(&entry.fs_id);
                let row_actions = &mut actions;
                let next_selected = &mut next_selected;
                let next_active = &mut next_active;
                let next_last_click = &mut next_last_click;
                let thumb = thumbnails.get(&entry.fs_id).cloned();
                let response = ui.row(|ui| {
                        let hovered = ui.is_hovered();
                        ui.clickable()
                            .width_full()
                            .height(Length::Px(46.0))
                            .padding_xy(10.0, 0.0)
                            .gap(10.0)
                            .align_items(Align::Center)
                            .radius(9.0)
                            .background(if is_selected {
                                theme.accent.fade(0.10)
                            } else if hovered {
                                theme.surface_hover
                            } else {
                                Color::TRANSPARENT
                            })
                            .clip();
                        let icon_data = match &thumb {
                            Some(thumb) => thumb.clone(),
                            None => icons.file_icon(entry.kind()),
                        };
                        ui.add(
                            Image::from_shared(icon_data)
                                .size(30.0, 30.0)
                                .fit(ImageFit::Contain),
                        );
                        ui.column(|ui| {
                            ui.grow(1.0).basis(Length::Px(0.0)).min_width(0.0).gap(1.0);
                            ui.add(
                                Label::new(entry.server_filename.clone())
                                    .font_size(13.0)
                                    .weight(if entry.is_dir() {
                                        FontWeight::MEDIUM
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .color(theme.text)
                                    .wrap(TextWrap::None),
                            );
                            if entry.is_dir() {
                                ui.add(
                                    Label::new("文件夹")
                                        .font_size(10.0)
                                        .color(theme.text_subtle),
                                );
                            }
                        });
                        ui.column(|ui| {
                            ui.width(Length::Px(120.0));
                            ui.add(
                                Label::new(if entry.is_dir() {
                                    "—".to_owned()
                                } else {
                                    human_size(entry.size)
                                })
                                .font_size(11.5)
                                .color(theme.text_muted)
                                .align(TextAlign::End)
                                .width(Length::Percent(100.0)),
                            );
                        });
                        ui.column(|ui| {
                            ui.width(Length::Px(160.0));
                            ui.add(
                                Label::new(format_time(entry.modified()))
                                    .font_size(11.0)
                                    .color(theme.text_subtle)
                                    .align(TextAlign::End)
                                    .width(Length::Percent(100.0)),
                            );
                        });
                        let more = components::icon_button(
                            ui,
                            &theme,
                            icons.ui("more"),
                            28.0,
                        );
                        if more.clicked() {
                            row_actions.push(Action::Actions(entry.clone()));
                        }
                });
                if response.clicked() {
                    let double = last_click
                        .map(|(id, time)| id == entry.fs_id && now - time < 0.4)
                        .unwrap_or(false);
                    if double {
                        row_actions.push(Action::Open(entry.clone()));
                        *next_last_click = None;
                    } else {
                        let toggle = ui.input().modifiers.ctrl || ui.input().modifiers.shift;
                        if toggle {
                            if !next_selected.remove(&entry.fs_id) {
                                next_selected.insert(entry.fs_id);
                            }
                        } else {
                            next_selected.clear();
                            next_selected.insert(entry.fs_id);
                        }
                        *next_active = Some(entry.fs_id);
                        *next_last_click = Some((entry.fs_id, now));
                    }
                }
                context_menu(
                    ui,
                    &theme,
                    &response,
                    entry,
                    &mut actions,
                );
            }
        },
    );
    app.selected = next_selected;
    app.active = next_active;
    app.last_click = next_last_click;
    app.actions.extend(actions);
    let _ = sort_key;
    let _ = sort_desc;
}

fn context_menu(
    ui: &mut Ui,
    theme: &Theme,
    owner: &Response,
    entry: &Entry,
    actions: &mut Vec<Action>,
) {
    let entry = entry.clone();
    ui.context_menu(owner, |ui| {
        let label = if entry.is_dir() { "打开" } else { "预览" };
        if ui.add(Button::new(label).menu_item()).clicked() {
            actions.push(Action::Open(entry.clone()));
        }
        if !entry.is_dir() {
            if ui.add(Button::new("下载").menu_item()).clicked() {
                actions.push(Action::Download(vec![entry.clone()]));
            }
            if ui.add(Button::new("分享").menu_item()).clicked() {
                actions.push(Action::ShareDialog(vec![entry.clone()]));
            }
        }
        if ui.add(Button::new("重命名").menu_item()).clicked() {
            actions.push(Action::RenameDialog(entry.clone()));
        }
        if ui.add(Button::new("移动到…").menu_item()).clicked() {
            actions.push(Action::MoveDialog(vec![entry.clone()]));
        }
        if ui.add(Button::new("复制到…").menu_item()).clicked() {
            actions.push(Action::CopyDialog(vec![entry.clone()]));
        }
        if ui.add(Button::new("详细信息").menu_item()).clicked() {
            actions.push(Action::Details(entry.clone()));
        }
        ui.row(|ui| {
            ui.width_full().height(1.0).background(theme.border);
        });
        if ui.add(Button::new("删除").menu_item()).clicked() {
            actions.push(Action::DeleteDialog(vec![entry.clone()]));
        }
    });
}

fn header_row(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let mut sort: Option<SortKey> = None;
    ui.row(|ui| {
        ui.width_full()
            .height(Length::Px(32.0))
            .padding_edges(Edges {
                left: 30.0,
                right: 30.0,
                top: 0.0,
                bottom: 0.0,
            })
            .align_items(Align::Center)
            .gap(10.0)
            .background(theme.surface)
            .border(1.0, theme.border);
        let label = sort_label("名称", SortKey::Name, app);
        if ui
            .row(|ui| {
                ui.clickable().grow(1.0).basis(Length::Px(0.0));
                ui.add(
                    Label::new(label)
                        .font_size(10.5)
                        .color(theme.text_subtle),
                );
            })
            .clicked()
        {
            sort = Some(SortKey::Name);
        }
        let label = sort_label("大小", SortKey::Size, app);
        if ui
            .row(|ui| {
                ui.clickable().width(Length::Px(120.0));
                ui.add(
                    Label::new(label)
                        .font_size(10.5)
                        .color(theme.text_subtle)
                        .align(TextAlign::End)
                        .width(Length::Percent(100.0)),
                );
            })
            .clicked()
        {
            sort = Some(SortKey::Size);
        }
        let label = sort_label("修改时间", SortKey::Time, app);
        if ui
            .row(|ui| {
                ui.clickable().width(Length::Px(160.0));
                ui.add(
                    Label::new(label)
                        .font_size(10.5)
                        .color(theme.text_subtle)
                        .align(TextAlign::End)
                        .width(Length::Percent(100.0)),
                );
            })
            .clicked()
        {
            sort = Some(SortKey::Time);
        }
        ui.row(|ui| {
            ui.width(Length::Px(28.0));
        });
    });
    if let Some(sort) = sort {
        app.actions.push(Action::Sort(sort));
    }
}

fn sort_label(label: &str, key: SortKey, app: &BaiduApp) -> String {
    if app.sort_key() == key {
        if app.sort_desc() {
            format!("{label} ↓")
        } else {
            format!("{label} ↑")
        }
    } else {
        label.to_owned()
    }
}

fn grid_view(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let entries = app.visible_entries();
    let selected = app.selected.clone();
    let thumbnails = app.thumbnails.clone();
    let icons = &app.icons;
    let now = app.now;
    let last_click = app.last_click;
    let mut next_last_click = last_click;
    let mut next_selected = selected.clone();
    let mut next_active = app.active;
    let mut actions: Vec<Action> = Vec::new();
    ui.scroll(
        ScrollView::vertical().grow(1.0).smooth(true),
        |ui| {
            ui.width_full().padding(20.0).gap(10.0);
            ui.wrap_row(|ui| {
                ui.width_full().gap(12.0);
                for entry in &entries {
                    let is_selected = selected.contains(&entry.fs_id);
                    let thumb = thumbnails.get(&entry.fs_id).cloned();
                    let response = ui.column(|ui| {
                            let hovered = ui.is_hovered();
                            ui.clickable()
                                .width(Length::Px(166.0))
                                .padding(10.0)
                                .gap(7.0)
                                .radius(12.0)
                                .background(if is_selected {
                                    theme.accent.fade(0.10)
                                } else if hovered {
                                    theme.surface_hover
                                } else {
                                    theme.panel
                                })
                                .border(
                                    1.0,
                                    if is_selected {
                                        theme.accent.fade(0.45)
                                    } else {
                                        theme.border
                                    },
                                )
                                .clip();
                            ui.center(|ui| {
                                ui.width_full()
                                    .height(Length::Px(92.0))
                                    .radius(8.0)
                                    .background(theme.surface);
                                match &thumb {
                                    Some(thumb) => {
                                        ui.add(
                                            Image::from_shared(thumb.clone())
                                                .size(146.0, 88.0)
                                                .fit(ImageFit::Cover)
                                                .radius(7.0),
                                        );
                                    }
                                    None => {
                                        ui.add(
                                            Image::from_shared(
                                                icons.file_icon(entry.kind()),
                                            )
                                            .size(54.0, 54.0),
                                        );
                                    }
                                }
                            });
                            ui.add(
                                Label::new(entry.server_filename.clone())
                                    .font_size(12.0)
                                    .color(theme.text)
                                    .width(Length::Percent(100.0))
                                    .wrap(TextWrap::None),
                            );
                            ui.row(|ui| {
                                ui.width_full();
                                ui.add(
                                    Label::new(if entry.is_dir() {
                                        "文件夹".to_owned()
                                    } else {
                                        human_size(entry.size)
                                    })
                                    .font_size(10.0)
                                    .color(theme.text_subtle),
                                );
                                ui.spacer();
                                ui.add(
                                    Label::new(format_time(entry.modified()))
                                        .font_size(9.5)
                                        .color(theme.text_subtle),
                                );
                            });
                    });
                    if response.clicked() {
                        let double = last_click
                            .map(|(id, time)| id == entry.fs_id && now - time < 0.4)
                            .unwrap_or(false);
                        if double {
                            actions.push(Action::Open(entry.clone()));
                            next_last_click = None;
                        } else {
                            let toggle =
                                ui.input().modifiers.ctrl || ui.input().modifiers.shift;
                            if toggle {
                                if !next_selected.remove(&entry.fs_id) {
                                    next_selected.insert(entry.fs_id);
                                }
                            } else {
                                next_selected.clear();
                                next_selected.insert(entry.fs_id);
                            }
                            next_active = Some(entry.fs_id);
                            next_last_click = Some((entry.fs_id, now));
                        }
                    }
                    context_menu(ui, &theme, &response, entry, &mut actions);
                }
            });
        },
    );
    app.selected = next_selected;
    app.active = next_active;
    app.last_click = next_last_click;
    app.actions.extend(actions);
}

fn loading(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    ui.center(|ui| {
        ui.width_full().height_full().gap(10.0);
        ui.add(
            Image::from_shared(app.icons.ui("loading")).size(40.0, 40.0),
        );
        components::spinner_text(ui, &theme, "正在加载目录", app.now);
    });
}

fn empty(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    ui.center(|ui| {
        ui.width_full().height_full().gap(10.0);
        ui.add(Image::from_shared(app.icons.ui("folder")).size(72.0, 72.0));
        ui.add(
            Label::new("这里还没有文件")
                .font_size(15.0)
                .weight(FontWeight::MEDIUM)
                .color(theme.text_muted),
        );
        ui.add(
            Label::new("点击上方“上传文件”把本地文件放到网盘")
                .font_size(12.0)
                .color(theme.text_subtle),
        );
    });
}

fn error_state(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let message = app
        .load_error
        .clone()
        .unwrap_or_else(|| "加载失败".to_owned());
    ui.center(|ui| {
        ui.width_full().height_full().gap(10.0);
        ui.add(Image::from_shared(app.icons.ui("warning")).size(48.0, 48.0));
        ui.add(
            Label::new(message)
                .font_size(12.5)
                .color(theme.danger)
                .align(TextAlign::Center),
        );
        if components::action_button(
            ui,
            &theme,
            Some(app.icons.ui("refresh")),
            "重试",
            ActionStyle::Secondary,
            true,
        )
        .clicked()
        {
            app.actions.push(Action::Refresh);
        }
    });
}

fn selection_entries(app: &BaiduApp) -> Vec<Entry> {
    let mut selection = app.selected_entries();
    if selection.is_empty()
        && let Some(active) = app.active
        && let Some(entry) = app.entry_by_id(active)
    {
        selection.push(entry);
    }
    selection
}

pub fn key_shortcuts(app: &mut BaiduApp, ui: &mut Ui) {
    if app.page != crate::app::Page::Files {
        return;
    }
    if ui.key_modifiers(&Key::Character("a".to_owned())).is_some_and(|m| m.ctrl || m.logo) {
        app.selected = app
            .visible_entries()
            .iter()
            .map(|entry| entry.fs_id)
            .collect();
    }
    let _ = natural_cmp("a", "a");
}
