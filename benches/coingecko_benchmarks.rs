use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pragma_monitoring::constants::{initialize_coingecko_mappings, COINGECKO_IDS};
use tokio::runtime::Runtime;

fn criterion_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Benchmark initialization
    c.bench_function("initialize_coingecko_mappings", |b| {
        b.iter(|| {
            rt.block_on(async {
                initialize_coingecko_mappings().await;
            });
        });
    });

    // Benchmark lookups
    c.bench_function("coingecko_lookup", |b| {
        rt.block_on(async {
            initialize_coingecko_mappings().await;
        });

        b.iter(|| {
            let mappings = COINGECKO_IDS.load();
            black_box(mappings.get("BTC/USD"));
        });
    });

    // Benchmark concurrent access
    c.bench_function("concurrent_access", |b| {
        rt.block_on(async {
            initialize_coingecko_mappings().await;
        });

        b.iter(|| {
            rt.block_on(async {
                let handles: Vec<_> = (0..10)
                    .map(|_| {
                        tokio::spawn(async {
                            let mappings = COINGECKO_IDS.load();
                            black_box(mappings.get("BTC/USD"));
                        })
                    })
                    .collect();

                futures::future::join_all(handles).await;
            });
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
