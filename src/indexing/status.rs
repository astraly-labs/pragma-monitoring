use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InternalIndexerStatus {
    pub is_running: bool,
    pub last_processed_block: Option<u64>,
    pub events_processed: u64,
    #[serde(skip)]
    pub last_activity: Option<Instant>,
    pub error_count: u32,
    pub last_error: Option<String>,
}

/// Global status tracker for the internal indexer
pub struct InternalIndexerTracker {
    status: Arc<RwLock<InternalIndexerStatus>>,
}

impl InternalIndexerTracker {
    pub fn new() -> Self {
        Self {
            status: Arc::new(RwLock::new(InternalIndexerStatus::default())),
        }
    }
}

impl Default for InternalIndexerTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl InternalIndexerTracker {
    pub async fn set_running(&self, running: bool) {
        let mut status = self.status.write().await;
        status.is_running = running;
        if running {
            status.last_activity = Some(Instant::now());
        }
    }

    pub async fn update_processed_block(&self, block_number: u64) {
        let mut status = self.status.write().await;
        status.last_processed_block = Some(block_number);
        status.last_activity = Some(Instant::now());
    }

    pub async fn increment_events_processed(&self, count: u64) {
        let mut status = self.status.write().await;
        status.events_processed += count;
        status.last_activity = Some(Instant::now());
    }

    pub async fn record_error(&self, error: String) {
        let mut status = self.status.write().await;
        status.error_count += 1;
        status.last_error = Some(error);
    }

    pub async fn get_status(&self) -> InternalIndexerStatus {
        self.status.read().await.clone()
    }

    pub async fn is_healthy(&self) -> bool {
        let status = self.status.read().await;

        // Check if indexer is running
        if !status.is_running {
            return false;
        }

        // Check if we've had recent activity (within last 5 minutes)
        if let Some(last_activity) = status.last_activity {
            if last_activity.elapsed() > Duration::from_secs(300) {
                return false;
            }
        } else {
            return false;
        }

        // Check if we have too many errors
        if status.error_count > 10 {
            return false;
        }

        true
    }
}

// Global instance
lazy_static::lazy_static! {
    pub static ref INTERNAL_INDEXER_TRACKER: InternalIndexerTracker = InternalIndexerTracker::new();
}
