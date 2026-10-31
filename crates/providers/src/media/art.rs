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
