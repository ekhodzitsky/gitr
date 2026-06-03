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
