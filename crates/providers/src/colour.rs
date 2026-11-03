//! Colour maths shared by the providers: CSS colour parsing, OKLCH, WCAG contrast and the
//! calendar's event colours. Pure and deterministic; nothing here touches the UI.
//!
//! Event colour rules, in order:
//! 1. `[calendar.colors]`: a keyword matches when it occurs in the title, ignoring case.
//!    The longest matching keyword wins; if two have the same length, the one listed first
//!    in the config file wins.
//! 2. The iCal `COLOR` property (hex or a CSS colour name).
//! 3. A hue hashed from the title prefix (see [`prefix`]) at fixed OKLCH lightness and chroma.
//!
//! Colours from 1 and 2 are made lighter (OKLCH lightness only) until they reach
//! [`MIN_CONTRAST`] against the capsule surface. Every colour leaves as `#rrggbb`.

/// An sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `#rrggbb`, lower case.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

/// `--cap-bg` (`rgba(14, 14, 18, 0.94)`) composited over black.
pub const SURFACE: Rgb = Rgb(13, 13, 17);
/// The least contrast an event bar may have against [`SURFACE`] (WCAG non-text contrast).
pub const MIN_CONTRAST: f64 = 3.0;
/// OKLCH lightness of hashed event colours.
pub const HASH_LIGHTNESS: f64 = 0.74;
/// OKLCH chroma of hashed event colours.
pub const HASH_CHROMA: f64 = 0.12;

// --- WCAG ----------------------------------------------------------------------------------

/// WCAG 2.x relative luminance.
pub fn relative_luminance(c: Rgb) -> f64 {
    let f = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c.0) + 0.7152 * f(c.1) + 0.0722 * f(c.2)
}

/// WCAG contrast ratio, from 1 to 21.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

// --- OKLCH ---------------------------------------------------------------------------------

fn to_linear(v: u8) -> f64 {
    let v = f64::from(v) / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(v: f64) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Lightness, chroma and hue (degrees) of an sRGB colour.
pub fn to_oklch(c: Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (to_linear(c.0), to_linear(c.1), to_linear(c.2));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let big_l = 0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s;
    let a = 1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s;
    let b = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s;
    let hue = b.atan2(a).to_degrees().rem_euclid(360.0);
    (big_l, a.hypot(b), hue)
}

/// Linear sRGB of an OKLCH colour; channels may fall outside 0..=1.
fn oklch_linear(l: f64, c: f64, h: f64) -> (f64, f64, f64) {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m_ = (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s_ = (l - 0.089_484_177_5 * a - 1.291_485_548_0 * b).powi(3);
    (
        4.076_741_662_1 * l_ - 3.307_711_591_3 * m_ + 0.230_969_929_2 * s_,
        -1.268_438_004_6 * l_ + 2.609_757_401_1 * m_ - 0.341_319_396_5 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_614_7 * m_ + 1.707_614_701_0 * s_,
    )
}

/// The sRGB colour for an OKLCH one. If it is outside the sRGB gamut, chroma is reduced
/// (lightness and hue kept) until it fits.
pub fn from_oklch(l: f64, c: f64, h: f64) -> Rgb {
    let fits = |c: f64| {
        let (r, g, b) = oklch_linear(l, c, h);
        let ok = |v: f64| (-0.000_5..=1.000_5).contains(&v);
        ok(r) && ok(g) && ok(b)
    };
    let mut chroma = c.max(0.0);
    if !fits(chroma) {
        let (mut lo, mut hi) = (0.0, chroma);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        chroma = lo;
    }
    let (r, g, b) = oklch_linear(l, chroma, h);
    Rgb(from_linear(r), from_linear(g), from_linear(b))
}

/// `colour` itself if it has `min` contrast against [`SURFACE`]; otherwise the same hue and
/// chroma at the lowest lightness (in steps of 0.005) that does.
pub fn ensure_contrast(colour: Rgb, min: f64) -> Rgb {
    if contrast(colour, SURFACE) >= min {
        return colour;
    }
    let (l, c, h) = to_oklch(colour);
    let mut lightness = l;
    while lightness < 1.0 {
        lightness = (lightness + 0.005).min(1.0);
        let candidate = from_oklch(lightness, c, h);
        if contrast(candidate, SURFACE) >= min {
            return candidate;
        }
    }
    Rgb(255, 255, 255)
}

// --- Parsing -------------------------------------------------------------------------------

/// `#rgb`, `#rrggbb` (the `#` is optional) or a CSS colour name, ignoring case and spaces.
pub fn parse_css(text: &str) -> Option<Rgb> {
    let text = text.trim().trim_matches('"').trim();
    let digits = text.strip_prefix('#').unwrap_or(text);
    if matches!(digits.len(), 3 | 6) && digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        let v = u32::from_str_radix(digits, 16).ok()?;
        return Some(if digits.len() == 3 {
            let (r, g, b) = ((v >> 8) & 0xf, (v >> 4) & 0xf, v & 0xf);
            Rgb((r * 17) as u8, (g * 17) as u8, (b * 17) as u8)
        } else {
            Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
        });
    }
    if text.starts_with('#') {
        return None;
    }
    let name = text.to_ascii_lowercase();
    NAMED
        .binary_search_by_key(&name.as_str(), |(n, _)| n)
        .ok()
        .map(|i| {
            let v = NAMED[i].1;
            Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
        })
}

/// The CSS named colours, sorted by name.
const NAMED: [(&str, u32); 148] = [
    ("aliceblue", 0xF0F8FF),
    ("antiquewhite", 0xFAEBD7),
    ("aqua", 0x00FFFF),
    ("aquamarine", 0x7FFFD4),
    ("azure", 0xF0FFFF),
    ("beige", 0xF5F5DC),
    ("bisque", 0xFFE4C4),
    ("black", 0x000000),
    ("blanchedalmond", 0xFFEBCD),
    ("blue", 0x0000FF),
    ("blueviolet", 0x8A2BE2),
    ("brown", 0xA52A2A),
    ("burlywood", 0xDEB887),
    ("cadetblue", 0x5F9EA0),
    ("chartreuse", 0x7FFF00),
    ("chocolate", 0xD2691E),
    ("coral", 0xFF7F50),
    ("cornflowerblue", 0x6495ED),
    ("cornsilk", 0xFFF8DC),
    ("crimson", 0xDC143C),
    ("cyan", 0x00FFFF),
    ("darkblue", 0x00008B),
    ("darkcyan", 0x008B8B),
    ("darkgoldenrod", 0xB8860B),
    ("darkgray", 0xA9A9A9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xA9A9A9),
    ("darkkhaki", 0xBDB76B),
    ("darkmagenta", 0x8B008B),
    ("darkolivegreen", 0x556B2F),
    ("darkorange", 0xFF8C00),
    ("darkorchid", 0x9932CC),
    ("darkred", 0x8B0000),
    ("darksalmon", 0xE9967A),
    ("darkseagreen", 0x8FBC8F),
    ("darkslateblue", 0x483D8B),
    ("darkslategray", 0x2F4F4F),
    ("darkslategrey", 0x2F4F4F),
    ("darkturquoise", 0x00CED1),
    ("darkviolet", 0x9400D3),
    ("deeppink", 0xFF1493),
    ("deepskyblue", 0x00BFFF),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1E90FF),
    ("firebrick", 0xB22222),
    ("floralwhite", 0xFFFAF0),
    ("forestgreen", 0x228B22),
    ("fuchsia", 0xFF00FF),
    ("gainsboro", 0xDCDCDC),
    ("ghostwhite", 0xF8F8FF),
    ("gold", 0xFFD700),
    ("goldenrod", 0xDAA520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xADFF2F),
    ("grey", 0x808080),
    ("honeydew", 0xF0FFF0),
    ("hotpink", 0xFF69B4),
    ("indianred", 0xCD5C5C),
    ("indigo", 0x4B0082),
    ("ivory", 0xFFFFF0),
    ("khaki", 0xF0E68C),
    ("lavender", 0xE6E6FA),
    ("lavenderblush", 0xFFF0F5),
    ("lawngreen", 0x7CFC00),
    ("lemonchiffon", 0xFFFACD),
    ("lightblue", 0xADD8E6),
    ("lightcoral", 0xF08080),
    ("lightcyan", 0xE0FFFF),
    ("lightgoldenrodyellow", 0xFAFAD2),
    ("lightgray", 0xD3D3D3),
    ("lightgreen", 0x90EE90),
    ("lightgrey", 0xD3D3D3),
    ("lightpink", 0xFFB6C1),
    ("lightsalmon", 0xFFA07A),
    ("lightseagreen", 0x20B2AA),
    ("lightskyblue", 0x87CEFA),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xB0C4DE),
    ("lightyellow", 0xFFFFE0),
    ("lime", 0x00FF00),
    ("limegreen", 0x32CD32),
    ("linen", 0xFAF0E6),
    ("magenta", 0xFF00FF),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66CDAA),
    ("mediumblue", 0x0000CD),
    ("mediumorchid", 0xBA55D3),
    ("mediumpurple", 0x9370DB),
    ("mediumseagreen", 0x3CB371),
    ("mediumslateblue", 0x7B68EE),
    ("mediumspringgreen", 0x00FA9A),
    ("mediumturquoise", 0x48D1CC),
    ("mediumvioletred", 0xC71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xF5FFFA),
    ("mistyrose", 0xFFE4E1),
    ("moccasin", 0xFFE4B5),
    ("navajowhite", 0xFFDEAD),
    ("navy", 0x000080),
    ("oldlace", 0xFDF5E6),
    ("olive", 0x808000),
    ("olivedrab", 0x6B8E23),
    ("orange", 0xFFA500),
    ("orangered", 0xFF4500),
    ("orchid", 0xDA70D6),
    ("palegoldenrod", 0xEEE8AA),
    ("palegreen", 0x98FB98),
    ("paleturquoise", 0xAFEEEE),
    ("palevioletred", 0xDB7093),
    ("papayawhip", 0xFFEFD5),
    ("peachpuff", 0xFFDAB9),
    ("peru", 0xCD853F),
    ("pink", 0xFFC0CB),
    ("plum", 0xDDA0DD),
    ("powderblue", 0xB0E0E6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xFF0000),
    ("rosybrown", 0xBC8F8F),
    ("royalblue", 0x4169E1),
    ("saddlebrown", 0x8B4513),
    ("salmon", 0xFA8072),
    ("sandybrown", 0xF4A460),
    ("seagreen", 0x2E8B57),
    ("seashell", 0xFFF5EE),
    ("sienna", 0xA0522D),
    ("silver", 0xC0C0C0),
    ("skyblue", 0x87CEEB),
    ("slateblue", 0x6A5ACD),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xFFFAFA),
    ("springgreen", 0x00FF7F),
    ("steelblue", 0x4682B4),
    ("tan", 0xD2B48C),
    ("teal", 0x008080),
    ("thistle", 0xD8BFD8),
    ("tomato", 0xFF6347),
    ("turquoise", 0x40E0D0),
    ("violet", 0xEE82EE),
    ("wheat", 0xF5DEB3),
    ("white", 0xFFFFFF),
    ("whitesmoke", 0xF5F5F5),
    ("yellow", 0xFFFF00),
    ("yellowgreen", 0x9ACD32),
];

// --- Event colours -------------------------------------------------------------------------

/// FNV-1a, 64 bit: stable across runs, platforms and Rust versions.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The part of a title that names the series: the text before the first `:`, `-`, `–`,
/// `—`, `·`, `|` or ASCII digit, trimmed, lower-cased, with runs of spaces collapsed
/// ("COS 214: Lecture" and "cos 3" both give "cos"). A title that starts with such a
/// character is used whole.
pub fn prefix(title: &str) -> String {
    let tidy = |s: &str| -> String {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let end = title
        .find(|c: char| matches!(c, ':' | '-' | '–' | '—' | '·' | '|') || c.is_ascii_digit())
        .unwrap_or(title.len());
    let head = tidy(&title[..end]);
    if head.is_empty() { tidy(title) } else { head }
}

/// The colour hashed from a title's prefix.
pub fn hashed(title: &str) -> Rgb {
    let hue = (fnv1a(prefix(title).as_bytes()) % 360) as f64;
    from_oklch(HASH_LIGHTNESS, HASH_CHROMA, hue)
}

/// The `[calendar.colors]` keywords, ready to match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColorRules {
    /// Lower-case keyword and colour, in config order.
    rules: Vec<(String, Rgb)>,
}

impl ColorRules {
    /// Builds the rules, skipping keywords that are empty and colours that don't parse.
    pub fn new(pairs: &[(String, String)]) -> Self {
        let mut skipped = false;
        let rules = pairs
            .iter()
            .filter_map(|(keyword, colour)| {
                let keyword = keyword.trim().to_lowercase();
                match parse_css(colour) {
                    Some(rgb) if !keyword.is_empty() => Some((keyword, rgb)),
                    _ => {
                        skipped = true;
                        None
                    }
                }
            })
            .collect();
        if skipped {
            eprintln!("qapsule: calendar.colors: ignoring an entry that isn't a hex or CSS colour");
        }
        Self { rules }
    }

    /// The longest keyword found in the title; the earliest listed on a tie.
    fn find(&self, title: &str) -> Option<Rgb> {
        let title = title.to_lowercase();
        let mut best: Option<&(String, Rgb)> = None;
        for rule in &self.rules {
            if title.contains(&rule.0) && best.is_none_or(|b| rule.0.len() > b.0.len()) {
                best = Some(rule);
            }
        }
        best.map(|r| r.1)
    }
}

/// The `#rrggbb` for an event: config keyword, else the iCal colour, else the hashed hue.
/// `ical` is the `COLOR` text from the feed, if any.
pub fn event_color(title: &str, ical: Option<&str>, rules: &ColorRules) -> String {
    let chosen = rules
        .find(title)
        .or_else(|| ical.and_then(parse_css))
        .unwrap_or_else(|| hashed(title));
    ensure_contrast(chosen, MIN_CONTRAST).hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_matches_the_wcag_extremes() {
        assert!((contrast(Rgb(0, 0, 0), Rgb(255, 255, 255)) - 21.0).abs() < 1e-9);
        assert!((contrast(Rgb(9, 9, 9), Rgb(9, 9, 9)) - 1.0).abs() < 1e-9);
        assert!(relative_luminance(Rgb(255, 255, 255)) > 0.999);
    }

    #[test]
    fn the_surface_is_the_capsule_over_black() {
        let tokens = include_str!("../../../ui/src/styles/tokens.css");
        assert!(tokens.contains("--cap-bg: rgba(14, 14, 18, 0.94)"));
        assert_eq!(SURFACE, Rgb(13, 13, 17));
    }

    #[test]
    fn oklch_round_trips() {
        for c in [
            Rgb(255, 0, 0),
            Rgb(30, 144, 255),
            Rgb(128, 128, 128),
            Rgb(250, 250, 10),
        ] {
            let (l, ch, h) = to_oklch(c);
            let back = from_oklch(l, ch, h);
            for (a, b) in [(c.0, back.0), (c.1, back.1), (c.2, back.2)] {
                assert!(a.abs_diff(b) <= 1, "{c:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn css_colours_parse() {
        assert_eq!(parse_css("#7AA2F7"), Some(Rgb(0x7A, 0xA2, 0xF7)));
        assert_eq!(parse_css("7aa2f7"), Some(Rgb(0x7A, 0xA2, 0xF7)));
        assert_eq!(parse_css("#fa0"), Some(Rgb(255, 170, 0)));
        assert_eq!(parse_css(" Tomato "), Some(Rgb(255, 99, 71)));
        assert_eq!(parse_css("RebeccaPurple"), Some(Rgb(0x66, 0x33, 0x99)));
        assert_eq!(parse_css("#12345"), None);
        assert_eq!(parse_css("#zzzzzz"), None);
        assert_eq!(parse_css("not a colour"), None);
        assert_eq!(parse_css(""), None);
    }

    #[test]
    fn the_name_table_is_sorted_and_complete() {
        assert!(NAMED.windows(2).all(|w| w[0].0 < w[1].0));
        for (name, _) in NAMED {
            assert!(parse_css(name).is_some());
        }
    }

    #[test]
    fn prefixes_follow_the_rule() {
        assert_eq!(prefix("COS 214: Lecture"), "cos");
        assert_eq!(prefix("cos  333"), "cos");
        assert_eq!(prefix("Gym - legs"), "gym");
        assert_eq!(prefix("Team · sync"), "team");
        assert_eq!(prefix("Standup"), "standup");
        assert_eq!(prefix("9am standup"), "9am standup");
        assert_eq!(prefix(""), "");
    }

    #[test]
    fn hashing_is_stable() {
        // Pinned: a change here would recolour everyone's calendar.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        let none = ColorRules::default();
        let first = event_color("COS 214: Lecture", None, &none);
        for _ in 0..3 {
            assert_eq!(event_color("COS 214: Lecture", None, &none), first);
        }
        // Same prefix, same colour; a different course, a different colour.
        assert_eq!(first, event_color("cos 333 tut", None, &none));
        assert_ne!(first, event_color("WTW 285", None, &none));
    }

    fn rules(pairs: &[(&str, &str)]) -> ColorRules {
        let pairs: Vec<_> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        ColorRules::new(&pairs)
    }

    #[test]
    fn config_beats_ical_beats_the_hash() {
        let r = rules(&[("cos 214", "#7AA2F7")]);
        assert_eq!(
            event_color("Cos 214 lecture", Some("#ff0000"), &r),
            "#7aa2f7"
        );
        assert_eq!(event_color("Other", Some("#ff8800"), &r), "#ff8800");
        assert_eq!(
            event_color("Other", None, &r),
            event_color("Other", None, &ColorRules::default())
        );
        // An unreadable iCal colour falls through to the hash.
        assert_eq!(
            event_color("Other", Some("nonsense"), &r),
            event_color("Other", None, &r)
        );
    }

    #[test]
    fn the_longest_keyword_wins_then_the_first_listed() {
        let r = rules(&[
            ("cos", "#111111"),
            ("cos 214", "#aaaaaa"),
            ("lec", "#bbbbbb"),
        ]);
        assert_eq!(event_color("COS 214 lecture", None, &r), "#aaaaaa");
        let tie = rules(&[("ab", "#cccccc"), ("bc", "#dddddd")]);
        assert_eq!(event_color("abc", None, &tie), "#cccccc");
        assert_eq!(
            rules(&[("x", "nope"), ("", "#ffffff")]),
            ColorRules::default()
        );
    }

    #[test]
    fn dark_config_and_ical_colours_are_lifted_to_pass() {
        let r = rules(&[("dark", "#000080")]);
        let lifted = parse_css(&event_color("dark", None, &r)).unwrap();
        assert!(contrast(lifted, SURFACE) >= MIN_CONTRAST);
        let (l0, _, h0) = to_oklch(Rgb(0, 0, 0x80));
        let (l1, _, h1) = to_oklch(lifted);
        assert!(l1 > l0 && (h1 - h0).abs() < 8.0, "{h0} {h1}");
        // A colour that already passes is left alone.
        let none = ColorRules::default();
        assert_eq!(event_color("x", Some("#7AA2F7"), &none), "#7aa2f7");
        for dark in ["#000000", "#0d0d11", "navy", "#101010"] {
            let c = parse_css(&event_color("t", Some(dark), &none)).unwrap();
            assert!(contrast(c, SURFACE) >= MIN_CONTRAST, "{dark}");
        }
    }

    #[test]
    fn every_hue_and_many_titles_pass_contrast() {
        for hue in 0..360 {
            let c = from_oklch(HASH_LIGHTNESS, HASH_CHROMA, f64::from(hue));
            assert!(contrast(c, SURFACE) >= MIN_CONTRAST, "hue {hue}");
        }
        for i in 0..360 {
            let title = format!("course{i}x: lecture");
            let hex = event_color(&title, None, &ColorRules::default());
            let c = parse_css(&hex).unwrap();
            assert!(contrast(c, SURFACE) >= MIN_CONTRAST, "{title} {hex}");
        }
    }

    #[test]
    fn every_named_colour_and_a_hex_sweep_pass_after_clamping() {
        let none = ColorRules::default();
        for (name, _) in NAMED {
            let c = parse_css(&event_color("t", Some(name), &none)).unwrap();
            assert!(contrast(c, SURFACE) >= MIN_CONTRAST, "{name}");
        }
        for r in (0..=255).step_by(51) {
            for g in (0..=255).step_by(51) {
                for b in (0..=255).step_by(51) {
                    let hex = Rgb(r, g, b).hex();
                    let c = parse_css(&event_color("t", Some(&hex), &none)).unwrap();
                    assert!(contrast(c, SURFACE) >= MIN_CONTRAST, "{hex}");
                }
            }
        }
    }
}
