use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::path::PathBuf;

use lazy_static::lazy_static;
use regex::Regex;
use reqwest::header;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serenity::async_trait;
use serenity::builder::CreateActionRow;
use serenity::builder::CreateButton;
use serenity::model::application::component::ButtonStyle;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use serenity::utils::MessageBuilder;

#[derive(Debug, Deserialize, Serialize)]
struct Gist {
    url: String,
    forks_url: String,
    commits_url: String,
    id: String,
    node_id: String,
    git_pull_url: String,
    git_push_url: String,
    html_url: String,
    files: HashMap<String, File>,
    public: bool,
    created_at: String,
    updated_at: String,
    description: String,
    comments: u32,
    comments_url: String,
    truncated: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct File {
    filename: String,
    #[serde(rename = "type")]
    file_type: String,
    language: String,
    raw_url: String,
    size: u32,
    truncated: bool,
    content: String,
}

#[derive(Debug)]
struct FilePreviewData {
    raw_url: Url,
    api_url: String,
    metadata_content: String,
    file_type: FileType,
    position: u32,
}

trait FilePreview {
    fn get_raw_url(&self) -> &Url;
    fn get_file_extension(&self) -> Option<&String>;
}

#[derive(Debug, Deserialize, Serialize)]
struct APIGistMetadata {
    description: String,
    public: bool,
    created_at: String,
    files: Vec<String>,
    owner: String,
    // div: String,
    // stylesheet: String,
}

struct GistFilePreview {
    message_url: Url,
    raw_url: Url,
    metadata_content: String,
    file_extension: Option<String>,
}

impl GistFilePreview {
    /// Normalizes a file name by removing all non-alphanumeric characters and converting all characters to lowercase.
    /// This is needed because the file name in the URL fragment is heavily modified compared to the actual file name.
    fn normalize_file_name(string: &str) -> String {
        string
            .chars()
            .filter_map(|character| {
                if character.is_alphanumeric() {
                    Some(character.to_ascii_lowercase())
                } else {
                    None
                }
            })
            .collect()
    }

    async fn new(message_url: Url) -> Result<Self, Box<dyn Error>> {
        let selected_file_name_fragment = Self::normalize_file_name(
            &REGEX_GITHUB_GIST_FILE_NAME
                .captures(
                    message_url
                        .fragment()
                        .expect("The specified URL is malformed."),
                )
                .ok_or("File name not found.")?[1],
        );

        let mut metadata_url = message_url.clone();
        metadata_url.set_fragment(None);
        metadata_url.set_path((metadata_url.path().to_owned() + ".json").as_str());

        let response = reqwest::get(metadata_url).await?;

        if !response.status().is_success() {
            return Err("API request failed.".into());
        }

        let metadata: APIGistMetadata = response.json().await?;

        let selected_file_name = metadata
            .files
            .iter()
            .find(|file_name| Self::normalize_file_name(file_name) == selected_file_name_fragment)
            .ok_or("File not found.")?;

        let file_extension = PathBuf::from(selected_file_name)
            .extension()
            .and_then(|extension| Some(extension.to_string_lossy().into_owned()));

        let mut raw_url = message_url.clone();
        raw_url.set_fragment(None);
        raw_url
            .path_segments_mut()
            .unwrap()
            .push("raw")
            .push(&selected_file_name);

        let metadata_content = MessageBuilder::new()
            .push_bold_line_safe(metadata.owner)
            .push_italic_line_safe(metadata.description)
            .build();

        Ok(Self {
            message_url,
            raw_url,
            metadata_content,
            file_extension,
        })
    }
}

impl FilePreview for GistFilePreview {
    fn get_raw_url(&self) -> &Url {
        &self.raw_url
    }

    fn get_file_extension(&self) -> Option<&String> {
        self.file_extension.as_ref()
    }
}

struct GitHubRepositoryFilePreview {
    message_url: Url,
    raw_url: Url,
    metadata_content: String,
    file_extension: Option<String>,
}

impl GitHubRepositoryFilePreview {
    fn new(message_url: Url) -> Result<Self, Box<dyn Error>> {
        let path_segments: Vec<&str> = message_url.path_segments().unwrap().collect();

        let (author, repository, branch, path) = match path_segments.as_slice() {
            [author, repository, "blob", branch, path @ ..] => {
                (author, repository, branch, path.join("/"))
            }
            _ => return Err("Malformed GitHub repository URL.".into()),
        };

        let metadata_content = MessageBuilder::new()
            .push_bold_safe(author)
            .push("/")
            .push_bold_safe(repository)
            .push("(on ")
            .push_safe(branch)
            .push_line(")")
            .push_line_safe(path.as_str())
            .build();

        // TODO: construct using URL, error handling
        let raw_url = Url::parse(
            format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                author,
                repository,
                branch,
                path.as_str()
            )
            .as_str(),
        )
        .unwrap();

        let file_name = message_url
            .path_segments()
            .and_then(|segments| segments.last())
            .ok_or("File name not found.")?;

        let file_extension = PathBuf::from(file_name)
            .extension()
            .and_then(|extension| Some(extension.to_string_lossy().into_owned()));

        Ok(Self {
            message_url,
            raw_url,
            metadata_content,
            file_extension,
        })
    }
}

impl FilePreview for GitHubRepositoryFilePreview {
    fn get_raw_url(&self) -> &Url {
        &self.raw_url
    }

    fn get_file_extension(&self) -> Option<&String> {
        self.file_extension.as_ref()
    }
}

#[derive(Debug, PartialEq)]
enum FileType {
    File,
    Gist,
}

lazy_static! {
    static ref REGEX_GITHUB_FILE_URL: Regex =
        Regex::new(r"https://github\.com(?:/[^/\s]+){2}/blob(?:/[^/\s]+)+#[^/\s]+").unwrap();
    static ref REGEX_GITHUB_GIST_URL: Regex =
        Regex::new(r"https://gist\.github\.com(?:/[^/\s]+){2}#file\-[^\s]+").unwrap();
    static ref REGEX_GITHUB_LINE_NUMBER: Regex = Regex::new(r"L(\d+)").unwrap();
    static ref REGEX_GITHUB_GIST_FILE_NAME: Regex = Regex::new(r"file-(.+)-L").unwrap();
}

async fn send_file_preview(
    ctx: &Context,
    msg: &Message,
    data: FilePreviewData,
) -> Result<(), Box<dyn Error>> {
    let line_numbers: Vec<u32> = REGEX_GITHUB_LINE_NUMBER
        .captures_iter(
            &data
                .raw_url
                .fragment()
                .ok_or("The specified URL is malformed.")?,
        )
        .filter_map(|match_captures| match_captures[1].parse::<u32>().ok())
        .collect();

    let top_line_number = line_numbers
        .iter()
        .min()
        .ok_or("At least one line number is required.")?
        .clone();

    let bottom_line_number = line_numbers
        .iter()
        .max()
        .ok_or("At least one line number is required.")?
        .clone();

    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_str(if data.file_type == FileType::File {
            "text/plain"
        } else {
            "application/vnd.github.raw+json"
        })?,
    );

    let request = reqwest::Client::new()
        .get(&data.api_url)
        .headers(headers)
        .send()
        .await?;

    if !request.status().is_success() {
        return Err("API request failed.".into());
    }

    if request
        .content_length()
        .is_some_and(|file_size| file_size > 4_194_304)
    {
        return Err("File size is too large.".into());
    }

    let file_extension: String;
    let selected_content: Vec<String>;

    if data.file_type == FileType::File {
        let file = request.text().await?;

        selected_content = file
            .lines()
            .skip(top_line_number as usize - 1)
            .take((bottom_line_number - top_line_number + 1) as usize)
            .map(|line| line.to_owned())
            .collect();

        let file_name = &data
            .raw_url
            .path_segments()
            .and_then(|segments| segments.last())
            .ok_or("File name not found.")?;

        file_extension = PathBuf::from(file_name)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("txt")
            .to_owned();
    } else if data.file_type == FileType::Gist {
        // TODO: fix
        let gist: Gist = request.json().await?;

        let selected_file_name = REGEX_GITHUB_GIST_FILE_NAME
            .captures(
                &data
                    .raw_url
                    .fragment()
                    .expect("The specified URL is malformed."),
            )
            .ok_or("File name not found.")?[1]
            .to_owned();

        let file = gist
            .files
            .into_iter()
            .find(|(_, file)| file.filename == selected_file_name)
            .map(|(_, file)| file);

        selected_content = file
            .map(|file| {
                file.content
                    .lines()
                    .skip(top_line_number as usize - 1)
                    .take((bottom_line_number - top_line_number + 1) as usize)
                    .map(|line| line.to_owned())
                    .collect()
            })
            .unwrap_or_default();

        file_extension = PathBuf::from(selected_file_name)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_owned();
    } else {
        return Err("Invalid file type.".into());
    }

    if selected_content.is_empty() {
        return Err("No content selected.".into());
    }

    let line_number_length = (top_line_number + selected_content.len() as u32 - 1)
        .to_string()
        .len()
        .max(1);
    let file_content = selected_content
        .iter()
        .enumerate()
        .map(|(index, line)| {
            format!(
                "{:line_number_width$} | {}\n",
                top_line_number + index as u32,
                line,
                line_number_width = line_number_length
            )
        })
        .collect::<String>();

    let open_button = CreateButton::default()
        .url(&data.raw_url)
        .style(ButtonStyle::Link)
        .emoji('🔗')
        .label("Open")
        .to_owned();

    let delete_button = CreateButton::default()
        .custom_id("deleteFilePreview")
        .style(ButtonStyle::Secondary)
        .emoji('🗑')
        .to_owned();

    let mut reply: Message;
    if file_content.len() + data.metadata_content.len() > 1900 || selected_content.len() > 6 {
        let mut components = CreateActionRow::default();
        components.add_button(open_button);
        components.add_button(delete_button);
        reply = msg
            .channel_id
            .send_message(&ctx.http, |m| {
                m.content(data.metadata_content);
                m.add_file((
                    file_content.as_bytes(),
                    format!("preview.{}", file_extension).as_str(),
                ));
                m.components(|c| c.add_action_row(components))
            })
            .await?;
        if reply
            .attachments
            .first()
            .map(|a| a.content_type.is_none())
            .unwrap_or(false)
        {
            reply
                .edit(&ctx.http, |m| {
                    m.attachment((file_content.as_bytes(), "preview.txt"))
                })
                .await?;
        }
    } else {
        reply = msg
            .channel_id
            .send_message(&ctx.http, |m| {
                m.content(format!(
                    "{}\n```{}\n{}```",
                    data.metadata_content, file_extension, file_content
                ));
                m.components(|c| {
                    c.create_action_row(|a| a.add_button(open_button).add_button(delete_button))
                })
            })
            .await?;
    }

    Ok(())
}

async fn check_file_preview(ctx: &Context, msg: &mut Message) -> Result<(), Box<dyn Error>> {
    let mut total_preview_count = 0;

    let mut queue: Vec<FilePreviewData> = Vec::new();

    for raw_url_match in REGEX_GITHUB_FILE_URL.find_iter(&msg.content) {
        let raw_url = match Url::parse(raw_url_match.as_str()) {
            Ok(url) => url,
            Err(_) => continue,
        };

        let path_segments: Vec<&str> = raw_url.path_segments().unwrap().collect();

        let (author, repository, branch, path) = match path_segments.as_slice() {
            [author, repository, "blob", branch, path @ ..] => {
                (author, repository, branch, path.join("/"))
            }
            _ => continue,
        };

        let metadata_content = format!(
            "**{}**/**{}** (on {})\n{}",
            author, repository, branch, path
        );
        let api_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            author, repository, branch, path
        );

        queue.push(FilePreviewData {
            position: raw_url_match.start() as u32,
            raw_url: raw_url,
            api_url,
            metadata_content,
            file_type: FileType::File,
        });
    }

    for raw_url_match in REGEX_GITHUB_GIST_URL.find_iter(&msg.content) {
        let raw_url = match Url::parse(raw_url_match.as_str()) {
            Ok(url) => url,
            Err(_) => continue,
        };

        let path_segments: Vec<&str> = raw_url.path_segments().unwrap().collect();
        let (author, id) = match path_segments.as_slice() {
            [_, author, id] => (author, id),
            _ => continue,
        };

        let metadata_content = format!("**{}**", author);
        let api_url = format!("https://api.github.com/gists/{}", id);

        queue.push(FilePreviewData {
            position: raw_url_match.start() as u32,
            raw_url: raw_url,
            api_url,
            metadata_content,
            file_type: FileType::Gist,
        });
    }

    queue.sort_unstable_by_key(|preview| preview.position);

    for preview in queue {
        if total_preview_count >= 3 {
            break;
        }

        send_file_preview(&ctx, &msg, preview).await?;
        total_preview_count += 1;
    }

    if total_preview_count > 0 {
        let _ = msg.suppress_embeds(&ctx).await;
    }

    Ok(())
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, context: Context, mut msg: Message) {
        if !msg.author.bot {
            check_file_preview(&context, &mut msg).await.unwrap();
        }

        if msg.content == "!ping" {
            let channel = match msg.channel_id.to_channel(&context).await {
                Ok(channel) => channel,
                Err(why) => {
                    println!("Error getting channel: {:?}", why);

                    return;
                }
            };

            // The message builder allows for creating a message by
            // mentioning users dynamically, pushing "safe" versions of
            // content (such as bolding normalized content), displaying
            // emojis, and more.
            let response = MessageBuilder::new()
                .push("User ")
                .push_bold_safe(&msg.author.name)
                .push(" used the 'ping' command in the ")
                .mention(&channel)
                .push(" channel")
                .build();

            if let Err(why) = msg.channel_id.say(&context, &response).await {
                println!("Error sending message: {:?}", why);
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to load .env file.");

    let token = env::var("BOT_TOKEN").expect("Expected bot token in .env file.");

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Error while creating the client.");

    if let Err(error) = client.start().await {
        println!("Error while starting the client: {:?}", error);
    }
}
