use lazy_static::lazy_static;
use opentelemetry::{KeyValue, metrics::Gauge};
use std::sync::Arc;

lazy_static! {
    pub static ref MONITORING_METRICS: Arc<MetricsRegistry> = MetricsRegistry::new();
}

#[derive(Debug)]
pub struct MetricsRegistry {
    pub monitoring_metrics: MonitoringMetricsRegistry,
}

impl MetricsRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            monitoring_metrics: Arc::try_unwrap(MonitoringMetricsRegistry::new())
                .unwrap_or_else(|arc| (*arc).clone()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct MonitoringMetricsRegistry {
    // Synchronous Gauges for real-time metrics
    pub time_since_last_update_publisher: Gauge<f64>,
    pub pair_price: Gauge<f64>,
    pub time_since_last_update_pair_id: Gauge<f64>,
    pub price_deviation: Gauge<f64>,
    pub price_deviation_source: Gauge<f64>,
    pub num_sources: Gauge<i64>,
    pub publisher_balance: Gauge<f64>,
    pub on_off_price_deviation: Gauge<f64>,
    pub indexed_events_count: Gauge<i64>,
    pub latest_indexed_block: Gauge<i64>,
}

impl MonitoringMetricsRegistry {
    pub fn new() -> Arc<Self> {
        let meter = opentelemetry::global::meter("pragma-monitoring");

        tracing::info!("📊 [METRICS] Initializing OTEL metrics...");

        // === Time-based metrics ===
        let time_since_last_update_publisher = meter
            .f64_gauge("pragma_publisher_last_update_seconds")
            .with_description("Seconds since publisher last submitted data")
            .with_unit("s")
            .init();

        let time_since_last_update_pair_id = meter
            .f64_gauge("pragma_pair_last_update_seconds")
            .with_description("Seconds since pair was last updated")
            .with_unit("s")
            .init();

        // === Price metrics ===
        let pair_price = meter
            .f64_gauge("pragma_pair_price")
            .with_description("Current price from source (normalized to decimals)")
            .with_unit("USD")
            .init();

        // === Deviation metrics (percentage) ===
        let price_deviation = meter
            .f64_gauge("pragma_deviation_vs_defillama")
            .with_description("Price deviation vs DefiLlama reference (ratio: 0.01 = 1%)")
            .with_unit("ratio")
            .init();

        let price_deviation_source = meter
            .f64_gauge("pragma_deviation_vs_median")
            .with_description("Source price deviation vs on-chain median (ratio: 0.01 = 1%)")
            .with_unit("ratio")
            .init();

        let on_off_price_deviation = meter
            .f64_gauge("pragma_deviation_onchain_vs_offchain")
            .with_description("On-chain median vs off-chain reference deviation (ratio)")
            .with_unit("ratio")
            .init();

        // === Aggregation metrics ===
        let num_sources = meter
            .i64_gauge("pragma_pair_num_sources")
            .with_description("Number of sources aggregated for pair median")
            .init();

        // === Publisher metrics ===
        let publisher_balance = meter
            .f64_gauge("pragma_publisher_balance_strk")
            .with_description("Publisher STRK balance for gas")
            .with_unit("STRK")
            .init();

        // === Indexer metrics ===
        let indexed_events_count = meter
            .i64_gauge("pragma_indexer_events_count")
            .with_description("Total events indexed in current session")
            .init();

        let latest_indexed_block = meter
            .i64_gauge("pragma_indexer_latest_block")
            .with_description("Most recent block number indexed")
            .init();

        tracing::info!("✅ [METRICS] 10 metrics registered");

        Arc::new(Self {
            time_since_last_update_publisher,
            pair_price,
            time_since_last_update_pair_id,
            price_deviation,
            price_deviation_source,
            num_sources,
            publisher_balance,
            on_off_price_deviation,
            indexed_events_count,
            latest_indexed_block,
        })
    }

    pub fn set_time_since_last_update_publisher(
        &self,
        value: f64,
        network: &str,
        publisher: &str,
        type_: &str,
    ) {
        self.time_since_last_update_publisher.record(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("publisher", publisher.to_string()),
                KeyValue::new("type", type_.to_string()),
            ],
        );
    }

    pub fn set_pair_price(&self, value: f64, network: &str, pair: &str, source: &str, type_: &str) {
        self.pair_price.record(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
                KeyValue::new("source", source.to_string()),
                KeyValue::new("type", type_.to_string()),
            ],
        );
    }

    pub fn set_time_since_last_update_pair_id(
        &self,
        value: f64,
        network: &str,
        pair: &str,
        type_: &str,
    ) {
        self.time_since_last_update_pair_id.record(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
                KeyValue::new("type", type_.to_string()),
            ],
        );
    }

    pub fn set_price_deviation(
        &self,
        value: f64,
        network: &str,
        pair: &str,
        source: &str,
        type_: &str,
    ) {
        self.price_deviation.record(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
                KeyValue::new("source", source.to_string()),
                KeyValue::new("type", type_.to_string()),
            ],
        );
    }

    pub fn set_price_deviation_source(
        &self,
        value: f64,
        network: &str,
        pair: &str,
        source: &str,
        type_: &str,
    ) {
        self.price_deviation_source.record(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
                KeyValue::new("source", source.to_string()),
                KeyValue::new("type", type_.to_string()),
            ],
        );
    }

    pub fn set_num_sources(&self, value: i64, network: &str, pair: &str, type_: &str) {
        self.num_sources.record(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
                KeyValue::new("type", type_.to_string()),
            ],
        );
    }

    pub fn set_publisher_balance(&self, value: f64, network: &str, publisher: &str) {
        self.publisher_balance.record(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("publisher", publisher.to_string()),
            ],
        );
    }

    pub fn set_on_off_price_deviation(&self, value: f64, network: &str, pair: &str, type_: &str) {
        self.on_off_price_deviation.record(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
                KeyValue::new("type", type_.to_string()),
            ],
        );
    }

    pub fn set_indexed_events_count(
        &self,
        value: i64,
        network: &str,
        pair: &str,
        event_type: &str,
    ) {
        self.indexed_events_count.record(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
                KeyValue::new("event_type", event_type.to_string()),
            ],
        );
    }

    pub fn set_latest_indexed_block(&self, value: u64, network: &str) {
        self.latest_indexed_block.record(
            value as i64,
            &[KeyValue::new("network", network.to_string())],
        );
    }
}
