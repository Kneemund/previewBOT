use std::env;
use std::error::Error;
use std::io::{Cursor, Write};
use std::ops::{Deref, Range};

use base64::engine::general_purpose;
use base64::Engine;
use image::io::Limits;
use image::{DynamicImage, GenericImage, GenericImageView, ImageFormat, ImageOutputFormat, Rgba};
use imageproc::definitions::HasWhite;
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut, text_size, Blend};
use imageproc::rect::Rect;
use once_cell::sync::Lazy;
use rusttype::{Font, Scale};
use serenity::all::{Attachment, CommandInteraction};
use serenity::builder::{
    CreateActionRow, CreateAttachment, CreateButton, CreateCommand, CreateCommandOption,
    EditInteractionResponse,
};
use serenity::model::application::{CommandOptionType, ResolvedOption, ResolvedValue};
use serenity::prelude::Context;
use tokio::try_join;

static IMAGE_LIMITS: Lazy<Limits> = Lazy::new(|| {
    let mut image_limits = Limits::default();
    image_limits.max_image_width = Some(4096);
    image_limits.max_image_height = Some(4096);
    image_limits.max_alloc = Some(32 * 1024 * 1024);

    image_limits
});

static LABEL_FONT: Lazy<Font> = Lazy::new(|| {
    let font_data = include_bytes!("../../assets/font/RobotoCondensed-Regular.ttf");
    Font::try_from_bytes(font_data as &[u8]).unwrap()
});

static BLAKE3_JUXTAPOSE_KEY: Lazy<[u8; 32]> = Lazy::new(|| {
    blake3::derive_key(
        "utilBOT 2023-10-15 12:11:06 juxtapose MAC v1",
        env::var("BLAKE3_KEY_MATERIAL")
            .expect("BLAKE3_KEY_MATERIAL is missing.")
            .as_bytes(),
    )
});

async fn get_image_from_attachment(
    attachment: &Attachment,
) -> Result<(Blend<DynamicImage>, CreateAttachment), Box<dyn Error + Send + Sync>> {
    let image_mime = attachment
        .content_type
        .clone()
        .ok_or("Failed to retrieve MIME type of image.")?;

    let image_format = ImageFormat::from_mime_type(image_mime)
        .ok_or("Failed to retrieve image format from MIME type of image.")?;

    let image_bytes = attachment.download().await?;

    let mut image_reader = image::io::Reader::new(Cursor::new(&image_bytes));
    image_reader.set_format(image_format);
    image_reader.limits(IMAGE_LIMITS.to_owned());

    let image = image_reader
        .decode()
        .map_err(|error| format!("Failed to decode image: {}", error))?;

    Ok((
        Blend(image),
        CreateAttachment::bytes(image_bytes, attachment.filename.as_str()),
    ))
}

pub fn draw_vertical_line_mut(image: &mut DynamicImage, line: Range<u32>, color: Rgba<u8>) {
    for y in 0..image.height() {
        for x in line.clone() {
            image.put_pixel(x, y, color);
        }
    }
}

pub async fn run(
    ctx: &Context,
    interaction: &CommandInteraction,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // TODO: check to_owned() - maybe better solution that doesn't require cloning

    let left_image_attachment = interaction
        .data
        .options()
        .get(0)
        .and_then(|option| match option {
            ResolvedOption {
                value: ResolvedValue::Attachment(attachment),
                ..
            } => Some(attachment.to_owned()),
            _ => None,
        })
        .unwrap();

    let right_image_attachment = interaction
        .data
        .options()
        .get(1)
        .and_then(|option| match option {
            ResolvedOption {
                value: ResolvedValue::Attachment(attachment),
                ..
            } => Some(attachment.to_owned()),
            _ => None,
        })
        .unwrap();

    let left_label = interaction
        .data
        .options()
        .get(2)
        .and_then(|option| match option {
            ResolvedOption {
                value: ResolvedValue::String(string),
                ..
            } => Some(string.to_owned()),
            _ => None,
        });

    let right_label = interaction
        .data
        .options()
        .get(3)
        .and_then(|option| match option {
            ResolvedOption {
                value: ResolvedValue::String(string),
                ..
            } => Some(string.to_owned()),
            _ => None,
        });

    let is_vertical = interaction
        .data
        .options()
        .get(4)
        .and_then(|option| match option {
            ResolvedOption {
                value: ResolvedValue::Boolean(boolean),
                ..
            } => Some(boolean.to_owned()),
            _ => None,
        })
        .unwrap_or(false);

    /* Limit Image Size and Dimensions */

    if left_image_attachment.size > 16 * 1024 * 1024
        || right_image_attachment.size > 16 * 1024 * 1024
    {
        return Err("The images must not be bigger than 16 MB.".into());
    }

    let left_image_width = left_image_attachment
        .width
        .ok_or("Failed to retrieve width of left image.")?;
    let left_image_height = left_image_attachment
        .height
        .ok_or("Failed to retrieve height of left image.")?;
    let right_image_width = right_image_attachment
        .width
        .ok_or("Failed to retrieve width of right image.")?;
    let right_image_height = right_image_attachment
        .height
        .ok_or("Failed to retrieve height of right image.")?;

    if IMAGE_LIMITS
        .check_dimensions(left_image_width, left_image_height)
        .is_err()
        || IMAGE_LIMITS
            .check_dimensions(right_image_width, right_image_height)
            .is_err()
    {
        return Err("The images must not be bigger than 4096x4096 pixels.".into());
    }

    /* Defer Interaction */

    interaction.defer(ctx).await?;

    /* Download and Process Images */

    let (
        (mut left_image, left_image_create_attachment),
        (mut right_image, right_image_create_attachment),
    ) = try_join!(
        get_image_from_attachment(left_image_attachment),
        get_image_from_attachment(right_image_attachment)
    )?;

    let time = std::time::Instant::now();

    let right_image_min_dimension = right_image_width.min(right_image_height);

    let label_scale = Scale {
        x: (right_image_min_dimension as f32) / 24.0,
        y: (right_image_min_dimension as f32) / 24.0,
    };

    let label_margin = (right_image_min_dimension as i32) / 64;

    if let Some(left_label) = left_label {
        let (label_width, label_height) = text_size(label_scale, &LABEL_FONT, left_label);

        draw_filled_rect_mut(
            &mut left_image,
            Rect::at(
                0,
                (right_image_height as i32) - (label_height + 2 * label_margin),
            )
            .of_size(
                (label_width + 2 * label_margin) as u32,
                (label_height + 2 * label_margin) as u32,
            ),
            Rgba([0, 0, 0, 128]),
        );

        draw_text_mut(
            &mut left_image.0,
            Rgba::white(),
            label_margin,
            (right_image_height as i32) - (label_height + label_margin),
            label_scale,
            &LABEL_FONT,
            left_label,
        )
    }

    if let Some(right_label) = right_label {
        let (label_width, label_height) = text_size(label_scale, &LABEL_FONT, right_label);

        draw_filled_rect_mut(
            &mut right_image,
            Rect::at(
                (right_image_width as i32) - (label_width + 2 * label_margin),
                (right_image_height as i32) - (label_height + 2 * label_margin),
            )
            .of_size(
                (label_width + 2 * label_margin) as u32,
                (label_height + 2 * label_margin) as u32,
            ),
            Rgba([0, 0, 0, 128]),
        );

        draw_text_mut(
            &mut right_image.0,
            Rgba::white(),
            (right_image_width as i32) - (label_width + label_margin),
            (right_image_height as i32) - (label_height + label_margin),
            label_scale,
            &LABEL_FONT,
            right_label,
        )
    }

    let left_image_view = left_image
        .0
        .view(0, 0, left_image_width / 2, left_image_height);
    right_image.0.copy_from(left_image_view.deref(), 0, 0)?;

    let vertical_line_center = right_image_width / 2;
    let vertical_line_extent = (right_image_width / 1000).max(1);
    draw_vertical_line_mut(
        &mut right_image.0,
        (vertical_line_center - vertical_line_extent)
            ..(vertical_line_center + vertical_line_extent),
        Rgba::white(),
    );

    let mut final_image_encoded = Vec::new();
    right_image
        .0
        .write_to(
            &mut Cursor::new(&mut final_image_encoded),
            ImageOutputFormat::Png,
        )
        .map_err(|error| format!("Failed to encode image: {}", error))?;

    println!("Image processing took {:?}.", time.elapsed());

    /* Reply */

    let reply = interaction
        .edit_response(
            ctx,
            EditInteractionResponse::new()
                .new_attachment(CreateAttachment::bytes(final_image_encoded, "preview.png"))
                .new_attachment(left_image_create_attachment)
                .new_attachment(right_image_create_attachment),
        )
        .await?;

    /* Encode Data */

    let mut data: Vec<u8> = Vec::new();
    data.write_all(reply.id.get().to_le_bytes().as_slice())?;
    data.write_all(reply.channel_id.get().to_le_bytes().as_slice())?;

    if let Some(guild_id) = reply.guild_id {
        data.write_all(guild_id.get().to_le_bytes().as_slice())?;
    }

    let mut mac = [0u8; 16];

    blake3::Hasher::new_keyed(&BLAKE3_JUXTAPOSE_KEY)
        .update(data.as_slice())
        .finalize_xof()
        .fill(&mut mac);

    // TOOO: compare using
    // constant_time_eq::constant_time_eq_16(a, b)

    let mut juxtapose_url_data = String::new();
    general_purpose::URL_SAFE_NO_PAD.encode_string(data.as_slice(), &mut juxtapose_url_data);

    let mut juxtapose_url_mac = String::new();
    general_purpose::URL_SAFE_NO_PAD.encode_string(mac.as_slice(), &mut juxtapose_url_mac);

    let juxtapose_url = reqwest::Url::parse_with_params(
        "https://juxtapose.kneemund.de/v1",
        &[
            ("d", juxtapose_url_data),
            ("m", juxtapose_url_mac),
            (
                "o",
                if is_vertical {
                    String::from("v")
                } else {
                    String::from("h")
                },
            ),
        ],
    )
    .unwrap();

    interaction
        .edit_response(
            ctx,
            EditInteractionResponse::new().components(vec![CreateActionRow::Buttons(vec![
                CreateButton::new_link(juxtapose_url)
                    .emoji('🔗')
                    .label("Open"),
            ])]),
        )
        .await?;

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("juxtapose")
        .description("Create a juxtapose by uploading two images.")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::Attachment,
                "left_image",
                "The image on the left side (or top).",
            )
            .required(true),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::Attachment,
                "right_image",
                "The image on the right side (or bottom).",
            )
            .required(true),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "left_label",
                "The label on the left side (or top).",
            )
            .required(false),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "right_label",
                "The label on the right side (or bottom).",
            )
            .required(false),
        )        .add_option(
            CreateCommandOption::new(
                CommandOptionType::Boolean,
                "vertical",
                "Whether or not the juxtapose should be vertical instead of horizontal. Defaults to false.",
            )
            .required(false),
        )
}
