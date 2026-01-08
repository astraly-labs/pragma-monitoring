# Pragma Monitoring

## Architecture

```
┌─────────────────┐     ┌──────────────┐     ┌─────────────┐     ┌─────────────┐
│  Starknet       │────▶│   Apibara    │────▶│ TimescaleDB │────▶│    OTEL     │
│  (Oracle)       │     │  (Indexer)   │     │    (DB)     │     │  (Metrics)  │
└─────────────────┘     └──────────────┘     └─────────────┘     └─────────────┘
        │                                                               │
        │                                                               ▼
        └─────────────────────────────────────────────────────────▶ Grafana
                              (RPC calls for live data)
```

**Flow:**
1. **Apibara Indexer** streams oracle events from Starknet
2. Events are stored in **TimescaleDB** for persistence with time-series optimizations
3. **Metrics** are computed on-the-fly and exported to OTEL
4. **Grafana** visualizes metrics and logs

## OTEL Metrics

| Metric | Labels | Unit | Description |
|--------|--------|------|-------------|
| `pragma.pair.price` | network, pair, source, type | USD | Current price from source |
| `pragma.pair.last_update_seconds` | network, pair, type | s | Seconds since pair updated |
| `pragma.pair.num_sources` | network, pair, type | - | Sources aggregated for median |
| `pragma.publisher.last_update_seconds` | network, publisher, type | s | Seconds since publisher submitted |
| `pragma.publisher.balance_eth` | network, publisher | ETH | Publisher ETH balance |
| `pragma.deviation.vs_defillama` | network, pair, source, type | ratio | Deviation vs DefiLlama (0.01 = 1%) |
| `pragma.deviation.vs_median` | network, pair, source, type | ratio | Source vs on-chain median |
| `pragma.deviation.onchain_vs_offchain` | network, pair, type | ratio | On-chain vs off-chain reference |
| `pragma.indexer.events_count` | network, pair, event_type | - | Total events indexed |
| `pragma.indexer.latest_block` | network | - | Latest block indexed |

## Shared Public Access

Monitoring is not publicicly available yet but databases will soon be in read-only mode.

## Local Development

Quick start for local development:

```bash
# 1. Clone pragma-node next to this repo (for migrations)
git clone https://github.com/astraly-labs/pragma-node ../pragma-node

# 2. Copy and configure environment
cp .env.example .env
# Edit .env with your APIBARA_API_KEY and RPC_URL

# 3. Start services (TimescaleDB + OTEL/Grafana)
./scripts/dev.sh

# 4. Run the monitoring service
cargo run
```

**Available Services:**
- TimescaleDB: `localhost:5432`
- Grafana: `http://localhost:3000` (admin/admin)
- OTLP gRPC: `localhost:4317`
- OTLP HTTP: `localhost:4318`

**View logs in Grafana:**
1. Open http://localhost:3000
2. Go to Explore > Select 'Loki' data source
3. Query: `{service_name="pragma-monitoring"}`

## Self-Hosting

We have created a `docker-compose.yml` file to help with self-hosting setup:

```bash
docker compose up -d
```

Make sure to first fill the environment file with your own config parameters:

```bash
# Database URL (required)
DATABASE_URL='postgres://postgres:postgres@localhost:5432/pragma_monitoring'

# The OTEL endpoint to send metrics to
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317

# Starknet RPC URL (required)
RPC_URL=https://starknet-mainnet.public.blastapi.io

# Apibara API Key (required for indexing)
APIBARA_API_KEY=your_api_key_here

# Network configuration
NETWORK=mainnet
ORACLE_ADDRESS=0x02a85bd616f912537c50a49a4076db02c00b29b2cdc8a197ce92ed1837fa875b

# Pairs to monitor
SPOT_PAIRS=BTC/USD,ETH/USD
FUTURE_PAIRS=BTC/USD,ETH/USD

# Optional: Sources/publishers to ignore
IGNORE_SOURCES=BITSTAMP,DEFILLAMA
IGNORE_PUBLISHERS=

# Optional: Replication delay (ms)
REPLICATION_DELAY_MS=0

# Optional: DefiLlama Pro API key for better rate limits
DEFILLAMA_API_KEY=
```

### Notes

- Metrics are driven directly from indexed events; no scheduled database polling is required.
- “Time since last update” gauges are recomputed every 30 seconds so alerts can trigger even when no new events are received.
- DefiLlama quotes are cached for 30 seconds to avoid rate limits while keeping recent pricing information.

Database migrations are loaded from the [pragma-node](https://github.com/astraly-labs/pragma-node) repository (`sql/` folder).

The monitoring service includes integrated indexing functionality, so no separate indexer service is needed.
