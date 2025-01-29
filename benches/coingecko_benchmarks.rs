use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pragma_monitoring::coingecko::get_coingecko_mappings;
use pragma_monitoring::constants::COINGECKO_IDS;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
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

    // Configure longer sampling time for rate limited operations
    group.measurement_time(Duration::from_secs(20));
    group.sample_size(10);

    // Benchmark simple lookup from ArcSwap
    group.bench_function("arcswap_lookup", |b| {
        b.iter(|| {
            let mappings = COINGECKO_IDS.load();
            black_box(mappings.get("BTC/USD").cloned())
        });
    });

    // Benchmark concurrent lookups from ArcSwap
    group.bench_function("concurrent_arcswap_lookups", |b| {
        b.iter(|| {
            rt.block_on(async {
                let handles: Vec<_> = (0..3)
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

    // Benchmark rate-limited API calls
    group.bench_function("rate_limited_api_calls", |b| {
        b.iter(|| rt.block_on(async { black_box(get_coingecko_mappings().await) }));
    });

    // Benchmark cached lookups
    group.bench_function("cached_lookups", |b| {
        b.iter(|| {
            rt.block_on(async {
                // First call will cache, subsequent calls will use cache
                let _ = get_coingecko_mappings().await;
                black_box(get_coingecko_mappings().await)
            })
        });
    });

    // Benchmark concurrent rate-limited API calls
    group.bench_function("concurrent_rate_limited_calls", |b| {
        b.iter(|| {
            rt.block_on(async {
                let handles: Vec<_> = (0..3)
                    .map(|_| tokio::spawn(async { black_box(get_coingecko_mappings().await) }))
                    .collect();

                futures::future::join_all(handles).await
            });
        });
    });

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
