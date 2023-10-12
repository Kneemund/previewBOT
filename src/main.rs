use std::env;

use event_handler::Handler;
use serenity::{prelude::GatewayIntents, Client};

mod event_handler;

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
