use axum::http::StatusCode;
use base64::{engine::general_purpose, Engine};
use serde::Deserialize;

use crate::BLAKE3_JUXTAPOSE_KEY;

#[derive(Debug, Deserialize)]
pub(crate) struct APIJuxtaposeRequest {
    #[serde(rename = "d")]
    pub(crate) data: String,
    #[serde(rename = "m")]
    mac: String,
}

impl APIJuxtaposeRequest {
    pub(crate) fn is_decoded_data_valid(
        &self,
        decoded_data_bytes: &[u8],
    ) -> Result<bool, StatusCode> {
        let mac_bytes = general_purpose::URL_SAFE_NO_PAD
            .decode(self.mac.as_str())
            .map_err(|_| StatusCode::BAD_REQUEST)?;

        let mac_bytes: &[u8; 16] = mac_bytes
            .as_slice()
            .try_into()
            .map_err(|_| StatusCode::BAD_REQUEST)?;

        let mut mac_calculated = [0u8; 16];
        blake3::Hasher::new_keyed(&BLAKE3_JUXTAPOSE_KEY)
            .update(decoded_data_bytes)
            .finalize_xof()
            .fill(&mut mac_calculated);

        Ok(constant_time_eq::constant_time_eq_16(
            mac_bytes,
            &mac_calculated,
        ))
    }
}
