use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::http::HeaderValue;
use bot::event_handler::Handler;
use once_cell::sync::Lazy;
use serenity::all::{Cache, Http};
use serenity::prelude::*;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use web::api_juxtapose_url_handler;

mod bot;
mod web;

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

struct SerenityGlobalData {
    redis_connection_manager: redis::aio::ConnectionManager,
}

#[derive(Clone)]
pub struct APIJuxtaposeUrlHandlerState {
    redis_connection_manager: redis::aio::ConnectionManager,
    serenity_cache: Arc<Cache>,
    serenity_http: Arc<Http>,
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to load .env file.");

    /* Redis */

    let redis_client = redis::Client::open(
        env::var("REDIS_URL")
            .as_deref()
            .unwrap_or("redis://127.0.0.1/"),
    )
    .unwrap();

    let redis_connection_manager = redis::aio::ConnectionManager::new(redis_client)
        .await
        .expect("Failed to connect to Redis.");

    /* Serenity */

    let token = env::var("BOT_TOKEN").expect("BOT_TOKEN is missing.");
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let mut serenity_client = Client::builder(&token, intents)
        .event_handler(Handler)
        .data(Arc::new(SerenityGlobalData {
            redis_connection_manager: redis_connection_manager.clone(),
        }) as _)
        .await
        .expect("Error while creating the client.");

    /* HTTP API */

    let cors = CorsLayer::new()
        .allow_methods([axum::http::Method::GET])
        .allow_origin(
            env::var("CORS_ORIGIN")
                .as_deref()
                .unwrap_or("*")
                .parse::<HeaderValue>()
                .unwrap(),
        );

    let app = axum::Router::new().route(
        "/url",
        axum::routing::get(api_juxtapose_url_handler::handler)
            .with_state(APIJuxtaposeUrlHandlerState {
                redis_connection_manager,
                serenity_cache: serenity_client.cache.clone(),
                serenity_http: serenity_client.http.clone(),
            })
            .layer(cors),
    );

    /* Start HTTP API */

    tokio::spawn(async move {
        let port: u16 = env::var("PORT")
            .expect("PORT is missing.")
            .parse()
            .expect("PORT is not a valid number.");

        println!("Running server on port {port}...");

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let listener = TcpListener::bind(&addr).await.unwrap();

        axum::serve(listener, app.into_make_service())
            .await
            .unwrap();
    });

    /* Start Serenity */

    if let Err(error) = serenity_client.start().await {
        println!("Error while starting the client: {:?}", error);
    }
}
