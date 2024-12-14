use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pragma_monitoring::constants::COINGECKO_IDS;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;

// Create a mock initialization function instead of calling the API
async fn initialize_test_mappings() {
    let mut test_mappings = HashMap::new();
    // Add just a few test pairs
    test_mappings.insert("BTC/USD".to_string(), "bitcoin".to_string());
    test_mappings.insert("ETH/USD".to_string(), "ethereum".to_string());
    test_mappings.insert("SOL/USD".to_string(), "solana".to_string());

    COINGECKO_IDS.store(Arc::new(test_mappings));
}

fn criterion_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Initialize with test data
    rt.block_on(async {
        initialize_test_mappings().await;
    });

    let mut group = c.benchmark_group("coingecko_operations");

    // Benchmark simple lookup
    group.bench_function("single_lookup", |b| {
        b.iter(|| {
            let mappings = COINGECKO_IDS.load();
            black_box(mappings.get("BTC/USD").cloned())
        });
    });

    // Benchmark concurrent lookups with smaller load
    group.bench_function("concurrent_lookups", |b| {
        b.iter(|| {
            rt.block_on(async {
                let handles: Vec<_> = (0..3) // Reduced to just 3 concurrent lookups
                    .map(|_| {
                        tokio::spawn(async {
                            let mappings = COINGECKO_IDS.load();
                            black_box(mappings.get("BTC/USD").cloned())
                        })
                    })
                    .collect();

                futures::future::join_all(handles).await
            });
        });
    });

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
