//! Pure-Rust PDF rendering through hayro.

use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use kasumi_core::ImageData;

pub fn page_count(bytes: &Arc<Vec<u8>>) -> Result<usize, String> {
    let pdf = Pdf::new(bytes.clone()).map_err(|error| format!("无法解析 PDF：{error:?}"))?;
    let count = pdf.pages().len();
    if count == 0 {
        return Err("PDF 没有页面".to_owned());
    }
    Ok(count)
}

pub fn render_page(
    bytes: &Arc<Vec<u8>>,
    page_index: usize,
    target_width: u32,
) -> Result<(Arc<ImageData>, usize), String> {
    let pdf = Pdf::new(bytes.clone()).map_err(|error| format!("无法解析 PDF：{error:?}"))?;
    let pages = pdf.pages();
    let count = pages.len();
    let page = pages
        .get(page_index)
        .ok_or_else(|| format!("页码越界：{page_index} / {count}"))?;
    let (width, height) = page.render_dimensions();
    if width <= 0.0 || height <= 0.0 {
        return Err("PDF 页面尺寸无效".to_owned());
    }
    let scale = (target_width.max(200) as f32 / width).clamp(0.25, 4.0);
    let cache = RenderCache::new();
    let settings = InterpreterSettings::default();
    let pixmap = render(
        page,
        &cache,
        &settings,
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );
    let png = pixmap
        .into_png()
        .map_err(|error| format!("PDF 页面编码失败：{error}"))?;
    let decoded = image::load_from_memory(&png)
        .map_err(|error| format!("PDF 页面解码失败：{error}"))?
        .to_rgba8();
    let (pixel_width, pixel_height) = decoded.dimensions();
    if pixel_width == 0 || pixel_height == 0 {
        return Err("PDF 渲染结果为空".to_owned());
    }
    Ok((
        Arc::new(ImageData::from_rgba8(
            pixel_width,
            pixel_height,
            decoded.into_raw(),
        )),
        count,
    ))
}
