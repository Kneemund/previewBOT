use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::mem::size_of;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use base64::{engine::general_purpose, Engine};
use event_handler::Handler;
use once_cell::sync::Lazy;
use redis::AsyncCommands;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serenity::all::ChannelId;
use serenity::http::Http;
use serenity::prelude::TypeMapKey;
use serenity::{all::MessageId, prelude::GatewayIntents, Client};
use tower_http::cors::CorsLayer;

pub(crate) mod commands;
pub(crate) mod event_handler;
pub(crate) mod file_preview;

pub(crate) static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::ClientBuilder::new()
    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/117.0.0.0 Safari/537.36")
    .build()
    .expect("Failed to build HTTP client.")
});

pub(crate) static BLAKE3_JUXTAPOSE_KEY: Lazy<[u8; 32]> = Lazy::new(|| {
    blake3::derive_key(
        "utilBOT 2023-10-15 12:11:06 juxtapose MAC v1",
        env::var("BLAKE3_KEY_MATERIAL")
            .expect("BLAKE3_KEY_MATERIAL is missing.")
            .as_bytes(),
    )
});

#[derive(Debug, Serialize)]
struct APIJuxtaposeResponse {
    left_image_url: String,
    right_image_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    left_image_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    right_image_label: Option<String>,
}

impl APIJuxtaposeResponse {
    fn get_expire_unix_ts(&self) -> Result<usize, Box<dyn Error + Send + Sync>> {
        let left_ts = usize::from_str_radix(
            &Url::parse(self.left_image_url.as_str())?
                .query_pairs()
                .find(|(key, _)| key == "ex")
                .ok_or("Expire parameter of left URL not found.")?
                .1,
            16,
        )?;

        let right_ts = usize::from_str_radix(
            &Url::parse(self.left_image_url.as_str())?
                .query_pairs()
                .find(|(key, _)| key == "ex")
                .ok_or("Expire parameter of right URL not found.")?
                .1,
            16,
        )?;

        Ok(right_ts.min(left_ts))
    }

    fn get_cache_headers(expire_unix_ts: u64) -> HeaderMap {
        HeaderMap::from_iter([
            (
                axum::http::header::EXPIRES,
                httpdate::fmt_http_date(
                    SystemTime::UNIX_EPOCH + Duration::from_secs(expire_unix_ts),
                )
                .parse()
                .unwrap(),
            ),
            (
                axum::http::header::CACHE_CONTROL,
                HeaderValue::from_static("public, must-revalidate, immutable"),
            ),
        ])
    }

    async fn redis_cache_set(
        &self,
        connection: &mut redis::aio::ConnectionManager,
        key: &str,
    ) -> Result<usize, StatusCode> {
        let mut data = vec![
            ("left_image", self.left_image_url.as_str()),
            ("right_image", self.right_image_url.as_str()),
        ];

        if let Some(left_image_label) = &self.left_image_label {
            data.push(("left_label", left_image_label.as_str()));
        }

        if let Some(right_image_label) = &self.right_image_label {
            data.push(("right_label", right_image_label.as_str()));
        }

        connection
            .hset_multiple(key, &data)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let unix_ts = self.get_expire_unix_ts().map_err(|err| {
            println!("Error while getting expire timestamp: {:?}", err);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        connection.expire_at(key, unix_ts).await.map_err(|err| {
            println!("Error while setting expire timestamp: {:?}", err);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        // let expire_time = SystemTime::UNIX_EPOCH + Duration::from_secs(unix_ts as u64);
        // let expire_seconds = expire_time
        //     .duration_since(SystemTime::now())
        //     .map(|duration| duration.as_secs())
        //     .unwrap_or(0);

        // Ok(expire_seconds)

        Ok(unix_ts)
    }

    async fn redis_cache_get_data(
        connection: &mut redis::aio::ConnectionManager,
        key: &str,
    ) -> Option<Self> {
        connection
            .hgetall::<&str, HashMap<String, String>>(key)
            .await
            .ok()
            .and_then(|cached_urls| {
                match (
                    cached_urls.get("left_image"),
                    cached_urls.get("right_image"),
                ) {
                    (Some(left_image_url), Some(right_image_url)) => Some(APIJuxtaposeResponse {
                        left_image_url: left_image_url.to_owned(),
                        right_image_url: right_image_url.to_owned(),
                        left_image_label: cached_urls.get("left_label").cloned(),
                        right_image_label: cached_urls.get("right_label").cloned(),
                    }),
                    _ => None,
                }
            })
    }

    async fn redis_cache_get_expire(
        connection: &mut redis::aio::ConnectionManager,
        key: &str,
    ) -> Result<usize, StatusCode> {
        redis::cmd("EXPIRETIME")
            .arg(key)
            .query_async(connection)
            .await
            .map_err(|err| {
                println!("Error while getting expire timestamp: {:?}", err);
                StatusCode::INTERNAL_SERVER_ERROR
            })

        // connection.ttl(key).await.map_err(|err| {
        //     println!("Error while getting expire timestamp: {:?}", err);
        //     StatusCode::INTERNAL_SERVER_ERROR
        // })
    }
}

#[derive(Debug, Deserialize)]
struct APIJuxtaposeRequest {
    #[serde(rename = "d")]
    data: String,
    #[serde(rename = "m")]
    mac: String,
}

async fn juxtapose_url_handler(
    State(JuxtaposeEndpointState {
        serenity_http,
        mut redis_connection_manager,
    }): State<JuxtaposeEndpointState>,
    Query(params): Query<APIJuxtaposeRequest>,
) -> Result<(HeaderMap, impl IntoResponse), StatusCode> /* (StatusCode, &'static str) */ {
    let time = Instant::now();

    let data_bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(params.data.as_str())
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let mac_bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(params.mac)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let mac_bytes: &[u8; 16] = mac_bytes
        .as_slice()
        .try_into()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let mut mac_calculated = [0u8; 16];
    blake3::Hasher::new_keyed(&BLAKE3_JUXTAPOSE_KEY)
        .update(data_bytes.as_slice())
        .finalize_xof()
        .fill(&mut mac_calculated);

    if !constant_time_eq::constant_time_eq_16(mac_bytes, &mac_calculated) {
        return Err(StatusCode::BAD_REQUEST);
    }

    println!("MAC check took {:?}.", time.elapsed());

    // TODO: function for this
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

        println!("Redis took {:?}.", time.elapsed());

        Ok((
            APIJuxtaposeResponse::get_cache_headers(expire_unix_ts as u64),
            Json(response_data),
        ))
    } else {
        // TODO: no unwrap
        let mut data_ids = data_bytes
            .chunks_exact(size_of::<u64>())
            .map(|id| u64::from_le_bytes(id.try_into().unwrap()));

        let message_id = MessageId::from(data_ids.next().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?);
        let channel_id = ChannelId::from(data_ids.next().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?);

        let juxtapose_message = serenity_http
            .get_message(channel_id, message_id)
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;

        // TODO: verify message author

        let left_attachment = juxtapose_message
            .attachments
            .get(1)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

        let right_attachment = juxtapose_message
            .attachments
            .get(2)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

        let response_data = APIJuxtaposeResponse {
            left_image_url: left_attachment.url.to_owned(),
            right_image_url: right_attachment.url.to_owned(),
            left_image_label: left_attachment.description.to_owned(),
            right_image_label: right_attachment.description.to_owned(),
        };

        println!("Serenity took {:?}.", time.elapsed());

        let expire_unix_ts = response_data
            .redis_cache_set(&mut redis_connection_manager, params.data.as_str())
            .await?;

        Ok((
            APIJuxtaposeResponse::get_cache_headers(expire_unix_ts as u64),
            Json(response_data),
        ))
    }
}

struct SerenityRedisConnection;

impl TypeMapKey for SerenityRedisConnection {
    type Value = redis::aio::ConnectionManager;
}

#[derive(Clone)]
pub struct JuxtaposeEndpointState {
    redis_connection_manager: redis::aio::ConnectionManager,
    serenity_http: Arc<Http>,
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to load .env file.");

    let token = env::var("BOT_TOKEN").expect("Expected bot token in .env file.");
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let mut serenity_client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Error while creating the client.");

    let redis_client = redis::Client::open("redis://127.0.0.1/").unwrap();
    let redis_connection_manager = redis::aio::ConnectionManager::new(redis_client)
        .await
        .expect("Failed to connect to Redis.");

    {
        let mut data = serenity_client.data.write().await;
        data.insert::<SerenityRedisConnection>(redis_connection_manager.clone());
    }

    // #[cfg(debug_assertions)]
    let cors = CorsLayer::permissive();

    // #[cfg(not(debug_assertions))]
    // let cors = CorsLayer::new()
    //     .allow_methods([axum::http::Method::GET])
    //     .allow_origin(
    //         "https://juxtapose.kneemund.de"
    //             .parse::<HeaderValue>()
    //             .unwrap(),
    //     );

    let app = axum::Router::new().route(
        "/v1/url",
        axum::routing::get(juxtapose_url_handler)
            .with_state(JuxtaposeEndpointState {
                redis_connection_manager,
                serenity_http: serenity_client.http.clone(),
            })
            .layer(cors),
    );

    tokio::spawn(async move {
        println!("Running server on port 3000...");

        axum::Server::bind(&"127.0.0.1:3000".parse().unwrap())
            .serve(app.into_make_service())
            .await
            .unwrap();
    });

    if let Err(error) = serenity_client.start().await {
        println!("Error while starting the client: {:?}", error);
    }
}
