use lazy_static::lazy_static;
use opentelemetry::{
    metrics::ObservableGauge,
    KeyValue,
};
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
    // Gauges converted to UpDownCounters
    pub time_since_last_update_publisher: ObservableGauge<f64>,
    pub pair_price: ObservableGauge<f64>,
    pub time_since_last_update_pair_id: ObservableGauge<f64>,
    pub price_deviation: ObservableGauge<f64>,
    pub price_deviation_source: ObservableGauge<f64>,
    pub num_sources: ObservableGauge<i64>,
    pub indexer_blocks_left: ObservableGauge<i64>,
    pub publisher_balance: ObservableGauge<f64>,
    pub api_price_deviation: ObservableGauge<f64>,
    pub on_off_price_deviation: ObservableGauge<f64>,
    pub api_time_since_last_update: ObservableGauge<f64>,
    pub api_num_sources: ObservableGauge<i64>,
    pub api_sequencer_deviation: ObservableGauge<f64>,
}

impl MonitoringMetricsRegistry {
    pub fn new() -> Arc<Self> {
        let meter = opentelemetry::global::meter("pragma-monitoring");

        // Initialize UpDownCounters (converted from Prometheus gauges)
        let time_since_last_update_publisher = meter
            .f64_observable_gauge("time_since_last_update_seconds")
            .with_description("Time since the last update in seconds")
            .init();

        let pair_price = meter
            .f64_observable_gauge("pair_price")
            .with_description("Price of the pair from the source")
            .init();

        let time_since_last_update_pair_id = meter
            .f64_observable_gauge("time_since_last_update_pair_id")
            .with_description("Time since the last update in seconds for pair")
            .init();

        let price_deviation = meter
            .f64_observable_gauge("price_deviation")
            .with_description(
                "Price deviation for a source compared to a reference price (DefiLlama)",
            )
            .init();

        let price_deviation_source = meter
            .f64_observable_gauge("price_deviation_source")
            .with_description("Price deviation for a source compared to our oracle price")
            .init();

        let num_sources = meter
            .i64_observable_gauge("num_sources")
            .with_description("Number of sources that have published data for a pair")
            .init();

        let indexer_blocks_left = meter
            .i64_observable_gauge("indexer_blocks_left")
            .with_description("Number of blocks left to index for a given indexer")
            .init();

        let publisher_balance = meter
            .f64_observable_gauge("publisher_balance")
            .with_description("Balance of the publisher in ETH")
            .init();

        let api_price_deviation = meter
            .f64_observable_gauge("api_price_deviation")
            .with_description(
                "Price deviation for our API compared to a reference price (DefiLlama)",
            )
            .init();

        let on_off_price_deviation = meter
            .f64_observable_gauge("on_off_price_deviation")
            .with_description(
                "Median on chain price deviation compared to a reference price (Defillama)",
            )
            .init();

        let api_time_since_last_update = meter
            .f64_observable_gauge("api_time_since_last_update")
            .with_description("Time since the last update in seconds")
            .init();

        let api_num_sources = meter
            .i64_observable_gauge("api_num_sources")
            .with_description("Number of sources aggregated for a pair")
            .init();

        let api_sequencer_deviation = meter
            .f64_observable_gauge("api_sequencer_deviation")
            .with_description("Price deviation from starknet gateway price")
            .init();

        Arc::new(Self {
            // Metrics (former gauges)
            time_since_last_update_publisher,
            pair_price,
            time_since_last_update_pair_id,
            price_deviation,
            price_deviation_source,
            num_sources,
            indexer_blocks_left,
            publisher_balance,
            api_price_deviation,
            on_off_price_deviation,
            api_time_since_last_update,
            api_num_sources,
            api_sequencer_deviation,
        })
    }

    pub fn set_time_since_last_update_publisher(
        &self,
        value: f64,
        network: &str,
        publisher: &str,
        type_: &str,
    ) {
        self.time_since_last_update_publisher.observe(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("publisher", publisher.to_string()),
                KeyValue::new("type", type_.to_string()),
            ],
        );
    }

    pub fn set_pair_price(&self, value: f64, network: &str, pair: &str, source: &str, type_: &str) {
        self.pair_price.observe(
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
        self.time_since_last_update_pair_id.observe(
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
        self.price_deviation.observe(
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
        self.price_deviation_source.observe(
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
        self.num_sources.observe(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
                KeyValue::new("type", type_.to_string()),
            ],
        );
    }

    pub fn set_indexer_blocks_left(&self, value: i64, network: &str, type_: &str) {
        self.indexer_blocks_left.observe(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("type", type_.to_string()),
            ],
        );
    }

    pub fn set_publisher_balance(&self, value: f64, network: &str, publisher: &str) {
        self.publisher_balance.observe(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("publisher", publisher.to_string()),
            ],
        );
    }

    pub fn set_api_price_deviation(&self, value: f64, network: &str, pair: &str) {
        self.api_price_deviation.observe(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
            ],
        );
    }

    pub fn set_on_off_price_deviation(&self, value: f64, network: &str, pair: &str, type_: &str) {
        self.on_off_price_deviation.observe(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
                KeyValue::new("type", type_.to_string()),
            ],
        );
    }

    pub fn set_api_time_since_last_update(&self, value: f64, network: &str, pair: &str) {
        self.api_time_since_last_update.observe(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
            ],
        );
    }

    pub fn set_api_num_sources(&self, value: i64, network: &str, pair: &str) {
        self.api_num_sources.observe(
            value,
            &[
                KeyValue::new("network", network.to_string()),
                KeyValue::new("pair", pair.to_string()),
            ],
        );
    }

    pub fn set_api_sequencer_deviation(&self, value: f64, network: &str) {
        self.api_sequencer_deviation
            .observe(value, &[KeyValue::new("network", network.to_string())]);
    }
}
