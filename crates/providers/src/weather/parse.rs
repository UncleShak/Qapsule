//! Open-Meteo responses to Qapsule's weather view. Pure: no network.

use qapsule_core::view::{HourView, WeatherReady};
use serde::Deserialize;

use super::wmo;

/// How many hours the hourly row shows, the current one included.
const HOURS: usize = 5;

/// A geocoded city.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// The response was not the shape we expect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub &'static str);

#[derive(Deserialize)]
struct GeocodeResponse {
    results: Option<Vec<GeocodeResult>>,
}

#[derive(Deserialize)]
struct GeocodeResult {
    name: String,
    latitude: f64,
    longitude: f64,
}

/// The first match, or `None` when the name matches nothing.
pub fn parse_geocode(body: &str) -> Result<Option<Place>, ParseError> {
    let response: GeocodeResponse =
        serde_json::from_str(body).map_err(|_| ParseError("bad geocode response"))?;
    Ok(response
        .results
        .and_then(|results| results.into_iter().next())
        .map(|r| Place {
            name: r.name,
            latitude: r.latitude,
            longitude: r.longitude,
        }))
}

#[derive(Deserialize)]
struct ForecastResponse {
    current: Current,
    hourly: Hourly,
    daily: Daily,
}

#[derive(Deserialize)]
struct Current {
    time: String,
    temperature_2m: f64,
    weather_code: u32,
}

#[derive(Deserialize)]
struct Hourly {
    time: Vec<String>,
    temperature_2m: Vec<f64>,
    weather_code: Vec<u32>,
}

#[derive(Deserialize)]
struct Daily {
    time: Vec<String>,
    temperature_2m_max: Vec<f64>,
    temperature_2m_min: Vec<f64>,
}

fn round(value: f64) -> i32 {
    value.round() as i32
}

/// The view for `place_name`: now, today's range and the next hours.
pub fn parse_forecast(body: &str, place_name: &str) -> Result<WeatherReady, ParseError> {
    let f: ForecastResponse =
        serde_json::from_str(body).map_err(|_| ParseError("bad forecast response"))?;

    // "2026-10-31T01:15" -> the date, and the hour slot "2026-10-31T01:00".
    let now = f.current.time.as_str();
    let date = now.get(..10).ok_or(ParseError("bad current time"))?;
    let hour_slot = format!(
        "{}:00",
        now.get(..13).ok_or(ParseError("bad current time"))?
    );

    let day = f
        .daily
        .time
        .iter()
        .position(|d| d == date)
        .ok_or(ParseError("today missing from daily"))?;
    let high = *f
        .daily
        .temperature_2m_max
        .get(day)
        .ok_or(ParseError("daily high missing"))?;
    let low = *f
        .daily
        .temperature_2m_min
        .get(day)
        .ok_or(ParseError("daily low missing"))?;

    let start = f
        .hourly
        .time
        .iter()
        .position(|t| *t == hour_slot)
        .ok_or(ParseError("current hour missing from hourly"))?;
    let mut hourly = Vec::with_capacity(HOURS);
    for i in start..(start + HOURS).min(f.hourly.time.len()) {
        let temp = *f
            .hourly
            .temperature_2m
            .get(i)
            .ok_or(ParseError("hourly temperature missing"))?;
        let code = *f
            .hourly
            .weather_code
            .get(i)
            .ok_or(ParseError("hourly code missing"))?;
        let label = if i == start {
            "Now".to_string()
        } else {
            // "2026-10-31T02:00" -> "02:00"
            f.hourly.time[i]
                .get(11..16)
                .ok_or(ParseError("bad hourly time"))?
                .to_string()
        };
        hourly.push(HourView {
            label,
            temp_c: round(temp),
            glyph: wmo::describe(code).1.to_string(),
        });
    }

    let (condition, glyph) = wmo::describe(f.current.weather_code);
    Ok(WeatherReady {
        city: place_name.to_string(),
        temp_c: round(f.current.temperature_2m),
        condition: condition.to_string(),
        glyph: glyph.to_string(),
        high_c: round(high),
        low_c: round(low),
        hourly,
        stale: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEOCODE: &str = include_str!("../../samples/weather/geocode-pretoria.json");
    const GEOCODE_NONE: &str = include_str!("../../samples/weather/geocode-none.json");
    const FORECAST: &str = include_str!("../../samples/weather/forecast-pretoria.json");

    #[test]
    fn geocode_takes_the_first_result() {
        let place = parse_geocode(GEOCODE).unwrap().unwrap();
        assert_eq!(place.name, "Pretoria");
        assert!((place.latitude - -25.74486).abs() < 1e-6);
        assert!((place.longitude - 28.18783).abs() < 1e-6);
    }

    #[test]
    fn geocode_without_results_is_none() {
        assert_eq!(parse_geocode(GEOCODE_NONE), Ok(None));
    }

    #[test]
    fn geocode_rejects_garbage() {
        assert!(parse_geocode("not json").is_err());
        assert!(parse_geocode("[]").is_err());
    }

    #[test]
    fn forecast_builds_the_view() {
        let w = parse_forecast(FORECAST, "Pretoria").unwrap();
        assert_eq!(w.city, "Pretoria");
        assert!(!w.stale);
        // Captured at 2026-10-31T01:xx: 19.9 degrees, clear, high 33.3, low 16.7.
        assert_eq!(w.temp_c, 20);
        assert_eq!((w.condition.as_str(), w.glyph.as_str()), ("Clear", "clear"));
        assert_eq!((w.high_c, w.low_c), (33, 17));
        assert_eq!(w.hourly.len(), 5);
        assert_eq!(w.hourly[0].label, "Now");
        assert_eq!(w.hourly[1].label, "02:00");
        assert_eq!(w.hourly[4].label, "05:00");
    }

    fn forecast(current_time: &str, hourly_len: usize) -> String {
        let times: Vec<String> = (0..hourly_len)
            .map(|h| format!("\"2026-10-31T{h:02}:00\""))
            .collect();
        let temps: Vec<String> = (0..hourly_len).map(|h| format!("{h}.5")).collect();
        let codes = vec!["61"; hourly_len];
        format!(
            r#"{{"current":{{"time":"{current_time}","temperature_2m":-0.4,"weather_code":95}},
            "hourly":{{"time":[{}],"temperature_2m":[{}],"weather_code":[{}]}},
            "daily":{{"time":["2026-10-31"],"temperature_2m_max":[10.5],"temperature_2m_min":[-3.6]}}}}"#,
            times.join(","),
            temps.join(","),
            codes.join(",")
        )
    }

    #[test]
    fn rounds_and_starts_at_the_current_hour() {
        let w = parse_forecast(&forecast("2026-10-31T10:45", 24), "X").unwrap();
        assert_eq!(w.temp_c, 0);
        assert_eq!((w.high_c, w.low_c), (11, -4));
        assert_eq!(w.glyph, "storm");
        assert_eq!(w.hourly[0].label, "Now");
        assert_eq!(w.hourly[0].temp_c, 11); // 10.5 rounds away from zero
        assert_eq!(w.hourly[0].glyph, "rain");
        assert_eq!(w.hourly[1].label, "11:00");
    }

    #[test]
    fn the_hourly_row_stops_at_the_data() {
        let w = parse_forecast(&forecast("2026-10-31T22:10", 24), "X").unwrap();
        assert_eq!(w.hourly.len(), 2);
    }

    #[test]
    fn forecast_rejects_bad_shapes() {
        assert!(parse_forecast("{}", "X").is_err());
        // Today is not in the daily list.
        assert!(parse_forecast(&forecast("2026-11-02T10:00", 24), "X").is_err());
    }
}
