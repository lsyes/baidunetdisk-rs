//! Baidu Netdisk inspired palette for the kasumi theme.

use kasumi::Theme;
use kasumi_core::Color;

pub const BRAND_BLUE: &str = "#2f6bff";
pub const BRAND_CYAN: &str = "#16b9ff";
pub const SIDEBAR_BG: &str = "#f7f9fc";

pub fn baidu_theme() -> Theme {
    Theme {
        background: Color::hex("#f4f6fa"),
        panel: Color::hex("#ffffff"),
        surface: Color::hex("#f7f9fc"),
        surface_hover: Color::hex("#ecf1f8"),
        scrollbar_rail: Color::hex("#eef2f8"),
        scrollbar_thumb: Color::hex("#c5cddb"),
        scrollbar_thumb_hover: Color::hex("#9aa6b8"),
        border: Color::hex("#e3e8f0"),
        border_strong: Color::hex("#c9d2e0"),
        text: Color::hex("#172033"),
        text_muted: Color::hex("#5b6678"),
        text_subtle: Color::hex("#8c96a8"),
        accent: Color::hex(BRAND_BLUE),
        accent_hover: Color::hex("#4a80ff"),
        on_accent: Color::hex("#ffffff"),
        focus_ring: Color::hex(BRAND_BLUE),
        danger: Color::hex("#e5484d"),
        success: Color::hex("#1f9d82"),
        font_size: 14.0,
        font_size_small: 12.0,
        font_size_large: 20.0,
        radius_small: 8.0,
        radius_medium: 10.0,
        radius_large: 16.0,
        spacing: 12.0,
    }
}

pub const CARD_SHADOW: Color = Color {
    r: 0.09,
    g: 0.13,
    b: 0.2,
    a: 0.08,
};
