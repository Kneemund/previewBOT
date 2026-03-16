use std::error::Error;
use std::path::PathBuf;

use percent_encoding::percent_decode_str;
use reqwest::Url;
use serenity::all::MessageBuilder;

use super::{FilePreview, fetch_raw_content};

pub struct GitHubRepositoryFilePreview {
    message_url: Url,
    metadata_content: String,
    file_extension: Option<String>,
    raw_content: String,
    action_row_buttons: Vec<(char, String, String)>,
}

fn get_short_reference(reference: &str) -> &str {
    if reference.len() == 40 && reference.chars().all(|c| c.is_ascii_hexdigit()) {
        &reference[..7]
    } else {
        reference
    }
}

impl GitHubRepositoryFilePreview {
    pub async fn new(message_url: Url) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let path_segments: Vec<&str> = message_url.path_segments().unwrap().collect();

        let (author, repository, reference, urlencoded_path) = match path_segments.as_slice() {
            [
                author,
                repository,
                "blob" | "blame",
                reference,
                urlencoded_path @ ..,
            ] => (author, repository, reference, urlencoded_path.join("/")),
            _ => return Err("Malformed GitHub repository URL.".into()),
        };

        let path = percent_decode_str(urlencoded_path.as_str())
            .decode_utf8()
            .map_err(|_| "Failed to decode GitHub URL file path.")?;

        let metadata_content = MessageBuilder::new()
            .push_bold_safe(author.to_owned())
            .push("/")
            .push_bold_safe(repository.to_owned())
            .push(" (on ")
            .push_safe(get_short_reference(reference))
            .push_line(")")
            .push_line_safe(path.as_ref())
            .build();

        let mut raw_url = Url::parse("https://raw.githubusercontent.com/").unwrap();
        raw_url.path_segments_mut().unwrap().extend(&[
            author,
            repository,
            reference,
            path.as_ref(),
        ]);

        let file_name = *path_segments.last().ok_or("File name not found.")?;
        let file_extension = PathBuf::from(file_name)
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned());

        let raw_content = fetch_raw_content(raw_url).await?;

        let mut author_url = Url::parse("https://github.com/").unwrap();
        author_url.path_segments_mut().unwrap().push(author);

        let mut repository_url = author_url.clone();
        repository_url
            .path_segments_mut()
            .unwrap()
            .extend(&[repository, "tree", reference]);

        let action_row_buttons = vec![
            ('📄', file_name.to_string(), message_url.to_string()),
            ('🗃', repository.to_string(), repository_url.to_string()),
            ('👥', author.to_string(), author_url.to_string()),
        ];

        Ok(Self {
            message_url,
            metadata_content,
            file_extension,
            raw_content,
            action_row_buttons,
        })
    }
}

impl FilePreview for GitHubRepositoryFilePreview {
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

    fn get_action_row_buttons(&self) -> &Vec<(char, String, String)> {
        &self.action_row_buttons
    }
}
