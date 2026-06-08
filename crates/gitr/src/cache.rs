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

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl Cache {
    /// Create a new cache with a 60-second default TTL.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            default_ttl: Duration::from_secs(60),
        }
    }

    /// Create a new cache with the given default TTL.
    pub fn with_ttl(default_ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            default_ttl,
        }
    }

    /// Get the cached status output if it exists and has not expired.
    pub fn get_status(&self) -> Option<String> {
        self.get("status")
    }

    /// Cache raw status output.
    pub fn set_status(&self, value: String) {
        self.set("status".to_string(), value);
    }

    /// Invalidate the status cache entry.
    pub fn invalidate(&self) {
        self.invalidate_key("status");
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
    pub fn invalidate_key(&self, key: &str) {
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
        let cache = Cache::new();
        cache.set("key".to_string(), "value".to_string());
        assert_eq!(cache.get("key"), Some("value".to_string()));
    }

    #[test]
    fn test_cache_miss() {
        let cache = Cache::new();
        assert_eq!(cache.get("missing"), None);
    }

    #[test]
    fn test_cache_expiry() {
        let cache = Cache::with_ttl(Duration::from_millis(1));
        cache.set("key".to_string(), "value".to_string());
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cache.get("key"), None);
    }

    #[test]
    fn test_cache_invalidate() {
        let cache = Cache::new();
        cache.set("key".to_string(), "value".to_string());
        cache.invalidate_key("key");
        assert_eq!(cache.get("key"), None);
    }

    #[test]
    fn test_status_api() {
        let cache = Cache::new();
        assert_eq!(cache.get_status(), None);
        cache.set_status("M file.txt".to_string());
        assert_eq!(cache.get_status(), Some("M file.txt".to_string()));
        cache.invalidate();
        assert_eq!(cache.get_status(), None);
    }
}
