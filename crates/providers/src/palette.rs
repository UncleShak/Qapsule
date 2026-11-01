//! The colour extractor behind the theme provider: pixels in, a safe palette out.
//! Pure and deterministic (a fixed seed, a fixed iteration cap), so the same wallpaper
//! always gives the same colours and the tests need no image files.
//!
//! 1. Box-average the image down to [`LONG_EDGE`] pixels on its long edge; skip
//!    transparent pixels.
//! 2. Convert to OKLab and run k-means with [`K`] clusters (seeded k-means++ start, at
//!    most [`MAX_ITERATIONS`] rounds).
//! 3. Score each cluster by area x chroma, times a penalty for near-black and near-white.
//!    Clusters under [`MIN_SHARE`] of the pixels or under [`MIN_CHROMA`] are ignored.
//! 4. The best cluster is the accent; the next one at least [`MIN_HUE_GAP`] degrees
//!    away in hue is `accent_2` (the accent again if there is none).
//! 5. Both accents are made at least [`MIN_ACCENT_CONTRAST`]:1 against the capsule
//!    surface. The tint is the second colour at low chroma. No usable cluster (a grey or
//!    black wallpaper): `None`, and the caller uses the Windows accent.

use crate::colour::{Rgb, ensure_contrast, from_oklch, to_oklch};

/// Pixels on the long edge after the downscale.
pub const LONG_EDGE: usize = 64;
/// Clusters.
pub const K: usize = 6;
/// Rounds of k-means, at most.
pub const MAX_ITERATIONS: usize = 16;
/// The generator seed of the k-means++ start.
pub const SEED: u64 = 0x5eed_c0de_0a11_ce55;
/// A cluster needs this OKLCH chroma to count as colour (greys are about 0).
pub const MIN_CHROMA: f64 = 0.04;
/// A cluster needs this share of the pixels, or it is a speck.
pub const MIN_SHARE: f64 = 0.02;
/// The least hue distance, in degrees, between the accent and the second colour.
pub const MIN_HUE_GAP: f64 = 30.0;
/// The accents' contrast against the capsule surface.
pub const MIN_ACCENT_CONTRAST: f64 = 4.5;
/// The most chroma an accent keeps: a wallpaper's neon is toned down.
pub const MAX_ACCENT_CHROMA: f64 = 0.20;
/// The tint's OKLCH lightness and its most chroma.
pub const TINT_LIGHTNESS: f64 = 0.55;
pub const TINT_CHROMA: f64 = 0.08;

/// The colours of a wallpaper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub accent: Rgb,
    pub accent_2: Rgb,
    pub tint: Rgb,
}

/// `colour` with chroma capped and lightness raised until it reads on the capsule.
pub fn safe(colour: Rgb) -> Rgb {
    let (l, c, h) = to_oklch(colour);
    let capped = from_oklch(l, c.min(MAX_ACCENT_CHROMA), h);
    ensure_contrast(capped, MIN_ACCENT_CONTRAST)
}

/// The palette of `rgba` (8 bits a channel, `width * height` pixels), or `None` if the
/// image has no usable colour.
pub fn extract(rgba: &[u8], width: usize, height: usize) -> Option<Palette> {
    let points = sample(rgba, width, height);
    if points.is_empty() {
        return None;
    }
    let clusters = cluster(&points);
    let total = points.len() as f64;

    let mut ranked: Vec<Scored> = clusters
        .iter()
        .filter_map(|(centre, count)| {
            let share = *count as f64 / total;
            let chroma = centre[1].hypot(centre[2]);
            (share >= MIN_SHARE && chroma >= MIN_CHROMA).then(|| Scored {
                score: share * chroma * extreme_penalty(centre[0]),
                lightness: centre[0],
                chroma,
                hue: centre[2].atan2(centre[1]).to_degrees().rem_euclid(360.0),
            })
        })
        .collect();
    // Stable sort: equal scores keep cluster order, which is deterministic.
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score));

    let first = ranked.first()?;
    let second = ranked
        .iter()
        .skip(1)
        .find(|c| hue_distance(c.hue, first.hue) >= MIN_HUE_GAP);
    let accent = safe(first.colour());
    let accent_2 = second.map_or(accent, |c| safe(c.colour()));
    let tint_from = second.unwrap_or(first);
    let tint = from_oklch(
        TINT_LIGHTNESS,
        tint_from.chroma.min(TINT_CHROMA),
        tint_from.hue,
    );
    Some(Palette {
        accent,
        accent_2,
        tint,
    })
}

struct Scored {
    score: f64,
    lightness: f64,
    chroma: f64,
    hue: f64,
}

impl Scored {
    fn colour(&self) -> Rgb {
        from_oklch(self.lightness, self.chroma, self.hue)
    }
}

/// Smaller for near-black and near-white clusters, which make poor accents.
fn extreme_penalty(lightness: f64) -> f64 {
    if lightness < 0.25 {
        (lightness / 0.25).clamp(0.0, 1.0).powi(2)
    } else if lightness > 0.90 {
        ((1.0 - lightness) / 0.10).clamp(0.0, 1.0).powi(2)
    } else {
        1.0
    }
}

pub fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).abs().rem_euclid(360.0);
    d.min(360.0 - d)
}

/// The downscaled image as OKLab points `[L, a, b]`.
fn sample(rgba: &[u8], width: usize, height: usize) -> Vec<[f64; 3]> {
    if width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return Vec::new();
    }
    let scale = (LONG_EDGE as f64 / width.max(height) as f64).min(1.0);
    let out_w = ((width as f64 * scale).round() as usize).max(1);
    let out_h = ((height as f64 * scale).round() as usize).max(1);
    let mut points = Vec::with_capacity(out_w * out_h);
    for oy in 0..out_h {
        let (y0, y1) = (
            oy * height / out_h,
            ((oy + 1) * height / out_h).max(oy * height / out_h + 1),
        );
        for ox in 0..out_w {
            let (x0, x1) = (
                ox * width / out_w,
                ((ox + 1) * width / out_w).max(ox * width / out_w + 1),
            );
            let (mut sum, mut n) = ([0u64; 3], 0u64);
            for y in y0..y1.min(height) {
                for x in x0..x1.min(width) {
                    let at = (y * width + x) * 4;
                    if rgba[at + 3] < 128 {
                        continue;
                    }
                    for (s, v) in sum.iter_mut().zip(&rgba[at..at + 3]) {
                        *s += u64::from(*v);
                    }
                    n += 1;
                }
            }
            if n == 0 {
                continue;
            }
            let avg = |i: usize| ((sum[i] + n / 2) / n) as u8;
            let (l, c, h) = to_oklch(Rgb(avg(0), avg(1), avg(2)));
            points.push([l, c * h.to_radians().cos(), c * h.to_radians().sin()]);
        }
    }
    points
}

fn distance_sq(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// xorshift64*: a small generator whose sequence is fixed by the seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// In `[0, 1)`.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// k-means: the centre and the size of each cluster.
fn cluster(points: &[[f64; 3]]) -> Vec<([f64; 3], usize)> {
    let k = K.min(points.len());
    let mut rng = Rng(SEED);
    let mut centres = vec![points[(rng.next() % points.len() as u64) as usize]];
    while centres.len() < k {
        let dist: Vec<f64> = points
            .iter()
            .map(|p| {
                centres
                    .iter()
                    .map(|c| distance_sq(p, c))
                    .fold(f64::MAX, f64::min)
            })
            .collect();
        let total: f64 = dist.iter().sum();
        if total <= 0.0 {
            break; // Every point is already a centre.
        }
        let mut target = rng.unit() * total;
        let mut pick = points.len() - 1;
        for (i, d) in dist.iter().enumerate() {
            if target < *d {
                pick = i;
                break;
            }
            target -= d;
        }
        centres.push(points[pick]);
    }

    let mut owner = vec![usize::MAX; points.len()];
    let mut counts = vec![0usize; centres.len()];
    for _ in 0..MAX_ITERATIONS {
        let mut moved = false;
        for (i, p) in points.iter().enumerate() {
            let nearest = (0..centres.len())
                .min_by(|a, b| {
                    distance_sq(p, &centres[*a]).total_cmp(&distance_sq(p, &centres[*b]))
                })
                .unwrap_or(0);
            if owner[i] != nearest {
                owner[i] = nearest;
                moved = true;
            }
        }
        if !moved {
            break;
        }
        let mut sums = vec![[0.0f64; 3]; centres.len()];
        counts.iter_mut().for_each(|c| *c = 0);
        for (p, o) in points.iter().zip(&owner) {
            counts[*o] += 1;
            for d in 0..3 {
                sums[*o][d] += p[d];
            }
        }
        for (i, centre) in centres.iter_mut().enumerate() {
            if counts[i] > 0 {
                *centre = sums[i].map(|v| v / counts[i] as f64);
            }
        }
    }
    // The counts of the final assignment (the loop may end right after moving points).
    counts.iter_mut().for_each(|c| *c = 0);
    for o in &owner {
        counts[*o] += 1;
    }
    centres.into_iter().zip(counts).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::SURFACE;
    use crate::colour::contrast;

    fn image(width: usize, height: usize, pixel: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
        let mut out = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let [r, g, b] = pixel(x, y);
                out.extend([r, g, b, 255]);
            }
        }
        out
    }

    fn hue(c: Rgb) -> f64 {
        to_oklch(c).2
    }

    #[test]
    fn the_same_image_gives_the_same_palette() {
        let (w, h) = (200, 120);
        let pixels = image(w, h, |x, y| {
            [
                ((x * 7 + y * 3) % 256) as u8,
                ((x * 2 + y * 11) % 256) as u8,
                ((x * y) % 256) as u8,
            ]
        });
        let a = extract(&pixels, w, h);
        assert_eq!(a, extract(&pixels, w, h));
        assert!(a.is_some());
    }

    #[test]
    fn every_accent_reaches_the_contrast_floor() {
        let cases: [[u8; 3]; 5] = [
            [200, 30, 40],
            [10, 20, 160],
            [20, 60, 10],
            [250, 250, 120],
            [90, 20, 110],
        ];
        for rgb in cases {
            let pixels = image(80, 80, |_, _| rgb);
            let palette = extract(&pixels, 80, 80).expect("a vivid image has a palette");
            assert!(
                contrast(palette.accent, SURFACE) >= MIN_ACCENT_CONTRAST,
                "{rgb:?}"
            );
            assert!(
                contrast(palette.accent_2, SURFACE) >= MIN_ACCENT_CONTRAST,
                "{rgb:?}"
            );
        }
        // And the safe() clamp alone, on the dimmest colours.
        for rgb in [Rgb(0, 0, 80), Rgb(40, 0, 0), Rgb(0, 40, 0)] {
            assert!(contrast(safe(rgb), SURFACE) >= MIN_ACCENT_CONTRAST);
        }
    }

    #[test]
    fn grey_black_white_and_empty_images_have_no_palette() {
        for rgb in [[0, 0, 0], [128, 128, 128], [255, 255, 255], [20, 20, 22]] {
            assert_eq!(extract(&image(64, 64, |_, _| rgb), 64, 64), None, "{rgb:?}");
        }
        assert_eq!(extract(&[], 0, 0), None);
        assert_eq!(extract(&[1, 2, 3], 10, 10), None);
        // Fully transparent.
        assert_eq!(extract(&[200, 10, 10, 0].repeat(100), 10, 10), None);
    }

    #[test]
    fn a_two_colour_image_gives_two_hues_far_apart() {
        let pixels = image(
            128,
            72,
            |x, _| if x < 64 { [210, 40, 40] } else { [30, 70, 210] },
        );
        let palette = extract(&pixels, 128, 72).unwrap();
        assert!(hue_distance(hue(palette.accent), hue(palette.accent_2)) >= MIN_HUE_GAP);
        assert_ne!(palette.accent, palette.accent_2);
    }

    #[test]
    fn near_duplicate_hues_leave_accent_2_equal_to_the_accent() {
        let pixels = image(
            128,
            72,
            |x, _| if x < 64 { [210, 60, 40] } else { [215, 70, 40] },
        );
        let palette = extract(&pixels, 128, 72).unwrap();
        assert_eq!(palette.accent, palette.accent_2);
    }

    #[test]
    fn a_speck_of_colour_on_grey_does_not_make_an_accent() {
        let pixels = image(100, 100, |x, y| {
            if x < 5 && y < 5 {
                [220, 20, 20]
            } else {
                [120, 120, 120]
            }
        });
        assert_eq!(extract(&pixels, 100, 100), None);
    }

    #[test]
    fn the_big_area_of_colour_beats_the_small_one() {
        let pixels = image(
            100,
            100,
            |x, _| if x < 80 { [30, 160, 60] } else { [220, 40, 40] },
        );
        let palette = extract(&pixels, 100, 100).unwrap();
        let green = to_oklch(Rgb(30, 160, 60)).2;
        assert!(hue_distance(hue(palette.accent), green) < 20.0);
    }

    #[test]
    fn the_tint_is_quiet() {
        let pixels = image(
            64,
            64,
            |x, _| if x < 32 { [210, 40, 40] } else { [30, 70, 210] },
        );
        let palette = extract(&pixels, 64, 64).unwrap();
        assert!(to_oklch(palette.tint).1 <= TINT_CHROMA + 0.01);
    }

    #[test]
    fn a_huge_image_is_sampled_not_walked_per_cluster() {
        // Only that a tall thin image is scaled on its long edge and still works.
        let pixels = image(10, 400, |_, y| {
            if y < 200 {
                [200, 30, 30]
            } else {
                [30, 30, 200]
            }
        });
        assert!(extract(&pixels, 10, 400).is_some());
    }
}
