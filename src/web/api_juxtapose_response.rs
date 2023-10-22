use axum::http::{HeaderMap, HeaderValue, StatusCode};
use redis::AsyncCommands;
use reqwest::Url;
use serde::Serialize;
use std::{
    collections::HashMap,
    error::Error,
    time::{Duration, SystemTime},
};

#[derive(Debug, Serialize)]
pub(crate) struct APIJuxtaposeResponse {
    pub(crate) left_image_url: String,
    pub(crate) right_image_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) left_image_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) right_image_label: Option<String>,
}

impl APIJuxtaposeResponse {
    fn get_expire_unix_ts(&self) -> Result<usize, Box<dyn Error + Send + Sync>> {
        let left_ts = usize::from_str_radix(
            &Url::parse(self.left_image_url.as_str())?
                .query_pairs()
                .find(|(key, _)| key == "ex")
                .ok_or("Expire parameter of left URL not found.")?
                .1,
            16,
        )?;

        let right_ts = usize::from_str_radix(
            &Url::parse(self.left_image_url.as_str())?
                .query_pairs()
                .find(|(key, _)| key == "ex")
                .ok_or("Expire parameter of right URL not found.")?
                .1,
            16,
        )?;

        Ok(right_ts.min(left_ts))
    }

    pub(crate) fn get_cache_headers(expire_unix_ts: u64) -> HeaderMap {
        HeaderMap::from_iter([
            (
                axum::http::header::EXPIRES,
                httpdate::fmt_http_date(
                    SystemTime::UNIX_EPOCH + Duration::from_secs(expire_unix_ts),
                )
                .parse()
                .unwrap(),
            ),
            (
                axum::http::header::CACHE_CONTROL,
                HeaderValue::from_static("public, must-revalidate, immutable"),
            ),
        ])
    }

    pub(crate) async fn redis_cache_set(
        &self,
        connection: &mut redis::aio::ConnectionManager,
        key: &str,
    ) -> Result<usize, StatusCode> {
        let mut data = vec![
            ("left_image", self.left_image_url.as_str()),
            ("right_image", self.right_image_url.as_str()),
        ];

        if let Some(left_image_label) = &self.left_image_label {
            data.push(("left_label", left_image_label.as_str()));
        }

        if let Some(right_image_label) = &self.right_image_label {
            data.push(("right_label", right_image_label.as_str()));
        }

        connection
            .hset_multiple(key, &data)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let unix_ts = self.get_expire_unix_ts().map_err(|err| {
            println!("Error while getting expire timestamp: {:?}", err);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        connection.expire_at(key, unix_ts).await.map_err(|err| {
            println!("Error while setting expire timestamp: {:?}", err);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        Ok(unix_ts)
    }

    pub(crate) async fn redis_cache_get_data(
        connection: &mut redis::aio::ConnectionManager,
        key: &str,
    ) -> Option<Self> {
        connection
            .hgetall::<&str, HashMap<String, String>>(key)
            .await
            .ok()
            .and_then(|cached_urls| {
                match (
                    cached_urls.get("left_image"),
                    cached_urls.get("right_image"),
                ) {
                    (Some(left_image_url), Some(right_image_url)) => Some(APIJuxtaposeResponse {
                        left_image_url: left_image_url.to_owned(),
                        right_image_url: right_image_url.to_owned(),
                        left_image_label: cached_urls.get("left_label").cloned(),
                        right_image_label: cached_urls.get("right_label").cloned(),
                    }),
                    _ => None,
                }
            })
    }

    pub(crate) async fn redis_cache_get_expire(
        connection: &mut redis::aio::ConnectionManager,
        key: &str,
    ) -> Result<usize, StatusCode> {
        redis::cmd("EXPIRETIME")
            .arg(key)
            .query_async(connection)
            .await
            .map_err(|err| {
                println!("Error while getting expire timestamp: {:?}", err);
                StatusCode::INTERNAL_SERVER_ERROR
            })
    }
}
