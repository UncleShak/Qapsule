//! The real fetcher: Open-Meteo over HTTPS (Schannel on Windows).

use std::future::Future;
use std::time::Duration;

use qapsule_core::view::WeatherReady;
use reqwest::{Client, RequestBuilder};

use super::parse::{Place, parse_forecast, parse_geocode};
use super::{FetchError, Fetcher};

const GEOCODE_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";
const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";
const TIMEOUT: Duration = Duration::from_secs(10);

pub struct Http {
    client: Option<Client>,
}

impl Http {
    pub fn new() -> Self {
        Self {
            client: Client::builder().timeout(TIMEOUT).build().ok(),
        }
    }

    async fn get(&self, build: impl Fn(&Client) -> RequestBuilder) -> Result<String, FetchError> {
        let client = self
            .client
            .as_ref()
            .ok_or(FetchError::new("no http client"))?;
        // `reqwest::Error` can carry the URL, so only a fixed reason is kept.
        let response = build(client).send().await.map_err(|e| {
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
        response
            .text()
            .await
            .map_err(|_| FetchError::new("network error"))
    }
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn geocode_request(client: &Client, city: &str) -> RequestBuilder {
    client.get(GEOCODE_URL).query(&[
        ("name", city),
        ("count", "1"),
        ("language", "en"),
        ("format", "json"),
    ])
}

pub(super) fn forecast_request(client: &Client, place: &Place) -> RequestBuilder {
    client.get(FORECAST_URL).query(&[
        ("latitude", place.latitude.to_string().as_str()),
        ("longitude", place.longitude.to_string().as_str()),
        ("current", "temperature_2m,weather_code"),
        ("hourly", "temperature_2m,weather_code"),
        ("daily", "temperature_2m_max,temperature_2m_min"),
        ("timezone", "auto"),
        ("forecast_days", "2"),
    ])
}

impl Fetcher for Http {
    fn geocode(
        &self,
        city: &str,
    ) -> impl Future<Output = Result<Option<Place>, FetchError>> + Send {
        let city = city.to_string();
        async move {
            let body = self.get(|c| geocode_request(c, &city)).await?;
            parse_geocode(&body).map_err(|e| FetchError::new(e.0))
        }
    }

    fn forecast(
        &self,
        place: &Place,
    ) -> impl Future<Output = Result<WeatherReady, FetchError>> + Send {
        let place = place.clone();
        async move {
            let body = self.get(|c| forecast_request(c, &place)).await?;
            parse_forecast(&body, &place.name).map_err(|e| FetchError::new(e.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_city_is_percent_encoded() {
        let client = Client::new();
        let request = geocode_request(&client, "São Paulo & co").build().unwrap();
        let url = request.url().as_str();
        assert!(url.starts_with("https://geocoding-api.open-meteo.com/v1/search?"));
        assert!(
            url.contains("name=S%C3%A3o+Paulo+%26+co")
                || url.contains("name=S%C3%A3o%20Paulo%20%26%20co")
        );
        assert!(url.contains("count=1") && url.contains("format=json"));
    }

    #[test]
    fn the_forecast_asks_for_what_we_parse() {
        let client = Client::new();
        let place = Place {
            name: "Pretoria".into(),
            latitude: -25.74486,
            longitude: 28.18783,
        };
        let request = forecast_request(&client, &place).build().unwrap();
        let url = request.url().as_str();
        assert!(url.starts_with("https://api.open-meteo.com/v1/forecast?"));
        assert!(url.contains("latitude=-25.74486") && url.contains("longitude=28.18783"));
        assert!(url.contains("current=temperature_2m%2Cweather_code"));
        assert!(url.contains("timezone=auto") && url.contains("forecast_days=2"));
    }
}
