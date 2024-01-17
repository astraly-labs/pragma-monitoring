# Pragma Monitoring

## Prometheus Exporter

This service runs a prometheus exporter that exposes metrics on the `/metrics` route.
It powers our internal grafana dashboards and alerts.

This service polls at a regular interval the database that is filled by our indexers.
It then processes the data and computes the following metrics:

- `time_since_last_update_seconds{network, publisher, typr}`: Time since a publisher has published any data. (in seconds)
- `pair_price{network, pair, source, type}`: Latest price of an asset for a given source and pair. (normalized to asset's decimals)
- `time_since_last_update_pair_id{network, pair, type}`: Time since an update has been published for a given pair. (in seconds)
- `price_deviation{network, pair, source, type}`: Deviation of the price from a reference price (DefiLlama API) given source and pair. (in percents)
- `price_deviation_source{network, pair, source, type}`: Deviation of the price from the on-chain aggregated median price given source and pair. (in percents)

## Shared Public Access

Monitoring is not publicicly available yet but databases will soon be in read-only mode.

## Self-Hosting

We have created a `docker-compose.yml` file to help with self-hosting setup:

```bash
docker compose up -d
```

You can then access prometheus dashboard at http://localhost:9000 and grafana at http://localhost:3000.

Make sure to first fill the envirronement file with your own config parameters:

```bash
# The database URL the application will use to connect to the database.
DATABASE_URL='postgres://postgres:postgres@localhost:5432/postgres'

# (Optional) Defillama API Key
DEFILLAMA_API_KEY=

# RPC URL
RPC_URL=

# Indexer Service URL
INDEXER_SERVICE_URL=

# Config
NETWORK=testnet
ORACLE_ADDRESS=0x
PAIRS=BTC/USD,ETH/USD
IGNORE_SOURCES=BITSTAMP,DEFILLAMA
IGNORE_PUBLISHERS=BINANCE

# Prometheus
TELEGRAM_BOT_TOKEN=
OPSGENIE_API_KEY=
```

In order for the full flow to work you will need to have tables following the table schemas defined <a href="src/schema.rs">here</a>.

You can use our [indexer service](https://github.com/Astraly-Labs/indexer-service) on this repository to spin off your indexer in a few commands very easily.
