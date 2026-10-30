//! WMO weather codes as Open-Meteo reports them.

/// The condition text and glyph name for a WMO code. Unknown codes read as cloudy.
pub fn describe(code: u32) -> (&'static str, &'static str) {
    match code {
        0 => ("Clear", "clear"),
        1 | 2 => ("Partly cloudy", "partly_cloudy"),
        3 => ("Cloudy", "cloudy"),
        45 | 48 => ("Fog", "fog"),
        51..=57 => ("Drizzle", "drizzle"),
        61..=67 | 80..=82 => ("Rain", "rain"),
        71..=77 | 85..=86 => ("Snow", "snow"),
        95..=99 => ("Thunderstorm", "storm"),
        _ => ("Cloudy", "cloudy"),
    }
}

#[cfg(test)]
mod tests {
    use super::describe;

    #[test]
    fn maps_every_group() {
        assert_eq!(describe(0), ("Clear", "clear"));
        assert_eq!(describe(1), ("Partly cloudy", "partly_cloudy"));
        assert_eq!(describe(2), ("Partly cloudy", "partly_cloudy"));
        assert_eq!(describe(3), ("Cloudy", "cloudy"));
        assert_eq!(describe(45), ("Fog", "fog"));
        assert_eq!(describe(48), ("Fog", "fog"));
        assert_eq!(describe(51), ("Drizzle", "drizzle"));
        assert_eq!(describe(57), ("Drizzle", "drizzle"));
        assert_eq!(describe(61), ("Rain", "rain"));
        assert_eq!(describe(67), ("Rain", "rain"));
        assert_eq!(describe(81), ("Rain", "rain"));
        assert_eq!(describe(71), ("Snow", "snow"));
        assert_eq!(describe(77), ("Snow", "snow"));
        assert_eq!(describe(86), ("Snow", "snow"));
        assert_eq!(describe(95), ("Thunderstorm", "storm"));
        assert_eq!(describe(99), ("Thunderstorm", "storm"));
    }

    #[test]
    fn unknown_codes_are_cloudy() {
        assert_eq!(describe(4), ("Cloudy", "cloudy"));
        assert_eq!(describe(1000), ("Cloudy", "cloudy"));
    }
}
