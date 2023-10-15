use std::time::Instant;

use serenity::all::Command;
use serenity::all::ComponentInteractionDataKind;
use serenity::all::Interaction;
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;

use crate::commands;
use crate::file_preview::check_file_preview;
use crate::file_preview::handle_delete_file_preview_button;

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

        println!("Time: {:?}", time.elapsed());

        // let message_id = MessageId::from_str("1162145068455563264").unwrap();
        // let is_vertical = false;
        // let mut encoded_data = String::new();
        // general_purpose::URL_SAFE_NO_PAD
        //     .encode_string(message_id.get().to_le_bytes(), &mut encoded_data);
        // general_purpose::URL_SAFE_NO_PAD
        //     .encode_string(message_id.get().to_le_bytes(), &mut encoded_data);
        // general_purpose::URL_SAFE_NO_PAD
        //     .encode_string(message_id.get().to_le_bytes(), &mut encoded_data);
        // general_purpose::URL_SAFE_NO_PAD.encode_string(&[is_vertical as u8], &mut encoded_data);
        // general_purpose::URL_SAFE_NO_PAD
        //     .encode_string("wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww", &mut encoded_data);
        // general_purpose::URL_SAFE_NO_PAD
        //     .encode_string("wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww", &mut encoded_data);

        // println!("Encoded data: {}", encoded_data);
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Component(component_interaction) => {
                match component_interaction.data.kind {
                    ComponentInteractionDataKind::Button => {
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
                    _ => {}
                }
            }
            Interaction::Command(command_interaction) => {
                match command_interaction.data.name.as_str() {
                    "juxtapose" => {
                        commands::juxtapose::run(&ctx, &command_interaction)
                            .await
                            .unwrap();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        Command::set_global_commands(ctx, vec![commands::juxtapose::register()])
            .await
            .expect("Failed to register global commands.");
    }
}
