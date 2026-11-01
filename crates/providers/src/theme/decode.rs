//! Reads an image file into RGBA pixels. Called only on the theme thread.

use std::path::Path;

use image::ImageDecoder;

/// Files bigger than this are not read.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// The most pixels on a side.
pub const MAX_SIDE: u32 = 16384;
/// The most memory the decoder may allocate.
pub const MAX_ALLOC: u64 = 256 * 1024 * 1024;

/// RGBA pixels, width and height. The first frame of an animation. Errors say why in a
/// few words (never a path).
pub fn load(path: &Path) -> Result<(Vec<u8>, usize, usize), String> {
    let size = std::fs::metadata(path)
        .map_err(|_| "the image can't be read".to_string())?
        .len();
    if size > MAX_FILE_BYTES {
        return Err("the image file is too big".into());
    }
    let mut reader = image::ImageReader::open(path)
        .map_err(|_| "the image can't be opened".to_string())?
        .with_guessed_format()
        .map_err(|_| "the image can't be read".to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    // The decoder reads the header only; its declared size is checked against the limits
    // here, before any pixel buffer is allocated.
    let decoder = reader
        .into_decoder()
        .map_err(|e| format!("the image can't be decoded ({e})"))?;
    let (width, height) = decoder.dimensions();
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err("the image is too large".into());
    }
    let image = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("the image can't be decoded ({e})"))?;
    let rgba = image.into_rgba8();
    let (width, height) = (rgba.width() as usize, rgba.height() as usize);
    Ok((rgba.into_raw(), width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_png_without_an_extension_is_recognised_by_its_bytes() {
        let dir = std::env::temp_dir().join(format!("qapsule-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("TranscodedWallpaper");
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(4, 2, image::Rgba([200, 10, 10, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        std::fs::write(&path, &png).unwrap();
        let (pixels, w, h) = load(&path).unwrap();
        assert_eq!((w, h, pixels.len()), (4, 2, 32));
        assert_eq!(&pixels[..4], &[200, 10, 10, 255]);
        std::fs::write(&path, b"not an image").unwrap();
        assert!(load(&path).is_err());
        assert!(load(&dir.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    #[test]
    fn a_png_that_declares_a_huge_size_is_refused_before_decoding() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        // IHDR data starts at byte 16: width, then height. Declare 100000 x 100000 and
        // fix the chunk's checksum so the file is otherwise valid.
        png[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        png[20..24].copy_from_slice(&100_000u32.to_be_bytes());
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());
        let dir = std::env::temp_dir().join(format!("qapsule-huge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("huge.png");
        std::fs::write(&path, &png).unwrap();
        let started = std::time::Instant::now();
        let error = load(&path).unwrap_err();
        assert!(
            error.contains("limit") || error.contains("large"),
            "{error}"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(dir);
    }
}
