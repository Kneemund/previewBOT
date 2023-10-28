use std::ops::Range;

use image::{DynamicImage, GenericImage, Rgba};
use imageproc::{
    definitions::HasWhite,
    drawing::{draw_filled_rect_mut, draw_text_mut, text_size, Blend, Canvas},
    rect::Rect,
};
use once_cell::sync::Lazy;
use rusttype::{Font, Scale};

static LABEL_FONT: Lazy<Font> = Lazy::new(|| {
    let font_data = include_bytes!("../../../../assets/font/RobotoSlab-Regular.ttf");
    Font::try_from_bytes(font_data as &[u8]).unwrap()
});

pub(super) fn draw_vertical_line_mut(image: &mut DynamicImage, line: Range<u32>, color: Rgba<u8>) {
    for y in 0..image.height() {
        for x in line.clone() {
            image.put_pixel(x, y, color);
        }
    }
}

pub(super) fn draw_horizontal_line_mut(
    image: &mut DynamicImage,
    line: Range<u32>,
    color: Rgba<u8>,
) {
    for x in 0..image.width() {
        for y in line.clone() {
            image.put_pixel(x, y, color);
        }
    }
}

pub(super) enum LabelPosition {
    TopLeft,
    BottomLeft,
    BottomRight,
}

pub(super) fn draw_label(
    canvas: &mut Blend<DynamicImage>,
    position: LabelPosition,
    scale: Scale,
    text: &str,
    margin: i32,
) {
    let (label_width, label_height) = text_size(scale, &LABEL_FONT, text);

    let background_position = match position {
        LabelPosition::TopLeft => Rect::at(0, 0),
        LabelPosition::BottomLeft => {
            Rect::at(0, canvas.height() as i32 - label_height - 2 * margin)
        }
        LabelPosition::BottomRight => Rect::at(
            canvas.width() as i32 - label_width - 2 * margin,
            canvas.height() as i32 - label_height - 2 * margin,
        ),
    };

    let background_rect = background_position.of_size(
        (label_width + 2 * margin) as u32,
        (label_height + 2 * margin) as u32,
    );

    draw_filled_rect_mut(canvas, background_rect, Rgba([0, 0, 0, 128]));

    draw_text_mut(
        &mut canvas.0,
        Rgba::white(),
        background_rect.left() + margin,
        background_rect.top() + margin,
        scale,
        &LABEL_FONT,
        text,
    )
}
