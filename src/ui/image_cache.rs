use crate::cache_db::LmdbCache;
use sha2::{Digest, Sha256};

// Hashes the URL to create a stable key for the DB.
fn url_to_key(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

pub fn load_from_lmdb(cache_db: &LmdbCache, url: &str) -> Option<Vec<u8>> {
    let key = url_to_key(url);
    match cache_db.read_image_cache(&key) {
        Ok(Some(data)) => Some(data),
        Ok(None) => None,
        Err(_e) => {
            // It's a cache, so we don't need to be too loud about errors.
            // eprintln!("Failed to read image from LMDB cache: {}", e);
            None
        }
    }
}

pub fn save_to_lmdb(cache_db: &LmdbCache, url: &str, data: &[u8]) {
    let key = url_to_key(url);
    if let Err(e) = cache_db.write_image_cache(&key, data) {
        eprintln!("Failed to write image to LMDB cache: {}", e);
    }
}
