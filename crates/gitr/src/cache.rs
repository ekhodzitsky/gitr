use crate::types::GitStatus;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Simple in-memory cache for frequently accessed git state.
///
/// The cache is scoped to a single `Repository` handle and is *not*
/// synchronized across clones. Call [`Cache::invalidate`] after any
/// mutating operation to keep subsequent reads consistent.
#[derive(Debug, Clone, Default)]
pub struct Cache {
    status: Arc<RwLock<Option<GitStatus>>>,
}

impl Cache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get cached status if available.
    pub async fn get_status(&self) -> Option<GitStatus> {
        self.status.read().await.clone()
    }

    /// Store status in the cache.
    pub async fn set_status(&self, status: GitStatus) {
        *self.status.write().await = Some(status);
    }

    /// Clear all cached entries.
    pub async fn invalidate(&self) {
        *self.status.write().await = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_new_empty() {
        let cache = Cache::new();
        assert!(cache.get_status().await.is_none());
    }

    #[tokio::test]
    async fn test_cache_set_and_get() {
        let cache = Cache::new();
        let status = GitStatus {
            staged: vec!["a.txt".to_string()],
            unstaged: vec![],
            untracked: vec![],
        };
        cache.set_status(status.clone()).await;
        let got = cache.get_status().await.unwrap();
        assert_eq!(got.staged, status.staged);
        assert!(got.unstaged.is_empty());
        assert!(got.untracked.is_empty());
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let cache = Cache::new();
        let status = GitStatus {
            staged: vec!["a.txt".to_string()],
            unstaged: vec![],
            untracked: vec![],
        };
        cache.set_status(status).await;
        cache.invalidate().await;
        assert!(cache.get_status().await.is_none());
    }

    #[tokio::test]
    async fn test_cache_clone_shares_state() {
        let cache = Cache::new();
        let status = GitStatus {
            staged: vec!["b.txt".to_string()],
            unstaged: vec![],
            untracked: vec![],
        };
        cache.set_status(status.clone()).await;
        let cache2 = cache.clone();
        assert_eq!(cache2.get_status().await.unwrap().staged, status.staged);
        cache2.invalidate().await;
        assert!(cache.get_status().await.is_none());
    }
}
