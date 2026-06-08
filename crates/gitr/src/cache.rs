use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A single cached value with an expiration time.
#[derive(Clone, Debug)]
struct CacheEntry<T> {
    value: T,
    expires_at: Instant,
}

/// Simple in-memory TTL cache for expensive git operations.
#[derive(Clone, Debug)]
pub struct Cache {
    inner: Arc<Mutex<HashMap<String, CacheEntry<String>>>>,
    default_ttl: Duration,
}

impl Cache {
    /// Create a new cache with the given default TTL.
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            default_ttl,
        }
    }

    /// Get a cached value if it exists and has not expired.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn get(&self, key: &str) -> Option<String> {
        let mut map = self.inner.lock().unwrap();
        if let Some(entry) = map.get(key) {
            if Instant::now() < entry.expires_at {
                return Some(entry.value.clone());
            }
            map.remove(key);
        }
        None
    }

    /// Insert a value into the cache with the default TTL.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn set(&self, key: String, value: String) {
        let mut map = self.inner.lock().unwrap();
        map.insert(
            key,
            CacheEntry {
                value,
                expires_at: Instant::now() + self.default_ttl,
            },
        );
    }

    /// Remove a single key from the cache.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn invalidate(&self, key: &str) {
        let mut map = self.inner.lock().unwrap();
        map.remove(key);
    }

    /// Clear all cached entries.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn clear(&self) {
        let mut map = self.inner.lock().unwrap();
        map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit() {
        let cache = Cache::new(Duration::from_secs(60));
        cache.set("key".to_string(), "value".to_string());
        assert_eq!(cache.get("key"), Some("value".to_string()));
    }

    #[test]
    fn test_cache_miss() {
        let cache = Cache::new(Duration::from_secs(60));
        assert_eq!(cache.get("missing"), None);
    }

    #[test]
    fn test_cache_expiry() {
        let cache = Cache::new(Duration::from_millis(1));
        cache.set("key".to_string(), "value".to_string());
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cache.get("key"), None);
    }

    #[test]
    fn test_cache_invalidate() {
        let cache = Cache::new(Duration::from_secs(60));
        cache.set("key".to_string(), "value".to_string());
        cache.invalidate("key");
        assert_eq!(cache.get("key"), None);
    }
}
