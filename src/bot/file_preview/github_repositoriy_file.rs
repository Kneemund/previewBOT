use std::error::Error;
use std::path::PathBuf;

use reqwest::Url;
use serenity::utils::MessageBuilder;

use super::{fetch_raw_content, FilePreview};

pub struct GitHubRepositoryFilePreview {
    message_url: Url,
    metadata_content: String,
    file_extension: Option<String>,
    raw_content: String,
}

impl GitHubRepositoryFilePreview {
    pub async fn new(message_url: Url) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let path_segments: Vec<&str> = message_url.path_segments().unwrap().collect();

        let (author, repository, branch, path) = match path_segments.as_slice() {
            [author, repository, "blob" | "blame", branch, path @ ..] => {
                (author, repository, branch, path.join("/"))
            }
            _ => return Err("Malformed GitHub repository URL.".into()),
        };

        let metadata_content = MessageBuilder::new()
            .push_bold_safe(author.to_owned())
            .push("/")
            .push_bold_safe(repository.to_owned())
            .push(" (on ")
            .push_safe(branch.to_owned())
            .push_line(")")
            .push_line_safe(path.as_str())
            .build();

        let mut raw_url = Url::parse("https://raw.githubusercontent.com/").unwrap();
        raw_url
            .path_segments_mut()
            .unwrap()
            .extend(&[author, repository, branch, path.as_str()]);

        let file_name = message_url
            .path_segments()
            .and_then(|segments| segments.last())
            .ok_or("File name not found.")?;

        let file_extension = PathBuf::from(file_name)
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned());

        let raw_content = fetch_raw_content(raw_url).await?;

        Ok(Self {
            message_url,
            metadata_content,
            file_extension,
            raw_content,
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
}
