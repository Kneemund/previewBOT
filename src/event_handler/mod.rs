use std::time::Instant;

use serenity::async_trait;
use serenity::model::application::component::ComponentType;
use serenity::model::application::interaction::Interaction;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;

use self::file_preview::check_file_preview;
use self::file_preview::handle_delete_file_preview_button;

mod file_preview;

pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, mut msg: Message) {
        if msg.author.bot {
            return;
        }

        let time = Instant::now();

        check_file_preview(&ctx, &mut msg)
            .await
            .unwrap_or_else(|error| {
                println!("Error while checking file preview: {:?}", error);
            });

        println!("Time: {:?}", time.elapsed().as_millis());
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Some(component_interaction) = interaction.message_component() {
            if component_interaction.data.component_type == ComponentType::Button {
                if component_interaction
                    .data
                    .custom_id
                    .starts_with("deleteFilePreview")
                {
                    handle_delete_file_preview_button(&ctx, component_interaction)
                        .await
                        .unwrap_or_else(|error| {
                            println!(
                                "Error while handling delete file preview button: {:?}",
                                error
                            );
                        });
                }
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}
