//! Shared visual building blocks. Everything here is composed from kasumi's
//! public `Ui` API plus the small `Ui::clickable` extension.

use std::sync::Arc;

use kasumi::prelude::*;
use kasumi::Image;
use kasumi_core::{Color, ImageData, Length, TextWrap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStyle {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

pub fn icon(ui: &mut Ui, data: Arc<ImageData>, size: f32) {
    ui.add(Image::from_shared(data).size(Length::Px(size), Length::Px(size)));
}

pub fn icon_button(
    ui: &mut Ui,
    theme: &Theme,
    data: Arc<ImageData>,
    size: f32,
) -> Response {
    let hovered = ui.is_hovered();
    let response = ui.row(|ui| {
        ui.clickable()
            .width(Length::Px(size))
            .height(Length::Px(size))
            .align_items(Align::Center)
            .justify(Justify::Center)
            .radius(size * 0.3)
            .background(if hovered {
                theme.surface_hover
            } else {
                Color::TRANSPARENT
            });
        ui.add(Image::from_shared(data).size(
            Length::Px(size * 0.58),
            Length::Px(size * 0.58),
        ));
    });
    if response.hovered() {
        ui.cursor(CursorIcon::Pointer);
    }
    response
}

pub fn action_button(
    ui: &mut Ui,
    theme: &Theme,
    icon_data: Option<Arc<ImageData>>,
    label: impl Into<String>,
    style: ActionStyle,
    enabled: bool,
) -> Response {
    let label = label.into();
    let hovered = ui.is_hovered() && enabled;
    let (background, foreground, border) = match style {
        ActionStyle::Primary => (
            if hovered {
                theme.accent_hover
            } else {
                theme.accent
            },
            theme.on_accent,
            0.0,
        ),
        ActionStyle::Secondary => (
            if hovered {
                theme.surface_hover
            } else {
                theme.panel
            },
            theme.text,
            1.0,
        ),
        ActionStyle::Ghost => (
            if hovered {
                theme.surface_hover
            } else {
                Color::TRANSPARENT
            },
            theme.text_muted,
            0.0,
        ),
        ActionStyle::Danger => (
            if hovered {
                theme.danger.fade(0.12)
            } else {
                Color::TRANSPARENT
            },
            theme.danger,
            0.0,
        ),
    };
    let (background, foreground) = if enabled {
        (background, foreground)
    } else {
        (background.fade(0.5), foreground.fade(0.45))
    };
    let response = ui.row(|ui| {
        ui.clickable()
            .height(Length::Px(34.0))
            .padding_xy(12.0, 0.0)
            .gap(7.0)
            .align_items(Align::Center)
            .radius(9.0)
            .background(background)
            .border(border, theme.border);
        if let Some(data) = icon_data {
            ui.add(Image::from_shared(data).size(Length::Px(16.0), Length::Px(16.0)));
        }
        ui.add(
            Label::new(label)
                .font_size(13.0)
                .weight(if style == ActionStyle::Primary {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .color(foreground)
                .wrap(TextWrap::None),
        );
    });
    response
}

pub fn pill(ui: &mut Ui, text: impl Into<String>, background: Color, foreground: Color) {
    ui.row(|ui| {
        ui.padding_xy(7.0, 2.0)
            .radius(6.0)
            .background(background)
            .align_items(Align::Center);
        ui.add(
            Label::new(text)
                .font_size(10.5)
                .weight(FontWeight::MEDIUM)
                .color(foreground)
                .wrap(TextWrap::None),
        );
    });
}

pub fn nav_item(
    ui: &mut Ui,
    theme: &Theme,
    icon_data: Arc<ImageData>,
    label: &str,
    active: bool,
    badge: Option<u32>,
) -> Response {
    let hovered = ui.is_hovered();
    ui.row(|ui| {
        ui.clickable()
            .width_full()
            .height(Length::Px(42.0))
            .padding_xy(10.0, 0.0)
            .gap(10.0)
            .align_items(Align::Center)
            .radius(10.0)
            .background(if active {
                theme.accent.fade(0.10)
            } else if hovered {
                theme.surface_hover
            } else {
                Color::TRANSPARENT
            });
        if active {
            ui.row(|ui| {
                ui.width(3.0).height(18.0).radius(2.0).background(theme.accent);
            });
        } else {
            ui.row(|ui| {
                ui.width(3.0).height(18.0);
            });
        }
        ui.add(Image::from_shared(icon_data).size(Length::Px(18.0), Length::Px(18.0)));
        ui.add(
            Label::new(label)
                .font_size(13.5)
                .weight(if active {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .color(if active { theme.accent } else { theme.text_muted })
                .wrap(TextWrap::None),
        );
        ui.spacer();
        if let Some(badge) = badge
            && badge > 0
        {
            pill(
                ui,
                badge.to_string(),
                theme.danger,
                theme.on_accent,
            );
        }
    })
}

pub fn card(theme: &Theme, padding: f32) -> impl FnOnce(&mut Ui) {
    let theme = theme.clone();
    move |ui: &mut Ui| {
        ui.width_full()
            .padding(padding)
            .gap(12.0)
            .background(theme.panel)
            .radius(14.0)
            .border(1.0, theme.border);
    }
}

pub fn progress_bar(ui: &mut Ui, theme: &Theme, fraction: f32, height: f32, color: Color) {
    let fraction = fraction.clamp(0.0, 1.0);
    ui.row(|ui| {
        ui.width_full()
            .height(Length::Px(height))
            .radius(height * 0.5)
            .background(theme.surface_hover);
        ui.row(|ui| {
            ui.width(Length::Percent(fraction * 100.0))
                .height_full()
                .radius(height * 0.5)
                .background(color);
        });
    });
}

pub fn checkbox(
    ui: &mut Ui,
    theme: &Theme,
    checked: bool,
    label: &str,
    enabled: bool,
) -> bool {
    let hovered = ui.is_hovered() && enabled;
    let mut toggled = false;
    let response = ui.row(|ui| {
        ui.clickable()
            .padding_xy(2.0, 4.0)
            .gap(8.0)
            .align_items(Align::Center)
            .background(Color::TRANSPARENT);
        ui.center(|ui| {
            ui.width(Length::Px(18.0))
                .height(Length::Px(18.0))
                .radius(5.0)
                .background(if checked {
                    theme.accent
                } else if hovered {
                    theme.surface_hover
                } else {
                    theme.panel
                })
                .border(
                    1.0,
                    if checked {
                        theme.accent
                    } else {
                        theme.border_strong
                    },
                );
            if checked {
                ui.add(
                    Label::new("✓")
                        .font_size(12.0)
                        .weight(FontWeight::BOLD)
                        .color(theme.on_accent),
                );
            }
        });
        if !label.is_empty() {
            ui.add(
                Label::new(label)
                    .font_size(12.5)
                    .color(if enabled {
                        theme.text_muted
                    } else {
                        theme.text_subtle
                    })
                    .wrap(TextWrap::None),
            );
        }
    });
    if response.clicked() && enabled {
        toggled = true;
    }
    toggled
}

pub fn field_label(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.add(
        Label::new(text)
            .font_size(11.5)
            .weight(FontWeight::MEDIUM)
            .color(theme.text_muted),
    );
}

pub fn section_title(ui: &mut Ui, theme: &Theme, title: &str, subtitle: Option<&str>) {
    ui.column(|ui| {
        ui.gap(2.0);
        ui.add(
            Label::new(title)
                .font_size(15.0)
                .weight(FontWeight::SEMIBOLD)
                .color(theme.text)
                .wrap(TextWrap::None),
        );
        if let Some(subtitle) = subtitle {
            ui.add(
                Label::new(subtitle)
                    .font_size(11.5)
                    .color(theme.text_subtle)
                    .wrap(TextWrap::None),
            );
        }
    });
}

pub fn empty_state(
    ui: &mut Ui,
    theme: &Theme,
    icon_data: Arc<ImageData>,
    title: &str,
    subtitle: &str,
) {
    ui.center(|ui| {
        ui.width_full().height(Length::Px(280.0)).gap(10.0);
        ui.add(Image::from_shared(icon_data).size(Length::Px(64.0), Length::Px(64.0)));
        ui.add(
            Label::new(title)
                .font_size(15.0)
                .weight(FontWeight::MEDIUM)
                .color(theme.text_muted),
        );
        ui.add(
            Label::new(subtitle)
                .font_size(12.0)
                .color(theme.text_subtle)
                .align(TextAlign::Center),
        );
    });
}

pub fn spinner_text(ui: &mut Ui, theme: &Theme, text: &str, time: f64) {
    let dots = ["", ".", "..", "..."][((time * 2.0) as usize) % 4];
    ui.add(
        Label::new(format!("{text}{dots}"))
            .font_size(12.5)
            .color(theme.text_subtle),
    );
}

pub fn separator(ui: &mut Ui, theme: &Theme) {
    ui.row(|ui| {
        ui.width_full().height(1.0).background(theme.border);
    });
}

pub fn key_value(ui: &mut Ui, theme: &Theme, key: &str, value: &str) {
    ui.row(|ui| {
        ui.width_full().gap(12.0).align_items(Align::Start);
        ui.column(|ui| {
            ui.width(Length::Px(92.0));
            ui.add(
                Label::new(key)
                    .font_size(11.5)
                    .color(theme.text_subtle)
                    .wrap(TextWrap::None),
            );
        });
        ui.add(
            Label::new(value)
                .font_size(12.5)
                .color(theme.text)
                .wrap(TextWrap::Word),
        );
    });
}

pub fn toast(ui: &mut Ui, theme: &Theme, text: &str, error: bool, width: f32) {
    ui.row(|ui| {
        ui.width(Length::Px(width))
            .padding_xy(14.0, 11.0)
            .radius(10.0)
            .background(if error {
                theme.danger
            } else {
                Color::hex("#1d2739")
            })
            .border(1.0, if error {
                theme.danger
            } else {
                Color::hex("#32405a")
            });
        ui.add(
            Label::new(text)
                .font_size(12.5)
                .color(Color::WHITE)
                .wrap(TextWrap::Word),
        );
    });
}

pub fn dotted(ui: &mut Ui, theme: &Theme, text: &str, width: f32) {
    let _ = theme;
    ui.add(
        Label::new(text)
            .font_size(12.0)
            .color(theme.text_subtle)
            .width(Length::Px(width))
            .wrap(TextWrap::None),
    );
}
