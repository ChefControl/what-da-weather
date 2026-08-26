use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::retry::retry_if;

/// The weather parameters activity constraints can reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherParam {
    TemperatureC,
    WindKmh,
    PrecipitationMm,
    VisibilityKm,
    /// 1.0 while the sun is up at the location, 0.0 at night — lets conditions
    /// treat day/night as a checkable fact.
    IsDay,
}

impl WeatherParam {
    pub fn label(&self) -> &'static str {
        match self {
            WeatherParam::TemperatureC => "temperature (C)",
            WeatherParam::WindKmh => "wind (km/h)",
            WeatherParam::PrecipitationMm => "precipitation (mm)",
            WeatherParam::VisibilityKm => "visibility (km)",
            WeatherParam::IsDay => "daylight (1 = day, 0 = night)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherSnapshot {
    pub temperature_c: f64,
    pub wind_kmh: f64,
    pub precipitation_mm: f64,
    pub visibility_km: f64,
    pub weather_code: i64,
    pub is_day: bool,
}

impl WeatherSnapshot {
    pub fn get(&self, param: WeatherParam) -> f64 {
        match param {
            WeatherParam::TemperatureC => self.temperature_c,
            WeatherParam::WindKmh => self.wind_kmh,
            WeatherParam::PrecipitationMm => self.precipitation_mm,
            WeatherParam::VisibilityKm => self.visibility_km,
            WeatherParam::IsDay => {
                if self.is_day {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoLocation {
    pub name: String,
    pub country: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum WeatherError {
    #[error("city not found: {0}")]
    CityNotFound(String),
    #[error("weather provider request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("weather provider returned unexpected payload: {0}")]
    Malformed(String),
}

impl WeatherError {
    fn retryable(&self) -> bool {
        match self {
            // Transient transport failures (timeouts, resets, 5xx, mid-body
            // decode aborts) are worth retrying; permanent client errors
            // (4xx: bad request, rate limit, not found) fail fast instead of
            // hammering the rate-limited provider.
            WeatherError::Http(e) => !e.status().is_some_and(|s| s.is_client_error()),
            _ => false,
        }
    }
}

/// Abstraction over the external weather source (DESIGN.md D3): Open-Meteo is
/// the default; an OpenWeather implementation can be slotted in behind this trait.
#[async_trait]
pub trait WeatherProvider: Send + Sync {
    async fn fetch(&self, city: &str) -> Result<(GeoLocation, WeatherSnapshot), WeatherError>;
}

pub struct OpenMeteo {
    geocode_base: String,
    forecast_base: String,
    client: reqwest::Client,
}

impl OpenMeteo {
    pub fn new(geocode_base: String, forecast_base: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client");
        Self {
            geocode_base,
            forecast_base,
            client,
        }
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var("GEOCODE_URL")
                .unwrap_or_else(|_| "https://geocoding-api.open-meteo.com".to_string()),
            std::env::var("FORECAST_URL")
                .unwrap_or_else(|_| "https://api.open-meteo.com".to_string()),
        )
    }

    async fn geocode(&self, city: &str) -> Result<GeoLocation, WeatherError> {
        #[derive(Deserialize)]
        struct GeoResponse {
            results: Option<Vec<GeoResult>>,
        }
        #[derive(Deserialize)]
        struct GeoResult {
            name: String,
            latitude: f64,
            longitude: f64,
            country: Option<String>,
        }

        let url = format!("{}/v1/search", self.geocode_base);
        let resp: GeoResponse = self
            .client
            .get(&url)
            .query(&[
                ("name", city),
                ("count", "1"),
                ("language", "en"),
                ("format", "json"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let first = resp
            .results
            .and_then(|mut r| {
                if r.is_empty() {
                    None
                } else {
                    Some(r.remove(0))
                }
            })
            .ok_or_else(|| WeatherError::CityNotFound(city.to_string()))?;

        Ok(GeoLocation {
            name: first.name,
            country: first.country,
            latitude: first.latitude,
            longitude: first.longitude,
        })
    }

    async fn current(
        &self,
        latitude: f64,
        longitude: f64,
    ) -> Result<WeatherSnapshot, WeatherError> {
        #[derive(Deserialize)]
        struct ForecastResponse {
            current: CurrentBlock,
        }
        #[derive(Deserialize)]
        struct CurrentBlock {
            temperature_2m: f64,
            precipitation: f64,
            weather_code: i64,
            wind_speed_10m: f64,
            visibility: f64,
            is_day: i64,
        }

        let url = format!("{}/v1/forecast", self.forecast_base);
        let resp: ForecastResponse = self
            .client
            .get(&url)
            .query(&[
                ("latitude", latitude.to_string()),
                ("longitude", longitude.to_string()),
                (
                    "current",
                    "temperature_2m,precipitation,weather_code,wind_speed_10m,visibility,is_day"
                        .to_string(),
                ),
            ])
            .send()
            .await?
            .error_for_status()?
            // Decode failures stay `Http` (retryable), matching the geocode
            // stage: a connection reset mid-body is transient at both.
            .json()
            .await?;

        let c = resp.current;
        Ok(WeatherSnapshot {
            temperature_c: c.temperature_2m,
            wind_kmh: c.wind_speed_10m,
            precipitation_mm: c.precipitation,
            visibility_km: c.visibility / 1000.0, // Open-Meteo reports meters
            weather_code: c.weather_code,
            is_day: c.is_day == 1,
        })
    }
}

#[async_trait]
impl WeatherProvider for OpenMeteo {
    async fn fetch(&self, city: &str) -> Result<(GeoLocation, WeatherSnapshot), WeatherError> {
        let base = Duration::from_millis(300);
        let loc = retry_if(3, base, WeatherError::retryable, || self.geocode(city)).await?;
        let snap = retry_if(3, base, WeatherError::retryable, || {
            self.current(loc.latitude, loc.longitude)
        })
        .await?;
        Ok((loc, snap))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn provider(server_uri: &str) -> OpenMeteo {
        OpenMeteo::new(server_uri.to_string(), server_uri.to_string())
    }

    #[tokio::test]
    async fn fetch_happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/search"))
            .and(query_param("name", "Tel Aviv"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [{"name": "Tel Aviv", "latitude": 32.08, "longitude": 34.78, "country": "Israel"}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/forecast"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "current": {
                    "temperature_2m": 28.5,
                    "precipitation": 0.0,
                    "weather_code": 1,
                    "wind_speed_10m": 12.3,
                    "visibility": 24140.0,
                    "is_day": 1
                }
            })))
            .mount(&server)
            .await;

        let (loc, snap) = provider(&server.uri()).fetch("Tel Aviv").await.unwrap();
        assert_eq!(loc.name, "Tel Aviv");
        assert_eq!(loc.country.as_deref(), Some("Israel"));
        assert_eq!(snap.temperature_c, 28.5);
        assert_eq!(snap.wind_kmh, 12.3);
        assert_eq!(snap.visibility_km, 24.14); // meters converted to km
        assert!(snap.is_day);
    }

    #[tokio::test]
    async fn unknown_city_is_not_found_and_not_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(1) // CityNotFound must not be retried
            .mount(&server)
            .await;

        let err = provider(&server.uri()).fetch("Atlantis").await.unwrap_err();
        assert!(matches!(err, WeatherError::CityNotFound(c) if c == "Atlantis"));
    }

    #[tokio::test]
    async fn transient_5xx_is_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/search"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [{"name": "Haifa", "latitude": 32.79, "longitude": 34.99, "country": "Israel"}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/forecast"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "current": {
                    "temperature_2m": 22.0,
                    "precipitation": 0.0,
                    "weather_code": 2,
                    "wind_speed_10m": 8.0,
                    "visibility": 8000.0,
                    "is_day": 0
                }
            })))
            .mount(&server)
            .await;

        let (loc, snap) = provider(&server.uri()).fetch("Haifa").await.unwrap();
        assert_eq!(loc.name, "Haifa");
        assert!(!snap.is_day);
    }
}
