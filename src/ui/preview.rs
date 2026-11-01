//! Built-in preview: text, Markdown, images and PDF pages.

use kasumi::prelude::*;
use kasumi::Image;
use kasumi_core::{Color, Length, TextWrap};

use crate::app::{Action, BaiduApp};
use crate::events::PreviewContent;
use crate::models::{FileKind, human_size};
use crate::ui::components::{self, ActionStyle};

pub fn view(app: &mut BaiduApp, ui: &mut Ui) {
    let theme = app.theme.clone();
    let Some(preview) = app.preview.clone() else {
        ui.center(|ui| {
            ui.width_full().height_full();
            ui.add(
                Label::new("没有正在预览的文件")
                    .font_size(13.0)
                    .color(theme.text_subtle),
            );
        });
        return;
    };
    let entry = preview.entry.clone();
    let kind = entry.kind();
    let viewport = ui.viewport();
    let main_width = (viewport.width - 224.0).max(360.0);
    let text_width = (main_width - 84.0).max(200.0);
    let mut actions: Vec<Action> = Vec::new();
    ui.column(|ui| {
        ui.width(Length::Px(main_width))
            .height_full()
            .min_width(0.0)
            .min_height(0.0)
            .max_width(Length::Px(main_width))
            .clip()
            .background(theme.background);
        ui.row(|ui| {
            ui.width_full()
                .min_width(0.0)
                .flex_shrink(0.0)
                .clip()
                .height(58.0)
                .padding_xy(18.0, 0.0)
                .gap(10.0)
                .align_items(Align::Center)
                .background(theme.panel)
                .border(1.0, theme.border);
            if components::icon_button(ui, &theme, app.icons.ui("back"), 34.0).clicked() {
                actions.push(Action::ClosePreview);
            }
            ui.add(
                Image::from_shared(app.icons.file_icon(kind)).size(30.0, 30.0),
            );
            ui.column(|ui| {
                ui.grow(1.0)
                    .basis(Length::Px(0.0))
                    .min_width(0.0)
                    .gap(1.0)
                    .clip();
                ui.add(
                    Label::new(entry.server_filename.clone())
                        .font_size(14.0)
                        .weight(FontWeight::MEDIUM)
                        .color(theme.text)
                        .width(Length::Percent(100.0))
                        .wrap(TextWrap::None),
                );
                ui.add(
                    Label::new(format!(
                        "{} · {} · {}",
                        entry.path,
                        human_size(entry.size),
                        kind.label()
                    ))
                    .font_size(10.0)
                    .color(theme.text_subtle)
                    .width(Length::Percent(100.0))
                    .wrap(TextWrap::None),
                );
            });
            let textual = matches!(
                kind,
                FileKind::Text | FileKind::Markdown | FileKind::Code | FileKind::Other
            );
            if textual {
                let (total, offset, limit) = match &preview.content {
                    Some(PreviewContent::Text {
                        total,
                        offset,
                        limit,
                        ..
                    })
                    | Some(PreviewContent::Markdown {
                        total,
                        offset,
                        limit,
                        ..
                    }) => (*total, *offset, *limit),
                    _ => (preview.total, preview.text_offset, preview.text_limit),
                };
                let pages = if limit == 0 {
                    1
                } else {
                    ((total + limit - 1) / limit).max(1)
                };
                let page = if limit == 0 { 1 } else { offset / limit + 1 };
                if components::action_button(
                    ui,
                    &theme,
                    Some(app.icons.ui("arrow-left")),
                    "上一页",
                    ActionStyle::Ghost,
                    page > 1,
                )
                .clicked()
                {
                    actions.push(Action::PreviewPagePrev);
                }
                ui.add(
                    Label::new(format!("{page} / {pages}"))
                        .font_size(10.5)
                        .color(theme.text_subtle)
                        .wrap(TextWrap::None),
                );
                if components::action_button(
                    ui,
                    &theme,
                    Some(app.icons.ui("arrow-right")),
                    "下一页",
                    ActionStyle::Ghost,
                    page < pages,
                )
                .clicked()
                {
                    actions.push(Action::PreviewPageNext);
                }
                ui.row(|ui| {
                    ui.width(6.0);
                });
                if components::action_button(
                    ui,
                    &theme,
                    None,
                    "A−",
                    ActionStyle::Ghost,
                    true,
                )
                .clicked()
                {
                    actions.push(Action::PreviewFontSize(-1.0));
                }
                if components::action_button(
                    ui,
                    &theme,
                    None,
                    "A+",
                    ActionStyle::Ghost,
                    true,
                )
                .clicked()
                {
                    actions.push(Action::PreviewFontSize(1.0));
                }
            }
            if kind == FileKind::Image {
                if components::action_button(ui, &theme, None, "−", ActionStyle::Ghost, true).clicked() {
                    if let Some(preview) = &mut app.preview {
                        preview.zoom = (preview.zoom - 0.15).max(0.2);
                    }
                }
                if components::action_button(ui, &theme, None, "+", ActionStyle::Ghost, true).clicked() {
                    if let Some(preview) = &mut app.preview {
                        preview.zoom = (preview.zoom + 0.15).min(4.0);
                    }
                }
                if components::action_button(ui, &theme, None, "适应窗口", ActionStyle::Ghost, true).clicked() {
                    if let Some(preview) = &mut app.preview {
                        preview.zoom = 0.0;
                    }
                }
            }
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("download")),
                "下载",
                ActionStyle::Secondary,
                true,
            )
            .clicked()
            {
                actions.push(Action::Download(vec![entry.clone()]));
            }
            if components::action_button(
                ui,
                &theme,
                Some(app.icons.ui("share")),
                "分享",
                ActionStyle::Ghost,
                true,
            )
            .clicked()
            {
                actions.push(Action::ShareDialog(vec![entry.clone()]));
            }
            if components::icon_button(ui, &theme, app.icons.ui("close"), 34.0).clicked() {
                actions.push(Action::ClosePreview);
            }
        });
        ui.column(|ui| {
            ui.grow(1.0)
                .width_full()
                .min_height(0.0)
                .background(theme.background);
            if preview.loading {
                ui.center(|ui| {
                    ui.size_full().gap(10.0);
                    components::spinner_text(ui, &theme, "正在加载预览", app.now);
                    if kind == FileKind::Pdf {
                        ui.add(
                            Label::new("PDF 将在后台渲染第一页")
                                .font_size(10.5)
                                .color(theme.text_subtle),
                        );
                    }
                });
            } else if let Some(error) = preview.error.clone() {
                ui.center(|ui| {
                    ui.size_full().gap(10.0);
                    ui.add(Image::from_shared(app.icons.ui("warning")).size(46.0, 46.0));
                    ui.add(
                        Label::new(error)
                            .font_size(12.0)
                            .color(theme.danger)
                            .align(TextAlign::Center),
                    );
                    if components::action_button(
                        ui,
                        &theme,
                        Some(app.icons.ui("download")),
                        "改为下载",
                        ActionStyle::Secondary,
                        true,
                    )
                    .clicked()
                    {
                        actions.push(Action::Download(vec![entry.clone()]));
                    }
                });
            } else if let Some(content) = preview.content.clone() {
                match content {
                    PreviewContent::Text {
                        text,
                        total,
                        offset,
                        limit,
                        encoding,
                    } => {
                        text_content(
                            app,
                            ui,
                            &theme,
                            &text,
                            total,
                            offset,
                            limit,
                            &encoding,
                            preview.font_size,
                            kind == FileKind::Code,
                            text_width,
                        );
                    }
                    PreviewContent::Markdown {
                        text,
                        total,
                        offset,
                        limit,
                    } => {
                        markdown_content(
                            ui,
                            &theme,
                            &text,
                            total,
                            offset,
                            limit,
                            preview.font_size,
                            text_width,
                        );
                    }
                    PreviewContent::Image { image, width, height } => {
                        image_content(app, ui, &theme, image, width, height, preview.zoom);
                    }
                    PreviewContent::Pdf { page, pages, image } => {
                        pdf_content(app, ui, &theme, image, page, pages, preview.zoom, &mut actions);
                    }
                    PreviewContent::Unsupported { message } => {
                        unsupported(app, ui, &theme, &entry, &message, &mut actions);
                    }
                }
            }
        });
    });
    app.actions.extend(actions);
}

fn text_content(
    app: &BaiduApp,
    ui: &mut Ui,
    theme: &Theme,
    text: &str,
    total: u64,
    offset: u64,
    limit: u64,
    encoding: &str,
    font_size: f32,
    code: bool,
    text_width: f32,
) {
    let pages = if limit == 0 {
        1
    } else {
        ((total + limit - 1) / limit).max(1)
    };
    let page = if limit == 0 { 1 } else { offset / limit + 1 };
    ui.row(|ui| {
        ui.width_full()
            .height(26.0)
            .padding_xy(20.0, 0.0)
            .background(theme.surface)
            .border(1.0, theme.border)
            .align_items(Align::Center);
        ui.add(
            Label::new(format!(
                "第 {page} / {pages} 页 · 每页 {} · 共 {} · {}",
                human_size(limit),
                human_size(total),
                encoding
            ))
            .font_size(10.0)
            .color(theme.text_subtle)
            .wrap(TextWrap::None),
        );
        ui.spacer();
    });
    ui.scroll(
        ScrollView::vertical().grow(1.0).smooth(true),
        |ui| {
            ui.width(Length::Px(text_width + 48.0))
                .min_width(0.0)
                .padding(24.0);
            ui.column(|ui| {
                ui.width(Length::Px(text_width))
                    .min_width(0.0)
                    .padding(18.0)
                    .radius(12.0)
                    .background(theme.panel)
                    .border(1.0, theme.border);
                let mut label = Label::new(text.to_owned())
                    .font_size(font_size)
                    .line_height(font_size * 1.7)
                    .color(theme.text)
                    .width(Length::Px(text_width - 36.0));
                if code {
                    label = label.family(FontFamily::Monospace);
                }
                ui.add(label);
            });
        },
    );
}

fn markdown_content(
    ui: &mut Ui,
    theme: &Theme,
    text: &str,
    total: u64,
    offset: u64,
    limit: u64,
    font_size: f32,
    text_width: f32,
) {
    let pages = if limit == 0 {
        1
    } else {
        ((total + limit - 1) / limit).max(1)
    };
    let page = if limit == 0 { 1 } else { offset / limit + 1 };
    ui.row(|ui| {
        ui.width_full()
            .height(26.0)
            .padding_xy(20.0, 0.0)
            .background(theme.surface)
            .border(1.0, theme.border)
            .align_items(Align::Center);
        ui.add(
            Label::new(format!(
                "Markdown · 第 {page} / {pages} 页 · 共 {}",
                human_size(total)
            ))
            .font_size(10.0)
            .color(theme.text_subtle)
            .wrap(TextWrap::None),
        );
    });
    ui.scroll(
        ScrollView::vertical().grow(1.0).smooth(true),
        |ui| {
            ui.width(Length::Px(text_width + 48.0))
                .min_width(0.0)
                .padding(24.0);
            ui.column(|ui| {
                ui.width(Length::Px(text_width))
                    .min_width(0.0)
                    .padding(22.0)
                    .radius(12.0)
                    .background(theme.panel)
                    .border(1.0, theme.border);
                ui.add(
                    Markdown::new(text.to_owned())
                        .font_size(font_size)
                        .width(Length::Px(text_width - 44.0)),
                );
            });
        },
    );
}

fn image_content(
    app: &BaiduApp,
    ui: &mut Ui,
    theme: &Theme,
    image: std::sync::Arc<kasumi_core::ImageData>,
    width: u32,
    height: u32,
    zoom: f32,
) {
    let viewport = ui.viewport();
    let max_width = (viewport.width - 120.0).max(120.0);
    let max_height = (viewport.height - 160.0).max(120.0);
    let fit = (max_width / width as f32)
        .min(max_height / height as f32)
        .min(1.0);
    let zoom = if zoom <= 0.0 {
        fit
    } else {
        zoom
    };
    let display_width = (width as f32 * zoom).max(24.0);
    let display_height = (height as f32 * zoom).max(24.0);
    ui.scroll(
        ScrollView::both().grow(1.0).smooth(true),
        |ui| {
            ui.size_full()
                .padding(24.0)
                .background(Color::hex("#eef1f6"));
            ui.center(|ui| {
                ui.width(Length::Percent(100.0))
                    .height(Length::Percent(100.0));
                ui.add(
                    Image::from_shared(image)
                        .size(display_width, display_height)
                        .fit(ImageFit::Contain)
                        .radius(6.0),
                );
            });
        },
    );
    ui.row(|ui| {
        ui.width_full()
            .height(26.0)
            .padding_xy(18.0, 0.0)
            .background(theme.panel)
            .align_items(Align::Center);
        ui.add(
            Label::new(format!(
                "{} × {} · 缩放 {:.0}%",
                width,
                height,
                zoom * 100.0
            ))
            .font_size(10.0)
            .color(theme.text_subtle),
        );
    });
    let _ = app;
}

fn pdf_content(
    app: &BaiduApp,
    ui: &mut Ui,
    theme: &Theme,
    image: std::sync::Arc<kasumi_core::ImageData>,
    page: usize,
    pages: usize,
    zoom: f32,
    actions: &mut Vec<Action>,
) {
    ui.row(|ui| {
        ui.width_full()
            .height(42.0)
            .padding_xy(18.0, 0.0)
            .gap(8.0)
            .align_items(Align::Center)
            .background(theme.panel)
            .border(1.0, theme.border);
        if components::action_button(
            ui,
            theme,
            Some(app.icons.ui("arrow-left")),
            "上一页",
            ActionStyle::Ghost,
            page > 0,
        )
        .clicked()
        {
            actions.push(Action::PdfPrev);
        }
        ui.add(
            Label::new(format!("第 {} / {} 页", page + 1, pages))
                .font_size(11.5)
                .color(theme.text_muted),
        );
        if components::action_button(
            ui,
            theme,
            Some(app.icons.ui("arrow-right")),
            "下一页",
            ActionStyle::Ghost,
            page + 1 < pages,
        )
        .clicked()
        {
            actions.push(Action::PdfNext);
        }
        ui.spacer();
        if components::action_button(ui, theme, None, "缩小", ActionStyle::Ghost, true).clicked() {
            actions.push(Action::PdfZoom(-0.2));
        }
        ui.add(
            Label::new(format!("{:.0}%", zoom * 100.0))
                .font_size(10.5)
                .color(theme.text_subtle),
        );
        if components::action_button(ui, theme, None, "放大", ActionStyle::Ghost, true).clicked() {
            actions.push(Action::PdfZoom(0.2));
        }
    });
    // The texture covers `PDF_BASE_WIDTH * render_zoom`; the viewer wants it
    // displayed at `PDF_BASE_WIDTH * zoom`. Deriving both dimensions from the
    // base width keeps the aspect ratio when the first page is downscaled to
    // fit before a rerender at the fitted zoom has completed.
    let width = (crate::app::PDF_BASE_WIDTH * zoom).max(120.0);
    let height = width * (image.height() as f32 / image.width() as f32);
    // The page keeps its rendered size so zooming actually enlarges it and
    // the scroll view can pan over the page. Sizing the content to the image
    // (instead of the viewport) is what gives the scroll view something to
    // scroll: with the previous `size_full` + `Contain` pair every page was
    // scaled down to fit and the zoom buttons only changed the texture.
    let viewport = ui.viewport();
    let available_width = (viewport.width - 224.0).max(320.0);
    let available_height = (viewport.height - 194.0).max(240.0);
    let content_width = (width + 48.0).max(available_width);
    let content_height = (height + 48.0).max(available_height);
    ui.scroll(
        ScrollView::both().grow(1.0).smooth(true),
        |ui| {
            ui.width(Length::Px(content_width))
                .height(Length::Px(content_height))
                .padding(24.0)
                .background(Color::hex("#e9edf3"));
            ui.center(|ui| {
                ui.size_full();
                ui.add(
                    Image::from_shared(image.clone())
                        .size(width, height)
                        .fit(ImageFit::Fill)
                        .radius(4.0),
                );
            });
        },
    );
}

fn unsupported(
    app: &BaiduApp,
    ui: &mut Ui,
    theme: &Theme,
    entry: &crate::models::Entry,
    message: &str,
    actions: &mut Vec<Action>,
) {
    ui.center(|ui| {
        ui.width_full().height_full().gap(10.0);
        ui.add(
            Image::from_shared(app.icons.file_icon(entry.kind())).size(84.0, 84.0),
        );
        ui.add(
            Label::new(message.to_owned())
                .font_size(12.5)
                .color(theme.text_muted)
                .align(TextAlign::Center)
                .width(Length::Px(460.0)),
        );
        ui.row(|ui| {
            ui.gap(8.0);
            if components::action_button(
                ui,
                theme,
                Some(app.icons.ui("download")),
                "下载文件",
                ActionStyle::Primary,
                true,
            )
            .clicked()
            {
                actions.push(Action::Download(vec![entry.clone()]));
            }
            if components::action_button(
                ui,
                theme,
                Some(app.icons.ui("info")),
                "详细信息",
                ActionStyle::Secondary,
                true,
            )
            .clicked()
            {
                actions.push(Action::Details(entry.clone()));
            }
        });
    });
}
