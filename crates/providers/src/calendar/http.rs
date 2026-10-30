//! The real fetcher: the secret iCal link over HTTPS (Schannel on Windows).
//! Errors keep only a fixed reason or a status code, never the link.

use std::future::Future;
use std::time::Duration;

use reqwest::Client;

use super::{FetchError, Fetcher};

const TIMEOUT: Duration = Duration::from_secs(10);
/// Larger responses are refused.
pub const MAX_BYTES: usize = 5 * 1024 * 1024;

pub struct Http {
    client: Option<Client>,
}

impl Http {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(TIMEOUT)
                .https_only(true)
                .build()
                .ok(),
        }
    }
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Fetcher for Http {
    fn fetch(&self, link: &str) -> impl Future<Output = Result<String, FetchError>> + Send {
        let link = link.to_string();
        async move {
            let client = self
                .client
                .as_ref()
                .ok_or(FetchError::new("no http client"))?;
            let mut response = client.get(&link).send().await.map_err(|e| {
                FetchError::new(if e.is_timeout() {
                    "timeout"
                } else {
                    "network error"
                })
            })?;
            let status = response.status();
            if !status.is_success() {
                return Err(FetchError::status(status.as_u16()));
            }
            if response
                .content_length()
                .is_some_and(|n| n > MAX_BYTES as u64)
            {
                return Err(FetchError::new("response too large"));
            }
            let mut body: Vec<u8> = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| FetchError::new("network error"))?
            {
                if body.len() + chunk.len() > MAX_BYTES {
                    return Err(FetchError::new("response too large"));
                }
                body.extend_from_slice(&chunk);
            }
            Ok(String::from_utf8_lossy(&body).into_owned())
        }
    }
}
