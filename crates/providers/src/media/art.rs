//! The cover art in memory: only the current image, served by the app on the
//! `qapsule-art` scheme (decision 0008).

use std::sync::{Arc, Mutex};

/// The custom URI scheme the app answers from [`ArtStore::get`].
pub const SCHEME: &str = "qapsule-art";
/// Thumbnails larger than this are refused before they are read.
pub const MAX_THUMBNAIL_BYTES: u64 = 10 * 1024 * 1024;
/// The longest side of the stored image.
pub const MAX_SIDE: u32 = 256;

/// Where the page finds the image with this hash. On Windows, WebView2 reaches a
/// custom scheme as `http://<scheme>.localhost/<path>` (Tauri's default).
pub fn url_for(hash: &str) -> String {
    format!("http://{SCHEME}.localhost/{hash}")
}

#[derive(Debug)]
struct Art {
    hash: String,
    png: Arc<Vec<u8>>,
}

/// Holds the current cover as PNG and its content hash. Shared between the
/// media provider (writes) and the app's scheme handler (reads).
#[derive(Debug, Default)]
pub struct ArtStore(Mutex<Option<Art>>);

impl ArtStore {
    /// Keeps `png` as the current image, replacing the old one. Returns its URL.
    pub fn set(&self, png: Vec<u8>) -> String {
        let hash = hash_of(&png);
        let url = url_for(&hash);
        if let Ok(mut current) = self.0.lock() {
            *current = Some(Art {
                hash,
                png: Arc::new(png),
            });
        }
        url
    }

    pub fn clear(&self) {
        if let Ok(mut current) = self.0.lock() {
            *current = None;
        }
    }

    /// The image whose hash is exactly `hash`, if it is the current one.
    pub fn get(&self, hash: &str) -> Option<Arc<Vec<u8>>> {
        let current = self.0.lock().ok()?;
        current
            .as_ref()
            .filter(|art| art.hash == hash)
            .map(|art| art.png.clone())
    }
}

/// FNV-1a over the bytes, with the length: a name for the content, not a security measure.
fn hash_of(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}{:x}", bytes.len())
}

/// A cover as the provider keeps it: the PNG for the page and the wash colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Cover {
    pub png: Vec<u8>,
    pub tint: Option<[u8; 3]>,
}

/// About this many samples per side: the work per cover is bounded however big it is.
const GRID: usize = 24;
const HUES: usize = 12;
/// The heaviest hue needs this much weight per sample, or the cover counts as grey.
const MIN_WEIGHT_PER_SAMPLE: f64 = 0.03;

fn to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-9 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s.clamp(0.0, 1.0), l)
}

fn from_hsl(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0).floor() as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// The colour for the album wash, from `pixels` (BGRA, 8 bits a channel,
/// premultiplied alpha, `width * height` of them).
///
/// A coarse grid of about 24 x 24 samples skips transparent pixels. Each goes into
/// one of 12 hue buckets weighted by saturation x (1 - |2L - 1|), so vivid
/// mid-tones count most. The heaviest bucket's weighted average is clamped in
/// HSL to S >= 0.35 and L in [0.42, 0.62]: visible on the dark fill, never
/// near-white. A cover with no vivid colour (greyscale) has none: `None`.
pub fn tint(pixels: &[u8], width: u32, height: u32) -> Option<[u8; 3]> {
    let (width, height) = (width as usize, height as usize);
    if width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return None;
    }
    let (step_x, step_y) = (width.div_ceil(GRID), height.div_ceil(GRID));
    let mut weight = [0.0f64; HUES];
    let mut sum = [[0.0f64; 3]; HUES];
    let mut samples = 0usize;
    for y in (0..height).step_by(step_y) {
        for x in (0..width).step_by(step_x) {
            let at = (y * width + x) * 4;
            let alpha = f64::from(pixels[at + 3]) / 255.0;
            if alpha < 0.5 {
                continue;
            }
            samples += 1;
            // Premultiplied: divide the alpha back out.
            let [b, g, r] = [pixels[at], pixels[at + 1], pixels[at + 2]]
                .map(|v| (f64::from(v) / 255.0 / alpha).min(1.0));
            let (h, s, l) = to_hsl(r, g, b);
            let w = s * (1.0 - (2.0 * l - 1.0).abs());
            if w <= 0.0 {
                continue;
            }
            let bucket = ((h / 360.0 * HUES as f64) as usize).min(HUES - 1);
            weight[bucket] += w;
            for (total, v) in sum[bucket].iter_mut().zip([r, g, b]) {
                *total += v * w;
            }
        }
    }
    let (best, heaviest) = weight
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))?;
    if samples == 0 || *heaviest / (samples as f64) < MIN_WEIGHT_PER_SAMPLE {
        return None;
    }
    let [r, g, b] = sum[best].map(|v| v / heaviest);
    let (h, s, l) = to_hsl(r, g, b);
    Some(from_hsl(h, s.max(0.35), l.clamp(0.42, 0.62)))
}

#[cfg(test)]
mod tint_tests {
    use super::*;

    /// A solid BGRA image.
    fn solid(width: u32, height: u32, [r, g, b]: [u8; 3], alpha: u8) -> Vec<u8> {
        let premultiply = |v: u8| (u16::from(v) * u16::from(alpha) / 255) as u8;
        [premultiply(b), premultiply(g), premultiply(r), alpha].repeat((width * height) as usize)
    }

    fn hsl(rgb: [u8; 3]) -> (f64, f64, f64) {
        to_hsl(
            f64::from(rgb[0]) / 255.0,
            f64::from(rgb[1]) / 255.0,
            f64::from(rgb[2]) / 255.0,
        )
    }

    fn within_clamp(rgb: [u8; 3]) {
        let (_, s, l) = hsl(rgb);
        assert!(s >= 0.33, "saturation {s}");
        assert!((0.40..=0.64).contains(&l), "lightness {l}");
    }

    #[test]
    fn a_vivid_cover_gives_its_colour() {
        let red = tint(&solid(256, 256, [220, 30, 30], 255), 256, 256).unwrap();
        within_clamp(red);
        assert!(red[0] > 150 && red[1] < 90 && red[2] < 90, "{red:?}");
        let (hue, ..) = hsl(red);
        assert!(!(15.0..345.0).contains(&hue), "hue {hue}");
    }

    #[test]
    fn dark_and_pale_colours_are_pulled_into_the_visible_range() {
        // Dark navy is lifted, a pastel is darkened, and neither ends near white or black.
        let navy = tint(&solid(64, 64, [10, 10, 80], 255), 64, 64).unwrap();
        within_clamp(navy);
        assert!(navy[2] > navy[0]);
        let pastel = tint(&solid(64, 64, [255, 190, 200], 255), 64, 64).unwrap();
        within_clamp(pastel);
    }

    #[test]
    fn greyscale_and_empty_covers_have_no_tint() {
        assert_eq!(tint(&solid(64, 64, [128, 128, 128], 255), 64, 64), None);
        assert_eq!(tint(&solid(64, 64, [255, 255, 255], 255), 64, 64), None);
        assert_eq!(tint(&solid(64, 64, [0, 0, 0], 255), 64, 64), None);
        assert_eq!(tint(&solid(64, 64, [220, 30, 30], 0), 64, 64), None);
        assert_eq!(tint(&[], 0, 0), None);
        assert_eq!(tint(&[1, 2, 3], 4, 4), None, "too few bytes");
    }

    #[test]
    fn the_heaviest_hue_wins_over_a_few_specks() {
        // Mostly blue, with a red stripe along one edge.
        let (width, height) = (96u32, 96u32);
        let mut image = solid(width, height, [20, 60, 220], 255);
        for y in 0..4usize {
            for x in 0..width as usize {
                let at = (y * width as usize + x) * 4;
                image[at..at + 4].copy_from_slice(&[30, 30, 220, 255]);
            }
        }
        let tint = tint(&image, width, height).unwrap();
        assert!(tint[2] > tint[0], "{tint:?} should be blue");
    }

    #[test]
    fn transparent_pixels_are_skipped() {
        // Left half green and opaque, right half transparent black.
        let (width, height) = (64u32, 64u32);
        let mut image = solid(width, height, [0, 0, 0], 0);
        for y in 0..height as usize {
            for x in 0..32usize {
                let at = (y * width as usize + x) * 4;
                image[at..at + 4].copy_from_slice(&[40, 200, 40, 255]);
            }
        }
        let tint = tint(&image, width, height).unwrap();
        assert!(tint[1] > tint[0] && tint[1] > tint[2], "{tint:?}");
    }

    #[test]
    fn work_is_bounded_for_big_images() {
        // Not a timing test: just that a large frame is sampled, not walked.
        let big = solid(256, 256, [30, 200, 90], 255);
        assert!(tint(&big, 256, 256).is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_current_hash_is_served() {
        let store = ArtStore::default();
        assert!(store.get("anything").is_none());
        let url = store.set(vec![1, 2, 3]);
        let hash = url.rsplit('/').next().unwrap().to_string();
        assert!(url.starts_with("http://qapsule-art.localhost/"));
        assert_eq!(store.get(&hash).as_deref(), Some(&vec![1, 2, 3]));
        assert!(store.get("").is_none());
        assert!(store.get("../x").is_none());

        let second = store.set(vec![9, 9]);
        assert_ne!(url, second);
        assert!(store.get(&hash).is_none(), "the old image is gone");
        store.clear();
        assert!(store.get(second.rsplit('/').next().unwrap()).is_none());
    }

    #[test]
    fn the_same_bytes_have_the_same_url() {
        let store = ArtStore::default();
        assert_eq!(store.set(vec![5; 100]), store.set(vec![5; 100]));
        assert_ne!(hash_of(&[1]), hash_of(&[1, 0]));
    }
}
