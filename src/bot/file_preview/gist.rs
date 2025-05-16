use std::error::Error;
use std::path::PathBuf;

use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serenity::all::MessageBuilder;

use crate::HTTP_CLIENT;

use super::{FilePreview, fetch_raw_content, truncate_string};

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

static FILE_NAME_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"file-([^L]+)").unwrap());

#[derive(Debug)]
pub struct GistFilePreview {
    message_url: Url,
    metadata_content: String,
    file_extension: Option<String>,
    raw_content: String,
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

    pub async fn new(message_url: Url) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let selected_file_name_fragment = Self::normalize_file_name(
            &FILE_NAME_REGEX
                .captures(
                    message_url
                        .fragment()
                        .ok_or("The specified URL is malformed.")?,
                )
                .ok_or("File name not found.")?[1],
        );

        let mut metadata_url = message_url.clone();
        metadata_url.set_fragment(None);
        metadata_url.set_path((metadata_url.path().to_owned() + ".json").as_str());

        let response = HTTP_CLIENT.get(metadata_url).send().await?;

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
            .map(|extension| extension.to_string_lossy().into_owned());

        let mut raw_url = message_url.clone();
        raw_url.set_fragment(None);
        raw_url
            .path_segments_mut()
            .unwrap()
            .push("raw")
            .push(selected_file_name);

        let mut metadata_content_builder = MessageBuilder::new()
            .push_bold_line_safe(metadata.owner.as_str())
            .push_line_safe(selected_file_name.as_str());

        if !metadata.description.is_empty() {
            metadata_content_builder = metadata_content_builder
                .push_quote_line_safe(truncate_string(metadata.description, 128).as_str());
        }

        let raw_content = fetch_raw_content(raw_url).await?;

        Ok(Self {
            message_url,
            metadata_content: metadata_content_builder.build(),
            file_extension,
            raw_content,
        })
    }
}

impl FilePreview for GistFilePreview {
    fn get_message_url(&self) -> &Url {
        &self.message_url
    }

    fn get_metadata_content(&self) -> &str {
        self.metadata_content.as_str()
    }

    fn get_file_extension(&self) -> Option<&str> {
        self.file_extension.as_deref()
    }

    fn get_raw_content(&self) -> &str {
        self.raw_content.as_str()
    }
}
