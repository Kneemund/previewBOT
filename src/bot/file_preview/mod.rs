use std::error::Error;
use std::fmt::Write;

use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Url;
use serenity::all::{ButtonStyle, ComponentInteraction};
use serenity::builder::{
    CreateActionRow, CreateAllowedMentions, CreateAttachment, CreateButton, CreateMessage,
    EditAttachments, EditMessage,
};
use serenity::futures::future::join_all;
use serenity::model::channel::Message;
use serenity::model::prelude::MessageReference;
use serenity::prelude::*;
use serenity::utils::MessageBuilder;

use crate::HTTP_CLIENT;

use self::gist::GistFilePreview;
use self::github_repositoriy_file::GitHubRepositoryFilePreview;

mod gist;
mod github_repositoriy_file;

static GITHUB_REPOSITORY_FILE_URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"https://github\.com(?:/[^/\s]+){2}/(?:blob|blame)(?:/[^/\s]+)+#[^/\s]+").unwrap()
});

static GIST_URL_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"https://gist\.github\.com(?:/[^/\s]+){2}#file\-[^\s]+").unwrap());

static GITHUB_LINE_NUMBER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"L(\d+)").unwrap());

trait FilePreview: Sync + Send {
    fn get_message_url(&self) -> &Url;
    fn get_metadata_content(&self) -> &str;
    fn get_file_extension(&self) -> Option<&str>;
    fn get_raw_content(&self) -> &str;
}

impl dyn FilePreview {
    fn get_file_extension_with_alias(&self) -> Option<&str> {
        self.get_file_extension().map(|extension| match extension {
            "vsh" => "glsl",
            "fsh" => "glsl",
            "gsh" => "glsl",
            "csh" => "glsl",
            "vert" => "glsl",
            "frag" => "glsl",
            "inc" => "glsl",
            _ => extension,
        })
    }
}

#[derive(Debug)]
enum PreviewUrlType {
    GitHubRepositoryFile,
    Gist,
}

#[derive(Debug)]
struct PreviewUrlMatch<'a> {
    url_string: &'a str,
    url_type: PreviewUrlType,
    position: usize,
}

impl PreviewUrlMatch<'_> {
    fn get_url(&self) -> Result<Url, Box<dyn Error + Send + Sync>> {
        Url::parse(self.url_string).map_err(|_| "The specified URL is malformed.".into())
    }

    async fn get_file_preview(self) -> Result<Box<dyn FilePreview>, Box<dyn Error + Send + Sync>> {
        match self.url_type {
            PreviewUrlType::GitHubRepositoryFile => Ok(Box::new(
                GitHubRepositoryFilePreview::new(self.get_url()?).await?,
            )),
            PreviewUrlType::Gist => Ok(Box::new(GistFilePreview::new(self.get_url()?).await?)),
        }
    }
}

async fn fetch_raw_content(url: Url) -> Result<String, Box<dyn Error + Send + Sync>> {
    let response = HTTP_CLIENT.get(url).send().await?;

    if !response.status().is_success() {
        return Err("API request failed.".into());
    }

    if response
        .content_length()
        .is_some_and(|file_size| file_size > 4_194_304)
    {
        return Err("File size is too large.".into());
    }

    Ok(response.text().await?)
}

fn truncate_string(string: &str, max_length: usize) -> String {
    if string.len() > max_length {
        let (truncated_string, _) = string.split_at(max_length - 3);
        format!("{}...", truncated_string)
    } else {
        string.to_owned()
    }
}

async fn send_file_preview(
    ctx: &Context,
    msg: &Message,
    file_preview: Box<dyn FilePreview>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let line_numbers: Vec<u32> = GITHUB_LINE_NUMBER_REGEX
        .captures_iter(
            file_preview
                .get_message_url()
                .fragment()
                .ok_or("The specified URL is malformed.")?,
        )
        .filter_map(|match_captures| match_captures[1].parse::<u32>().ok())
        .collect();

    let top_line_number = *line_numbers
        .iter()
        .min()
        .ok_or("At least one line number is required.")?;

    let bottom_line_number = *line_numbers
        .iter()
        .max()
        .ok_or("At least one line number is required.")?;

    let selected_content_lines: Vec<String> = file_preview
        .get_raw_content()
        .lines()
        .skip(top_line_number as usize - 1)
        .take((bottom_line_number - top_line_number + 1) as usize)
        .map(|line| line.to_owned())
        .collect();

    if selected_content_lines.is_empty() {
        return Err("No content selected.".into());
    }

    let line_number_length = (top_line_number as usize + selected_content_lines.len() - 1)
        .to_string()
        .len()
        .max(1);

    let file_content_capacity = selected_content_lines
        .iter()
        .map(|line| line.len())
        .sum::<usize>()
        + selected_content_lines.len() * (line_number_length + 4);

    let file_content = selected_content_lines.iter().enumerate().fold(
        String::with_capacity(file_content_capacity),
        |mut output, (index, line)| {
            let _ = writeln!(
                output,
                "{:width$} | {}",
                top_line_number + index as u32,
                line,
                width = line_number_length
            );

            output
        },
    );

    let open_button = CreateButton::new_link(file_preview.get_message_url().as_str())
        .emoji('🔗')
        .label("Open")
        .to_owned();

    let delete_button = CreateButton::new(format!("deleteFilePreview:{}", msg.author.id))
        .style(ButtonStyle::Secondary)
        .emoji('🗑')
        .to_owned();

    if file_content.len() + file_preview.get_metadata_content().len() > 1900
        || selected_content_lines.len() > 6
    {
        let mut reply = msg
            .channel_id
            .send_message(
                &ctx.http,
                CreateMessage::new()
                    .content(file_preview.get_metadata_content())
                    .add_file(CreateAttachment::bytes(
                        file_content.as_bytes(),
                        format!(
                            "preview.{}",
                            file_preview
                                .get_file_extension_with_alias()
                                .unwrap_or("txt")
                        )
                        .as_str(),
                    ))
                    .reference_message(msg)
                    .allowed_mentions(CreateAllowedMentions::new().replied_user(false))
                    .components(vec![CreateActionRow::Buttons(vec![
                        open_button,
                        delete_button,
                    ])]),
            )
            .await?;

        if reply
            .attachments
            .first()
            .map(|a| a.content_type.is_none())
            .unwrap_or(false)
        {
            reply
                .edit(
                    &ctx,
                    EditMessage::new().attachments(EditAttachments::new().add(
                        CreateAttachment::bytes(file_content.as_bytes(), "preview.txt"),
                    )),
                )
                .await?;
        }
    } else {
        msg.channel_id
            .send_message(
                &ctx.http,
                CreateMessage::new()
                    .content(
                        MessageBuilder::new()
                            .push(file_preview.get_metadata_content())
                            .push_codeblock_safe(
                                file_content,
                                file_preview.get_file_extension_with_alias(),
                            )
                            .build(),
                    )
                    .reference_message(MessageReference::from(msg))
                    .allowed_mentions(CreateAllowedMentions::new().replied_user(false))
                    .components(vec![CreateActionRow::Buttons(vec![
                        open_button,
                        delete_button,
                    ])]),
            )
            .await?;
    }

    Ok(())
}

pub async fn check_file_preview(
    ctx: &Context,
    msg: &mut Message,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut url_matches: Vec<PreviewUrlMatch> = GITHUB_REPOSITORY_FILE_URL_REGEX
        .find_iter(&msg.content)
        .map(|url_match| PreviewUrlMatch {
            url_string: url_match.as_str(),
            url_type: PreviewUrlType::GitHubRepositoryFile,
            position: url_match.start(),
        })
        .chain(
            GIST_URL_REGEX
                .find_iter(&msg.content)
                .map(|url_match| PreviewUrlMatch {
                    url_string: url_match.as_str(),
                    url_type: PreviewUrlType::Gist,
                    position: url_match.start(),
                }),
        )
        .collect();

    if url_matches.is_empty() {
        return Ok(());
    }

    url_matches.sort_unstable_by_key(|element| element.position);

    let file_previews = join_all(
        url_matches
            .into_iter()
            .take(3)
            .map(|element| element.get_file_preview())
            .collect::<Vec<_>>(),
    )
    .await;

    for file_preview in file_previews {
        send_file_preview(ctx, msg, file_preview?).await?;
    }

    Ok(())
}

pub async fn handle_delete_file_preview_button(
    ctx: &Context,
    interaction: ComponentInteraction,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let author_id = interaction
        .data
        .custom_id
        .split_once(':')
        .ok_or("Failed to retrieve author ID from custom ID.")?
        .1;

    interaction.defer(ctx).await?;

    if author_id != interaction.user.id.to_string() {
        return Ok(());
    }

    interaction.delete_response(ctx).await?;
    Ok(())
}
