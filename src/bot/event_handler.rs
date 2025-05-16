use std::env;

use serenity::all::{
    Colour, Command, ComponentInteractionDataKind, CreateEmbed, EditInteractionResponse, FullEvent,
    Interaction,
};
use serenity::async_trait;
use serenity::prelude::*;

pub struct Handler;

use super::commands::*;
use super::file_preview::check_file_preview;
use super::file_preview::handle_delete_file_preview_button;

#[async_trait]
impl EventHandler for Handler {
    async fn dispatch(&self, ctx: &Context, event: &FullEvent) {
        match event {
            FullEvent::Message { new_message, .. } => {
                if new_message.author.bot() {
                    return;
                }

                if let Err(error) = check_file_preview(ctx, new_message).await {
                    println!("Error while checking file preview: {:?}", error);
                }
            }
            FullEvent::InteractionCreate { interaction, .. } => match interaction {
                Interaction::Component(component_interaction) => {
                    #[allow(clippy::single_match)]
                    match component_interaction.data.kind {
                        ComponentInteractionDataKind::Button => {
                            if component_interaction
                                .data
                                .custom_id
                                .starts_with("deleteFilePreview")
                            {
                                if let Err(error) =
                                    handle_delete_file_preview_button(ctx, component_interaction)
                                        .await
                                {
                                    println!(
                                        "Error while handling delete file preview button: {:?}",
                                        error
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Interaction::Command(command_interaction) => {
                    #[allow(clippy::single_match)]
                    match command_interaction.data.name.as_str() {
                        "juxtapose" => {
                            if let Err(error) = juxtapose::run(ctx, command_interaction).await {
                                let _ = command_interaction
                                    .edit_response(
                                        &ctx.http,
                                        EditInteractionResponse::new().add_embed(
                                            CreateEmbed::new()
                                                .title("Error")
                                                .colour(Colour::RED)
                                                .description(error),
                                        ),
                                    )
                                    .await;
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            FullEvent::Ready { data_about_bot, .. } => {
                println!("{} is connected!", data_about_bot.user.name);

                let reload_commands = env::args().any(|argument| argument == "--reload-commands");

                if reload_commands {
                    println!("Reloading commands...");

                    Command::set_global_commands(&ctx.http, &[juxtapose::register()])
                        .await
                        .expect("Failed to register global commands.");
                }
            }
            _ => {}
        }
    }
}
