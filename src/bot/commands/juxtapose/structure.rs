use serenity::all::{CommandOptionType, CreateCommand, CreateCommandOption};

pub(crate) fn register() -> CreateCommand<'static> {
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
            .max_length(100)
            .required(false),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "right_label",
                "The label on the right side (or bottom).",
            )
            .max_length(100)
            .required(false),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::Boolean,
                "vertical",
                "Whether or not the juxtapose should be vertical instead of horizontal. Defaults to false.",
            )
            .required(false),
        )
}
