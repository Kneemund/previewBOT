use std::env;

use event_handler::Handler;
use lazy_static::lazy_static;
use serenity::{prelude::GatewayIntents, Client};

pub(crate) mod commands;
pub(crate) mod event_handler;
pub(crate) mod file_preview;

lazy_static! {
    pub(crate) static ref HTTP_CLIENT: reqwest::Client = reqwest::ClientBuilder::new()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/117.0.0.0 Safari/537.36")
        .build()
        .unwrap();
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to load .env file.");

    let token = env::var("BOT_TOKEN").expect("Expected bot token in .env file.");
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Error while creating the client.");

    if let Err(error) = client.start().await {
        println!("Error while starting the client: {:?}", error);
    }
}
