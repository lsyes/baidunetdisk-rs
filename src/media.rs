//! Image decoding and PDF rendering helpers shared by previews and auth.

use std::sync::Arc;

use kasumi_core::ImageData;

pub fn decode_image(bytes: &[u8], max_edge: u32) -> Option<Arc<ImageData>> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let mut rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    if width.max(height) > max_edge {
        let scale = max_edge as f32 / width.max(height) as f32;
        let new_width = ((width as f32 * scale).round() as u32).max(1);
        let new_height = ((height as f32 * scale).round() as u32).max(1);
        rgba = image::imageops::resize(
            &rgba,
            new_width,
            new_height,
            image::imageops::FilterType::Lanczos3,
        );
    }
    let (width, height) = rgba.dimensions();
    Some(Arc::new(ImageData::from_rgba8(
        width,
        height,
        rgba.into_raw(),
    )))
}

pub fn decode_image_exact(bytes: &[u8]) -> Option<Arc<ImageData>> {
    let decoded = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = decoded.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    Some(Arc::new(ImageData::from_rgba8(
        width,
        height,
        decoded.into_raw(),
    )))
}

pub fn pdf_page_count(bytes: &Arc<Vec<u8>>) -> Result<usize, String> {
    crate::pdf::page_count(bytes)
}

pub fn render_pdf(
    bytes: &Arc<Vec<u8>>,
    page: usize,
    width: u32,
) -> Result<(Arc<ImageData>, usize), String> {
    crate::pdf::render_page(bytes, page, width)
}
