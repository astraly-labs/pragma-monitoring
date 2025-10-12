use crate::monitoring::metrics::MONITORING_METRICS;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct PairKey {
    network: String,
    pair: String,
    data_type: String,
}

impl PairKey {
    fn new(network: &str, pair: &str, data_type: &str) -> Self {
        Self {
            network: network.to_string(),
            pair: pair.to_string(),
            data_type: data_type.to_string(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct PublisherKey {
    network: String,
    publisher: String,
    data_type: String,
}

impl PublisherKey {
    fn new(network: &str, publisher: &str, data_type: &str) -> Self {
        Self {
            network: network.to_string(),
            publisher: publisher.to_string(),
            data_type: data_type.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct LastUpdateTracker {
    pair_updates: RwLock<HashMap<PairKey, u64>>,
    publisher_updates: RwLock<HashMap<PublisherKey, u64>>,
}

impl LastUpdateTracker {
    fn new() -> Self {
        Self {
            pair_updates: RwLock::new(HashMap::new()),
            publisher_updates: RwLock::new(HashMap::new()),
        }
    }

    pub async fn record_pair_update(
        &self,
        network: &str,
        pair: &str,
        data_type: &str,
        timestamp: u64,
    ) {
        let mut guard = self.pair_updates.write().await;
        guard.insert(PairKey::new(network, pair, data_type), timestamp);
    }

    pub async fn record_publisher_update(
        &self,
        network: &str,
        publisher: &str,
        data_type: &str,
        timestamp: u64,
    ) {
        let mut guard = self.publisher_updates.write().await;
        guard.insert(PublisherKey::new(network, publisher, data_type), timestamp);
    }

    pub async fn set_pair_updates(
        &self,
        network: &str,
        data_type: &str,
        entries: &[(String, u64)],
    ) {
        let mut guard = self.pair_updates.write().await;
        guard.retain(|key, _| !(key.network == network && key.data_type == data_type));
        for (pair, timestamp) in entries {
            guard.insert(PairKey::new(network, pair, data_type), *timestamp);
        }
    }

    pub async fn set_publisher_updates(
        &self,
        network: &str,
        data_type: &str,
        entries: &[(String, u64)],
    ) {
        let mut guard = self.publisher_updates.write().await;
        guard.retain(|key, _| !(key.network == network && key.data_type == data_type));
        for (publisher, timestamp) in entries {
            guard.insert(PublisherKey::new(network, publisher, data_type), *timestamp);
        }
    }

    pub async fn refresh_metrics(&self) {
        let now = Utc::now().timestamp();
        if now <= 0 {
            return;
        }
        let now = now as u64;

        {
            let guard = self.pair_updates.read().await;
            for (key, last) in guard.iter() {
                let elapsed = now.saturating_sub(*last) as f64;
                MONITORING_METRICS
                    .monitoring_metrics
                    .set_time_since_last_update_pair_id(
                        elapsed,
                        &key.network,
                        &key.pair,
                        &key.data_type,
                    );
            }
        }

        {
            let guard = self.publisher_updates.read().await;
            for (key, last) in guard.iter() {
                let elapsed = now.saturating_sub(*last) as f64;
                MONITORING_METRICS
                    .monitoring_metrics
                    .set_time_since_last_update_publisher(
                        elapsed,
                        &key.network,
                        &key.publisher,
                        &key.data_type,
                    );
            }
        }
    }
}

pub static LAST_UPDATE_TRACKER: LazyLock<Arc<LastUpdateTracker>> =
    LazyLock::new(|| Arc::new(LastUpdateTracker::new()));

pub fn spawn_refresh_task(interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        LAST_UPDATE_TRACKER.refresh_metrics().await;
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            LAST_UPDATE_TRACKER.refresh_metrics().await;
        }
    })
}
