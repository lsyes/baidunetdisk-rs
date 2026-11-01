//! Share management: my shares, opening a share link, browse and save.

use kasumi::prelude::*;
use kasumi::Image;
use kasumi_core::{Color, Length, TextWrap};

use crate::app::{Action, BaiduApp, LoadState, format_time};
use crate::models::{Entry, human_size};
use crate::share::ShareSummary;
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
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(10.0);
            ui.column(|ui| {
                ui.gap(2.0);
                ui.add(
                    Label::new("我的分享")
                        .font_size(22.0)
                        .weight(FontWeight::SEMIBOLD)
                        .color(theme.text),
                );
                ui.add(
                    Label::new("管理已创建的分享，或打开别人的分享链接")
                        .font_size(11.0)
                        .color(theme.text_subtle),
                );
            });
            ui.spacer();
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("refresh")),
                "刷新",
                ActionStyle::Ghost,
                true,
            )
            .clicked()
            {
                app.actions.push(Action::RefreshShares);
            }
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("link")),
                "打开分享链接",
                ActionStyle::Primary,
                true,
            )
            .clicked()
            {
                app.actions.push(Action::SetPage(crate::app::Page::OpenShare));
            }
        });
        ui.scroll(
            ScrollView::vertical().grow(1.0).smooth(true),
            |ui| {
                ui.width_full().gap(10.0);
                match app.shares_state {
                    LoadState::Loading if app.my_shares.is_empty() => {
                        ui.center(|ui| {
                            ui.width_full().height(220.0);
                            components::spinner_text(ui, &theme, "正在加载分享列表", app.now);
                        });
                    }
                    LoadState::Failed => {
                        ui.center(|ui| {
                            ui.width_full().height(220.0).gap(8.0);
                            ui.add(Image::from_shared(app.icons.ui("warning")).size(40.0, 40.0));
                            ui.add(
                                Label::new(
                                    app.shares_error
                                        .clone()
                                        .unwrap_or_else(|| "加载分享列表失败".to_owned()),
                                )
                                .font_size(12.0)
                                .color(theme.danger),
                            );
                            if components::action_button(
                                ui,
                                &theme,
                                None,
                                "重试",
                                ActionStyle::Secondary,
                                true,
                            )
                            .clicked()
                            {
                                app.actions.push(Action::RefreshShares);
                            }
                        });
                    }
                    _ if app.my_shares.is_empty() => {
                        components::empty_state(
                            ui,
                            &theme,
                            app.icons.ui("share"),
                            "还没有创建分享",
                            "在“我的文件”里右键文件，选择“分享”即可创建带提取码的链接",
                        );
                    }
                    _ => {
                        let shares = app.my_shares.clone();
                        for share in shares {
                            share_card(app, ui, &share);
                        }
                    }
                }
            },
        );
    });
}

fn share_card(app: &mut BaiduApp, ui: &mut Ui, share: &ShareSummary) {
    let theme = app.theme.clone();
    let title = if share.names.is_empty() {
        format!("分享 #{}", share.share_id)
    } else if share.names.len() == 1 {
        share.names[0].clone()
    } else {
        format!("{} 等 {} 个文件", share.names[0], share.names.len())
    };
    let mut actions: Vec<Action> = Vec::new();
    ui.keyed(share.share_id, |ui| {
        ui.column(|ui| {
            ui.width_full()
                .padding(14.0)
                .gap(9.0)
                .radius(13.0)
                .background(theme.panel)
                .border(1.0, theme.border);
            ui.row(|ui| {
                ui.width_full().align_items(Align::Center).gap(11.0);
                ui.center(|ui| {
                    ui.width(38.0)
                        .height(38.0)
                        .radius(10.0)
                        .background(theme.accent.fade(0.12));
                    ui.add(Image::from_shared(app.icons.ui("share")).size(20.0, 20.0));
                });
                ui.column(|ui| {
                    ui.grow(1.0).basis(Length::Px(0.0)).min_width(0.0).gap(3.0);
                    ui.add(
                        Label::new(title.clone())
                            .font_size(13.5)
                            .weight(FontWeight::MEDIUM)
                            .color(theme.text)
                            .wrap(TextWrap::None),
                    );
                    ui.row(|ui| {
                        ui.gap(10.0);
                        ui.add(
                            Label::new(format!("创建于 {}", format_time(share.ctime)))
                                .font_size(10.5)
                                .color(theme.text_subtle),
                        );
                        ui.add(
                            Label::new(share.period_label())
                                .font_size(10.5)
                                .color(theme.text_subtle),
                        );
                        if share.pwd.is_empty() {
                            components::pill(
                                ui,
                                "无提取码",
                                theme.surface_hover,
                                theme.text_subtle,
                            );
                        } else {
                            components::pill(
                                ui,
                                format!("提取码 {}", share.pwd),
                                theme.accent.fade(0.10),
                                theme.accent,
                            );
                        }
                    });
                });
                if components::action_button(
                    ui,
                    &theme,
                    Some(app.icons.ui("open")),
                    "打开",
                    ActionStyle::Secondary,
                    true,
                )
                .clicked()
                {
                    actions.push(Action::OpenShareSummary(share.clone()));
                }
                if components::action_button(
                    ui,
                    &theme,
                    Some(app.icons.ui("copy")),
                    "复制链接",
                    ActionStyle::Ghost,
                    true,
                )
                .clicked()
                {
                    actions.push(Action::CopyText(share.link()));
                }
                if components::action_button(
                    ui,
                    &theme,
                    None,
                    "取消分享",
                    ActionStyle::Danger,
                    true,
                )
                .clicked()
                {
                    actions.push(Action::CancelShare(share.share_id));
                }
            });
            ui.row(|ui| {
                ui.width_full()
                    .padding_xy(10.0, 6.0)
                    .radius(8.0)
                    .background(theme.surface)
                    .align_items(Align::Center)
                    .gap(8.0);
                ui.add(
                    Label::new(share.link())
                        .font_size(10.5)
                        .family(FontFamily::Monospace)
                        .color(theme.text_muted)
                        .wrap(TextWrap::None),
                );
                ui.spacer();
                if components::action_button(
                    ui,
                    &theme,
                    None,
                    "复制",
                    ActionStyle::Ghost,
                    true,
                )
                .clicked()
                {
                    actions.push(Action::CopyText(share.link()));
                }
            });
        });
    });
    app.actions.extend(actions);
}

pub fn open_share_view(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let inputs = app.inputs().clone();
    ui.column(|ui| {
        ui.width_full()
            .height_full()
            .padding(24.0)
            .gap(12.0);
        ui.row(|ui| {
            if components::icon_button(ui, &theme, app.icons.ui("back"), 34.0).clicked() {
                app.actions.push(Action::SetNav(crate::app::Nav::Shares));
            }
            ui.add(
                Label::new("打开分享链接")
                    .font_size(20.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(theme.text),
            );
        });
        ui.center(|ui| {
            ui.grow(1.0).width_full();
            ui.column(|ui| {
                ui.width(Length::Px(540.0))
                    .padding(24.0)
                    .gap(13.0)
                    .radius(16.0)
                    .background(theme.panel)
                    .border(1.0, theme.border);
                ui.row(|ui| {
                    ui.gap(10.0).align_items(Align::Center);
                    ui.add(Image::from_shared(app.icons.ui("link")).size(20.0, 20.0));
                    ui.add(
                        Label::new("粘贴百度网盘分享链接")
                            .font_size(15.0)
                            .weight(FontWeight::SEMIBOLD)
                            .color(theme.text),
                    );
                });
                components::field_label(ui, &theme, "分享链接");
                ui.add(
                    TextInput::new(&inputs.share_link)
                        .placeholder("https://pan.baidu.com/s/1xxxx 或 surl")
                        .width(Length::Percent(100.0))
                        .height(38.0),
                );
                components::field_label(ui, &theme, "提取码（可选）");
                ui.add(
                    TextInput::new(&inputs.share_link_pwd)
                        .placeholder("4 位提取码")
                        .width(Length::Px(180.0))
                        .height(38.0),
                );
                ui.add(
                    Label::new("链接中的 ?pwd= 会被自动识别；需要提取码的分享会在打开时验证。")
                        .font_size(10.5)
                        .color(theme.text_subtle)
                        .line_height(16.0),
                );
                if components::action_button(
                    ui,
                    &theme,
                    Some(app.icons.ui("open")),
                    "打开",
                    ActionStyle::Primary,
                    true,
                )
                .clicked()
                {
                    app.actions.push(Action::OpenShareLink);
                }
            });
        });
    });
}

pub fn browser_view(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let Some(browser) = app.share_browser.clone() else {
        ui.center(|ui| {
            ui.width_full().height_full();
            ui.add(
                Label::new("没有正在浏览的分享")
                    .font_size(13.0)
                    .color(theme.text_subtle),
            );
        });
        return;
    };
    ui.column(|ui| {
        ui.width_full()
            .height_full()
            .padding_edges(Edges {
                left: 20.0,
                right: 20.0,
                top: 12.0,
                bottom: 10.0,
            })
            .gap(10.0);
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(8.0);
            if components::icon_button(ui, &theme, app.icons.ui("back"), 34.0).clicked() {
                app.actions.push(Action::SetNav(crate::app::Nav::Shares));
            }
            ui.column(|ui| {
                ui.gap(1.0);
                ui.add(
                    Label::new(browser.root_name.clone())
                        .font_size(16.0)
                        .weight(FontWeight::SEMIBOLD)
                        .color(theme.text)
                        .wrap(TextWrap::None),
                );
                ui.add(
                    Label::new(format!(
                        "分享 ID {} · 浏览路径 {}",
                        browser.share_id, browser.cwd
                    ))
                    .font_size(10.0)
                    .color(theme.text_subtle),
                );
            });
            ui.spacer();
            if components::icon_button(ui, &theme, app.icons.ui("arrow-up"), 34.0).clicked() {
                app.actions.push(Action::ShareBrowserUp);
            }
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("save")),
                "保存选中到我的网盘",
                ActionStyle::Primary,
                !app.selected_share_entries().is_empty(),
            )
            .clicked()
            {
                app.actions
                    .push(Action::TransferShare(app.selected_share_entries()));
            }
        });
        ui.row(|ui| {
            ui.width_full()
                .height(34.0)
                .padding_xy(10.0, 0.0)
                .radius(8.0)
                .background(theme.surface)
                .align_items(Align::Center)
                .gap(6.0);
            ui.add(Image::from_shared(app.icons.ui("folder")).size(14.0, 14.0));
            ui.add(
                Label::new(browser.cwd.clone())
                    .font_size(11.5)
                    .color(theme.text_muted),
            );
            ui.spacer();
            ui.add(
                Label::new("双击文件夹进入 · 单击选中")
                    .font_size(10.0)
                    .color(theme.text_subtle),
            );
        });
        if app.share_browse_state == LoadState::Loading {
            ui.row(|ui| {
                ui.padding_xy(10.0, 4.0);
                components::spinner_text(ui, &theme, "正在加载分享目录", app.now);
            });
        }
        if let Some(error) = app.share_browse_error.clone() {
            ui.row(|ui| {
                ui.width_full()
                    .padding(10.0)
                    .radius(8.0)
                    .background(theme.danger.fade(0.08));
                ui.add(
                    Label::new(error)
                        .font_size(11.5)
                        .color(theme.danger),
                );
            });
        }
        let entries = app.sorted_entries(&browser.entries);
        let selected = app.selected.clone();
        let icons = &app.icons;
        let now = app.now;
        let mut next_selected = selected.clone();
        let mut actions: Vec<Action> = Vec::new();
        let mut last_click = app.last_click;
        ui.scroll(
            ScrollView::vertical().grow(1.0).smooth(true),
            |ui| {
                ui.width_full().gap(2.0);
                if entries.is_empty() {
                    components::empty_state(
                        ui,
                        &theme,
                        app.icons.ui("folder"),
                        "这个分享目录是空的",
                        "返回上一级或保存整个分享到我的网盘",
                    );
                    return;
                }
                for entry in &entries {
                    let is_selected = selected.contains(&entry.fs_id);
                    let next_selected = &mut next_selected;
                    let actions_ref = &mut actions;
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
                                })
                                .clip();
                            ui.add(
                                Image::from_shared(icons.file_icon(entry.kind()))
                                    .size(28.0, 28.0),
                            );
                            ui.column(|ui| {
                                ui.grow(1.0).basis(Length::Px(0.0)).min_width(0.0);
                                ui.add(
                                    Label::new(entry.server_filename.clone())
                                        .font_size(12.5)
                                        .color(theme.text)
                                        .wrap(TextWrap::None),
                                );
                            });
                            ui.add(
                                Label::new(if entry.is_dir() {
                                    "文件夹".to_owned()
                                } else {
                                    human_size(entry.size)
                                })
                                .font_size(10.5)
                                .color(theme.text_subtle)
                                .width(Length::Px(110.0))
                                .align(TextAlign::End),
                            );
                            if components::action_button(
                                ui,
                                &theme,
                                None,
                                "保存",
                                ActionStyle::Ghost,
                                true,
                            )
                            .clicked()
                            {
                                actions_ref.push(Action::TransferShare(vec![
                                    entry.clone()
                                ]));
                            }
                    });
                    if response.clicked() {
                        let double = last_click
                            .map(|(id, time)| id == entry.fs_id && now - time < 0.4)
                            .unwrap_or(false);
                        if double {
                            actions.push(Action::ShareBrowserOpen(entry.clone()));
                            last_click = None;
                        } else {
                            if !next_selected.remove(&entry.fs_id) {
                                next_selected.insert(entry.fs_id);
                            }
                            last_click = Some((entry.fs_id, now));
                        }
                    }
                }
            },
        );
        app.selected = next_selected;
        app.last_click = last_click;
        app.actions.extend(actions);
    });
}

impl BaiduApp {
    pub fn selected_share_entries(&self) -> Vec<Entry> {
        let Some(browser) = &self.share_browser else {
            return Vec::new();
        };
        let selected: Vec<Entry> = browser
            .entries
            .iter()
            .filter(|entry| self.selected.contains(&entry.fs_id))
            .cloned()
            .collect();
        if selected.is_empty() {
            self.active
                .and_then(|active| {
                    browser
                        .entries
                        .iter()
                        .find(|entry| entry.fs_id == active)
                        .cloned()
                })
                .into_iter()
                .collect()
        } else {
            selected
        }
    }
}
