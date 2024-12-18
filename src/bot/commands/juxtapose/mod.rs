use std::env;
use std::io::Cursor;
use std::ops::Deref;

use base64::engine::general_purpose;
use base64::Engine;
use image::Limits;
use image::{DynamicImage, GenericImage, GenericImageView, ImageFormat, Rgba};
use imageproc::definitions::HasWhite;
use imageproc::drawing::Blend;
use once_cell::sync::Lazy;
use serenity::all::{
    Attachment, CommandInteraction, CreateActionRow, CreateAttachment, CreateButton,
    EditAttachments, EditInteractionResponse, ResolvedOption, ResolvedValue,
};
use serenity::prelude::*;
use tokio::try_join;

use crate::bot::commands::juxtapose::preview::{
    draw_horizontal_line_mut, draw_label, draw_vertical_line_mut, LabelPosition,
};
use crate::web::api_juxtapose_response::APIJuxtaposeResponse;
use crate::{SerenityGlobalData, BLAKE3_JUXTAPOSE_KEY, HTTP_CLIENT};

mod preview;
mod structure;
pub(crate) use structure::register;

static IMAGE_LIMITS: Lazy<Limits> = Lazy::new(|| {
    let mut image_limits = Limits::default();
    image_limits.max_image_width = Some(4096);
    image_limits.max_image_height = Some(4096);
    image_limits.max_alloc = Some(32 * 1024 * 1024);

    image_limits
});

static JUXTAPOSE_BASE_URL: Lazy<reqwest::Url> = Lazy::new(|| {
    reqwest::Url::parse(
        env::var("JUXTAPOSE_BASE_URL")
            .as_deref()
            .unwrap_or("http://localhost"),
    )
    .expect("Failed to parse JUXTAPOSE_BASE_URL.")
});

async fn get_image_from_attachment(
    attachment: &Attachment,
    image_width: u32,
    image_height: u32,
) -> Result<(Blend<DynamicImage>, CreateAttachment), String> {
    let image_mime = attachment
        .content_type
        .clone()
        .ok_or("Failed to retrieve MIME type of image.")?;

    let image_format = ImageFormat::from_mime_type(image_mime)
        .ok_or("Failed to retrieve image format from MIME type of image.")?;

    if !image_format.can_read() {
        return Err("The image format is not supported.".to_owned());
    }

    let image_url = reqwest::Url::parse_with_params(
        attachment.proxy_url.as_str(),
        &[
            ("width", image_width.to_string()),
            ("height", image_height.to_string()),
        ],
    )
    .map_err(|_| "Failed to parse attachment URL.")?;

    let image_bytes = HTTP_CLIENT
        .get(image_url)
        .send()
        .await
        .map_err(|_| "Failed to fetch image from CDN.")?
        .bytes()
        .await
        .map_err(|_| "Failed to receive image data from CDN.")?;

    let mut image_reader = image::ImageReader::new(Cursor::new(&image_bytes));
    image_reader.set_format(image_format);
    image_reader.limits(IMAGE_LIMITS.to_owned());

    let image = image_reader
        .decode()
        .map_err(|error| format!("Failed to decode image: {}", error))?;

    Ok((
        Blend(image),
        CreateAttachment::bytes(image_bytes.to_vec(), attachment.filename.to_owned()),
    ))
}

pub async fn run(ctx: &Context, interaction: &CommandInteraction) -> Result<(), String> {
    let left_image_attachment = interaction
        .data
        .options()
        .first()
        .and_then(|option| match option {
            ResolvedOption {
                value: ResolvedValue::Attachment(attachment),
                ..
            } => Some(*attachment),
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
            } => Some(*attachment),
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
            } => Some((*string).to_owned()),
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
            } => Some((*string).to_owned()),
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
            } => Some(*boolean),
            _ => None,
        })
        .unwrap_or(false);

    /* Defer Interaction */

    if let Err(error) = interaction.defer(&ctx.http).await {
        println!("Failed to defer juxtapose interaction: {:?}", error);
        return Ok(());
    }

    /* Limit Image Size and Dimensions */

    if left_image_attachment.size > 16 * 1024 * 1024
        || right_image_attachment.size > 16 * 1024 * 1024
    {
        return Err("The images must not be bigger than 16 MB.".to_owned());
    }

    let left_image_width = left_image_attachment
        .width
        .ok_or("The left (top) attachment is not a supported image.")?;
    let left_image_height = left_image_attachment
        .height
        .ok_or("The left (top) attachment is not a supported image.")?;
    let right_image_width = right_image_attachment
        .width
        .ok_or("The right (bottom) attachment is not a supported image.")?;
    let right_image_height = right_image_attachment
        .height
        .ok_or("The right (bottom) attachment is not a supported image.")?;

    let mut preview_image_width = left_image_width.min(right_image_width).get();
    let mut preview_image_height = left_image_height.min(right_image_height).get();

    const PREVIEW_IMAGE_MAX_SIZE: u32 = 4096;
    let preview_image_max_dimension = preview_image_width.max(preview_image_height);

    if preview_image_max_dimension > PREVIEW_IMAGE_MAX_SIZE {
        let scale = PREVIEW_IMAGE_MAX_SIZE as f32 / preview_image_max_dimension as f32;

        preview_image_width = (preview_image_width as f32 * scale) as u32;
        preview_image_height = (preview_image_height as f32 * scale) as u32;
    }

    /* Download and Process Images */

    let (
        (mut left_image, mut left_image_create_attachment),
        (mut right_image, mut right_image_create_attachment),
    ) = try_join!(
        get_image_from_attachment(
            left_image_attachment,
            preview_image_width,
            preview_image_height
        ),
        get_image_from_attachment(
            right_image_attachment,
            preview_image_width,
            preview_image_height
        )
    )?;

    let preview_image_min_dimension = preview_image_width.min(preview_image_height);

    let label_scale = (preview_image_min_dimension as f32) / 24.0;
    let label_margin = (preview_image_min_dimension as i32) / 64;

    if let Some(ref left_label) = left_label {
        left_image_create_attachment = left_image_create_attachment.description(left_label);

        draw_label(
            &mut left_image,
            if is_vertical {
                LabelPosition::TopLeft
            } else {
                LabelPosition::BottomLeft
            },
            label_scale,
            left_label,
            label_margin,
        );
    }

    if let Some(ref right_label) = right_label {
        right_image_create_attachment = right_image_create_attachment.description(right_label);

        draw_label(
            &mut right_image,
            if is_vertical {
                LabelPosition::BottomLeft
            } else {
                LabelPosition::BottomRight
            },
            label_scale,
            right_label,
            label_margin,
        );
    }

    let left_image_view = if is_vertical {
        left_image
            .0
            .view(0, 0, preview_image_width, preview_image_height / 2)
    } else {
        left_image
            .0
            .view(0, 0, preview_image_width / 2, preview_image_height)
    };

    right_image
        .0
        .copy_from(left_image_view.deref(), 0, 0)
        .map_err(|_| "Failed to overlay left (top) image onto right (bottom) image.")?;

    if is_vertical {
        let horizontal_line_center = preview_image_height / 2;
        let horizontal_line_extent = (preview_image_height / 1000).max(1);
        draw_horizontal_line_mut(
            &mut right_image.0,
            (horizontal_line_center - horizontal_line_extent)
                ..(horizontal_line_center + horizontal_line_extent),
            Rgba::white(),
        );
    } else {
        let vertical_line_center = preview_image_width / 2;
        let vertical_line_extent = (preview_image_width / 1000).max(1);
        draw_vertical_line_mut(
            &mut right_image.0,
            (vertical_line_center - vertical_line_extent)
                ..(vertical_line_center + vertical_line_extent),
            Rgba::white(),
        );
    }

    let mut final_image_encoded = Vec::new();
    right_image
        .0
        .write_to(&mut Cursor::new(&mut final_image_encoded), ImageFormat::Png)
        .map_err(|error| format!("Failed to encode image: {}", error))?;

    /* Reply */

    let reply = interaction
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new().attachments(
                EditAttachments::new()
                    .add(CreateAttachment::bytes(final_image_encoded, "preview.png"))
                    .add(left_image_create_attachment)
                    .add(right_image_create_attachment),
            ),
        )
        .await
        .map_err(|_| "Failed to upload images to Discord. Perhaps they are too large?")?;

    /* Encode Data */

    let data = [
        reply.id.get().to_le_bytes(),
        interaction.channel_id.get().to_le_bytes(),
    ]
    .concat();

    let mut mac = [0u8; 16];

    blake3::Hasher::new_keyed(&BLAKE3_JUXTAPOSE_KEY)
        .update(data.as_slice())
        .finalize_xof()
        .fill(&mut mac);

    let juxtapose_url_data = general_purpose::URL_SAFE_NO_PAD.encode(data.as_slice());
    let juxtapose_url_mac = general_purpose::URL_SAFE_NO_PAD.encode(mac.as_slice());

    let mut juxtapose_url = JUXTAPOSE_BASE_URL.clone();
    juxtapose_url.query_pairs_mut().extend_pairs(&[
        ("d", juxtapose_url_data.as_str()),
        ("m", juxtapose_url_mac.as_str()),
        ("o", if is_vertical { "v" } else { "h" }),
    ]);

    interaction
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new().components(&[CreateActionRow::buttons(&[
                CreateButton::new_link(juxtapose_url.as_str())
                    .emoji('🔗')
                    .label("Open"),
            ])]),
        )
        .await
        .map_err(|_| "Failed to add button containing the juxtapose URL.")?;

    let mut redis_connection_manager = ctx
        .data::<SerenityGlobalData>()
        .redis_connection_manager
        .clone();

    let juxtapose_cache_data = APIJuxtaposeResponse {
        left_image_url: left_image_attachment.url.to_string(),
        right_image_url: right_image_attachment.url.to_string(),
        left_image_label: left_label,
        right_image_label: right_label,
    };

    juxtapose_cache_data
        .redis_cache_set(&mut redis_connection_manager, juxtapose_url_data.as_str())
        .await
        .unwrap();

    Ok(())
}
