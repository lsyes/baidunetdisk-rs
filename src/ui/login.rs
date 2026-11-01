//! Login screen: QR code, account password and cookie import.

use std::sync::Arc;

use kasumi::prelude::*;
use kasumi::Image;
use kasumi_core::{Color, ImageData, Length, TextWrap};

use crate::app::{Action, BaiduApp, LoginTab};
use crate::events::QrState;
use crate::ui::components::{self, ActionStyle};

pub fn view(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    ui.row(|ui| {
        ui.size_full();
        brand_panel(app, ui);
        ui.column(|ui| {
            ui.grow(1.0)
                .height_full()
                .padding(32.0)
                .background(theme.background);
            ui.center(|ui| {
                ui.size_full();
                login_card(app, ui);
            });
        });
    });
}

fn brand_panel(app: &mut BaiduApp, ui: &mut Ui) {
    ui.column(|ui| {
        ui.width(Length::Percent(44.0))
            .height_full()
            .padding(44.0)
            .gap(18.0)
            .background(Color::hex("#2f6bff"));
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center).gap(12.0);
            ui.center(|ui| {
                ui.width(44.0)
                    .height(44.0)
                    .radius(12.0)
                    .background(Color::WHITE.fade(0.16));
                ui.add(Image::from_shared(app.icons.logo()).size(34.0, 34.0));
            });
            ui.column(|ui| {
                ui.gap(1.0);
                ui.add(
                    Label::new("百度网盘")
                        .font_size(18.0)
                        .weight(FontWeight::SEMIBOLD)
                        .color(Color::WHITE),
                );
                ui.add(
                    Label::new("Baidu Netdisk · Rust")
                        .font_size(10.5)
                        .color(Color::WHITE.fade(0.7)),
                );
            });
        });
        ui.spacer();
        ui.add(
            Label::new("你的文件，\n随身而行。")
                .font_size(36.0)
                .weight(FontWeight::SEMIBOLD)
                .line_height(48.0)
                .color(Color::WHITE),
        );
        ui.add(
            Label::new(
                "纯 Rust 编写的第三方百度网盘桌面客户端。\n多线程下载、分享、预览，全部在一处完成。",
            )
            .font_size(13.5)
            .line_height(22.0)
            .color(Color::WHITE.fade(0.82)),
        );
        ui.row(|ui| {
            ui.height(14.0);
        });
        for (icon, title, detail) in [
            ("download", "多线程下载器", "分段并发、断点续传、暂停恢复"),
            ("open-file", "内置阅读器", "PDF / Markdown / TXT / 图片"),
            ("share", "分享管理", "创建、提取码、打开分享链接"),
        ] {
            ui.row(|ui| {
                ui.width_full()
                    .padding(12.0)
                    .gap(12.0)
                    .align_items(Align::Center)
                    .radius(12.0)
                    .background(Color::WHITE.fade(0.10));
                ui.center(|ui| {
                    ui.width(34.0)
                        .height(34.0)
                        .radius(9.0)
                        .background(Color::WHITE.fade(0.16));
                    ui.add(Image::from_shared(app.icons.ui(icon)).size(18.0, 18.0));
                });
                ui.column(|ui| {
                    ui.gap(2.0);
                    ui.add(
                        Label::new(title)
                            .font_size(12.5)
                            .weight(FontWeight::MEDIUM)
                            .color(Color::WHITE),
                    );
                    ui.add(
                        Label::new(detail)
                            .font_size(10.5)
                            .color(Color::WHITE.fade(0.68)),
                    );
                });
            });
        }
        ui.spacer();
        ui.add(
            Label::new("© 2026 · 非官方客户端 · 仅用于个人学习")
                .font_size(10.0)
                .color(Color::WHITE.fade(0.55)),
        );
    });
}

fn login_card(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    ui.column(|ui| {
        ui.width(Length::Px(430.0))
            .padding(26.0)
            .gap(16.0)
            .radius(18.0)
            .background(theme.panel)
            .border(1.0, theme.border);
        ui.column(|ui| {
            ui.gap(4.0);
            ui.add(
                Label::new(match app.login.tab {
                    LoginTab::Qr => "扫码登录",
                    LoginTab::Account => "账号登录",
                    LoginTab::Cookie => "Cookie 登录",
                })
                .font_size(22.0)
                .weight(FontWeight::SEMIBOLD)
                .color(theme.text),
            );
            ui.add(
                Label::new(match app.login.tab {
                    LoginTab::Qr => "打开百度网盘 App 扫一扫，安全快捷",
                    LoginTab::Account => "使用手机号 / 邮箱和密码登录",
                    LoginTab::Cookie => "从浏览器复制 pan.baidu.com 的 Cookie",
                })
                .font_size(12.0)
                .color(theme.text_subtle),
            );
        });
        ui.row(|ui| {
            ui.width_full().gap(6.0).padding(3.0).radius(9.0).background(theme.surface);
            tab(ui, app, "扫码", LoginTab::Qr);
            tab(ui, app, "账号", LoginTab::Account);
            tab(ui, app, "Cookie", LoginTab::Cookie);
        });
        let tab_key = match app.login.tab {
            LoginTab::Qr => "qr",
            LoginTab::Account => "account",
            LoginTab::Cookie => "cookie",
        };
        ui.keyed(("login-tab", tab_key), |ui| match app.login.tab {
            LoginTab::Qr => qr_tab(app, ui),
            LoginTab::Account => account_tab(app, ui),
            LoginTab::Cookie => cookie_tab(app, ui),
        });
        if let Some(error) = app.login.error.clone() {
            ui.row(|ui| {
                ui.width_full()
                    .padding(10.0)
                    .gap(8.0)
                    .radius(9.0)
                    .background(theme.danger.fade(0.08))
                    .align_items(Align::Start);
                ui.add(Image::from_shared(app.icons.ui("warning")).size(16.0, 16.0));
                ui.add(
                    Label::new(error)
                        .font_size(11.5)
                        .color(theme.danger)
                        .width(Length::Percent(92.0)),
                );
            });
        }
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center);
            ui.add(
                Label::new("登录信息只保存在本机 · 不会上传到第三方")
                    .font_size(10.0)
                    .color(theme.text_subtle),
            );
            ui.spacer();
            ui.add(
                Label::new("v0.1 · API 8.9.7")
                    .font_size(10.0)
                    .color(theme.text_subtle)
                    .family(FontFamily::Monospace),
            );
        });
    });
}

fn tab(ui: &mut Ui, app: &mut BaiduApp, label: &str, value: LoginTab) {
    let theme = app.theme.clone();
    let active = app.login.tab == value;
    let hovered = ui.is_hovered();
    let response = ui.row(|ui| {
        ui.clickable()
            .grow(1.0)
            .basis(Length::Px(0.0))
            .height(34.0)
            .radius(7.0)
            .justify(Justify::Center)
            .align_items(Align::Center)
            .background(if active {
                theme.panel
            } else if hovered {
                theme.surface_hover
            } else {
                Color::TRANSPARENT
            });
        if active {
            ui.add(
                Label::new(label)
                    .font_size(12.5)
                    .weight(FontWeight::MEDIUM)
                    .color(theme.accent),
            );
        } else {
            ui.add(
                Label::new(label)
                    .font_size(12.5)
                    .color(theme.text_muted),
            );
        }
    });
    if response.clicked() {
        app.actions.push(Action::ToggleLoginTab(value));
    }
}

fn qr_tab(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    ui.center(|ui| {
        ui.width_full().gap(12.0).padding_xy(0.0, 6.0);
        ui.center(|ui| {
            ui.width(220.0)
                .height(220.0)
                .radius(12.0)
                .background(Color::WHITE)
                .border(1.0, theme.border);
            if let Some(image) = app.login.qr_image.clone() {
                ui.add(
                    Image::from_shared(image)
                        .size(196.0, 196.0)
                        .fit(ImageFit::Contain),
                );
            } else {
                components::spinner_text(ui, &theme, "正在生成二维码", app.now);
            }
        });
        let (message, color) = match app.login.qr_state {
            QrState::Loading => ("正在连接 passport.baidu.com…".to_owned(), theme.text_subtle),
            QrState::Waiting => ("请使用百度网盘 App 扫描二维码".to_owned(), theme.text_muted),
            QrState::Scanned => ("已扫描，请在手机上确认登录".to_owned(), theme.accent),
            QrState::Confirmed => ("登录成功，正在获取账号信息…".to_owned(), theme.success),
            QrState::Expired => ("二维码已过期，请刷新".to_owned(), theme.danger),
            QrState::Failed => ("二维码获取失败".to_owned(), theme.danger),
        };
        if message.is_empty() {
            ui.add(
                Label::new(app.login.qr_message.clone())
                    .font_size(12.0)
                    .color(color)
                    .align(TextAlign::Center),
            );
        } else {
            ui.add(
                Label::new(message)
                    .font_size(12.0)
                    .color(color)
                    .align(TextAlign::Center),
            );
        }
        components::action_button(
            ui,
            &theme,
            Some(app.icons.ui("refresh")),
            "刷新二维码",
            ActionStyle::Secondary,
            !app.login.busy,
        )
        .clicked()
        .then(|| app.actions.push(Action::LoginQrRefresh));
    });
}

fn account_tab(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let inputs = app.inputs().clone();
    ui.column(|ui| {
        ui.width_full().gap(10.0);
        components::field_label(ui, &theme, "手机号 / 邮箱 / 用户名");
        ui.add(
            TextInput::new(&inputs.account)
                .placeholder("百度账号")
                .width(Length::Percent(100.0))
                .height(38.0),
        );
        components::field_label(ui, &theme, "密码");
        ui.add(
            TextInput::new(&inputs.password)
                .placeholder("登录密码")
                .width(Length::Percent(100.0))
                .height(38.0),
        );
        ui.add(
            Label::new("密码只在本机用于 Passport 加密登录，不会被保存。")
                .font_size(10.0)
                .color(theme.text_subtle),
        );
        if app.login.captcha.is_some() {
            ui.add(
                Label::new("需要验证码，请输入图中字符")
                    .font_size(11.0)
                    .color(theme.text_muted),
            );
            if let Some(captcha) = app.login.captcha.clone() {
                ui.add(
                    Image::from_shared(captcha)
                        .size(140.0, 48.0)
                        .fit(ImageFit::Contain),
                );
            }
            ui.add(
                TextInput::new(&inputs.vcode)
                    .placeholder("验证码")
                    .width(Length::Percent(100.0))
                    .height(38.0),
            );
        }
        let button = components::action_button(
            ui,
            &theme,
            None,
            if app.login.busy { "正在登录…" } else { "登录" },
            ActionStyle::Primary,
            !app.login.busy,
        );
        if button.clicked() {
            app.actions.push(Action::LoginAccount);
        }
        ui.add(
            Label::new("账号登录可能触发百度安全验证；如果反复失败，可改用扫码或 Cookie 登录。")
                .font_size(10.0)
                .color(theme.text_subtle)
                .line_height(15.0),
        );
    });
}

fn cookie_tab(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let inputs = app.inputs().clone();
    ui.column(|ui| {
        ui.width_full().gap(10.0);
        components::field_label(ui, &theme, "Cookie");
        ui.add(
            TextInput::new(&inputs.cookie)
                .placeholder("BDUSS=...; STOKEN=...")
                .width(Length::Percent(100.0))
                .height(38.0),
        );
        ui.add(
            Label::new("在浏览器打开 pan.baidu.com，按 F12 → Network → 复制请求头里的 Cookie。至少需要 BDUSS。")
                .font_size(10.5)
                .color(theme.text_subtle)
                .line_height(16.0),
        );
        let button = components::action_button(
            ui,
            &theme,
            None,
            if app.login.busy { "验证中…" } else { "使用 Cookie 登录" },
            ActionStyle::Primary,
            !app.login.busy,
        );
        if button.clicked() {
            app.actions.push(Action::LoginCookie);
        }
    });
}

pub fn _unused(_: Arc<ImageData>, _: TextWrap) {}
