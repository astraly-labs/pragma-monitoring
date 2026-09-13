use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InternalIndexerStatus {
    pub is_running: bool,
    pub is_synced: bool,
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
        if !running {
            status.is_synced = false;
        }
        if running {
            status.last_activity = Some(Instant::now());
        }
    }

    pub async fn set_synced(&self, synced: bool) {
        let mut status = self.status.write().await;
        status.is_synced = synced;
        status.last_activity = Some(Instant::now());
    }

    pub async fn is_synced(&self) -> bool {
        self.status.read().await.is_synced
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

    pub async fn handle_reorg(&self, invalidated_block: u64) {
        let mut status = self.status.write().await;

        // Update the last processed block to be one less than the invalidated block
        // This represents the last valid block after the reorg
        if let Some(current_last_block) = status.last_processed_block
            && current_last_block >= invalidated_block
        {
            status.last_processed_block = Some(invalidated_block.saturating_sub(1));
            tracing::warn!(
                "🔄 [REORG] Rolled back: block {} → {}",
                current_last_block,
                status.last_processed_block.unwrap()
            );
        }

        status.is_synced = false;
        status.last_activity = Some(Instant::now());
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

        // Activity follows oracle events, which may be 30 minutes apart.
        // Match the 40-minute feed freshness allowance before declaring a stall.
        if let Some(last_activity) = status.last_activity {
            if last_activity.elapsed() > Duration::from_secs(40 * 60) {
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
pub static INTERNAL_INDEXER_TRACKER: LazyLock<InternalIndexerTracker> =
    LazyLock::new(InternalIndexerTracker::new);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_allows_the_publisher_heartbeat_but_detects_stalls() {
        let tracker = InternalIndexerTracker::new();
        tracker.set_running(true).await;
        tracker.status.write().await.last_activity =
            Some(Instant::now() - Duration::from_secs(31 * 60));
        assert!(tracker.is_healthy().await);

        tracker.status.write().await.last_activity =
            Some(Instant::now() - Duration::from_secs(41 * 60));
        assert!(!tracker.is_healthy().await);
    }

    #[tokio::test]
    async fn recent_activity_does_not_hide_a_stopped_or_failing_indexer() {
        let tracker = InternalIndexerTracker::new();
        tracker.set_running(true).await;
        tracker.set_running(false).await;
        assert!(!tracker.is_healthy().await);

        tracker.set_running(true).await;
        tracker.status.write().await.error_count = 11;
        assert!(!tracker.is_healthy().await);
    }
}
