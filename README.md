# Pragma Monitoring

## OTEL Exporter

This service runs an OTEL exporter.

This service polls at a regular interval the database that is filled by our indexers.
It then processes the data and computes the following metrics:

- `time_since_last_update_seconds{network, publisher, typr}`: Time since a publisher has published any data. (in seconds)
- `pair_price{network, pair, source, type}`: Latest price of an asset for a given source and pair. (normalized to asset's decimals)
- `time_since_last_update_pair_id{network, pair, type}`: Time since an update has been published for a given pair. (in seconds)
- `price_deviation{network, pair, source, type}`: Deviation of the price from a reference price (DefiLlama API) given source and pair. (in percents)
- `price_deviation_source{network, pair, source, type}`: Deviation of the price from the on-chain aggregated median price given source and pair. (in percents)
- `publisher_balance{network, publisher}`: Balance of a publisher. (in ETH)

## Shared Public Access

Monitoring is not publicicly available yet but databases will soon be in read-only mode.

## Self-Hosting

We have created a `docker-compose.yml` file to help with self-hosting setup:

```bash
docker compose up -d
```

You can then access prometheus dashboard at <http://localhost:9000> and grafana at <http://localhost:3000>.

Make sure to first fill the envirronement file with your own config parameters:

```bash
# The database URL the application will use for writes (primary database).
DATABASE_URL='postgres://postgres:postgres@localhost:5432/postgres'

# (Optional) The database URL for reads (replica database).
# If not set, DATABASE_URL will be used for both reads and writes.
DATABASE_READ_URL='postgres://postgres:postgres@replica:5432/postgres'

# (Optional) Replication delay in milliseconds to wait after writes before reads.
# Set this if you're using database replication and experiencing read-after-write consistency issues.
# Default: 0 (no delay)
REPLICATION_DELAY_MS=100

# The OTEL endpoint to send metrics to.
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317

# (Optional) Defillama API Key
DEFILLAMA_API_KEY=

# Pragma API key
PRAGMA_API_KEY=

# RPC URL
RPC_URL=

# APIBARA API Key
APIBARA_API_KEY=

# Config
NETWORK=testnet
ORACLE_ADDRESS=0x
PAIRS=BTC/USD,ETH/USD
IGNORE_SOURCES=BITSTAMP,DEFILLAMA
IGNORE_PUBLISHERS=BINANCE
```

### Database Configuration

The monitoring service now supports separate read and write database connections to handle database replication scenarios:

- **Single Database Setup**: If you're using a single database, only set `DATABASE_URL`. The service will use this for both reads and writes.

- **Replicated Database Setup**: If you're using a primary-replica setup:
  - Set `DATABASE_URL` to your primary (write) database
  - Set `DATABASE_READ_URL` to your replica (read) database
  - Set `REPLICATION_DELAY_MS` to a value (e.g., 100-500ms) to wait after writes before reads, allowing replication to catch up

This prevents read-after-write consistency issues where data written to the primary isn't immediately available on the replica.

In order for the full flow to work you will need to have tables following the table schemas defined [here in the schema.rs file](src/schema.rs).

The monitoring service now includes integrated indexing functionality, so no separate indexer service is needed.
