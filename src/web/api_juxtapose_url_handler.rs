use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use base64::{engine::general_purpose, Engine};
use serenity::all::{ChannelId, MessageId};
use std::mem::size_of;

use crate::APIJuxtaposeUrlHandlerState;

use super::{
    api_juxtapose_request::APIJuxtaposeRequest, api_juxtapose_response::APIJuxtaposeResponse,
};

pub(crate) async fn handler(
    State(APIJuxtaposeUrlHandlerState {
        serenity_http,
        serenity_cache,
        mut redis_connection_manager,
    }): State<APIJuxtaposeUrlHandlerState>,
    Query(params): Query<APIJuxtaposeRequest>,
) -> Result<(HeaderMap, impl IntoResponse), StatusCode> /* (StatusCode, &'static str) */ {
    let data_bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(params.data.as_str())
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    if !params.is_decoded_data_valid(data_bytes.as_slice())? {
        return Err(StatusCode::BAD_REQUEST);
    }

    if let Some(response_data) = APIJuxtaposeResponse::redis_cache_get_data(
        &mut redis_connection_manager,
        params.data.as_str(),
    )
    .await
    {
        let expire_unix_ts = APIJuxtaposeResponse::redis_cache_get_expire(
            &mut redis_connection_manager,
            params.data.as_str(),
        )
        .await?;

        Ok((
            APIJuxtaposeResponse::get_cache_headers(expire_unix_ts as u64),
            Json(response_data),
        ))
    } else {
        let mut data_ids = data_bytes.chunks_exact(size_of::<u64>()).map(|id| {
            id.try_into()
                .map(u64::from_le_bytes)
                .map_err(|_| StatusCode::BAD_REQUEST)
        });

        let message_id =
            MessageId::from(data_ids.next().ok_or(StatusCode::INTERNAL_SERVER_ERROR)??);

        let channel_id =
            ChannelId::from(data_ids.next().ok_or(StatusCode::INTERNAL_SERVER_ERROR)??);

        let juxtapose_message = serenity_http
            .get_message(channel_id, message_id)
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;

        if !juxtapose_message.is_own(&serenity_cache) {
            return Err(StatusCode::BAD_REQUEST);
        }

        let left_attachment = juxtapose_message
            .attachments
            .get(1)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

        let right_attachment = juxtapose_message
            .attachments
            .get(2)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

        let response_data = APIJuxtaposeResponse {
            left_image_url: left_attachment.url.to_string(),
            right_image_url: right_attachment.url.to_string(),
            left_image_label: left_attachment
                .description
                .as_ref()
                .map(ToString::to_string),
            right_image_label: right_attachment
                .description
                .as_ref()
                .map(ToString::to_string),
        };

        let expire_unix_ts = response_data
            .redis_cache_set(&mut redis_connection_manager, params.data.as_str())
            .await?;

        Ok((
            APIJuxtaposeResponse::get_cache_headers(expire_unix_ts as u64),
            Json(response_data),
        ))
    }
}
