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
- `long_tail_asset_threshold{pair, type}`: Deviation threshold configuration for long tail assets. Type can be either "low" for when the pair has less than 7 sources else "high".
- `long_tail_asset_source_deviation{network, pair, type}`: Deviation of a source from the on-chain aggregated median price given source and pair. (in percents)
- `long_tail_asset_total_sources{network, pair, type}`: Current number of sources available for a given pair.
- `publisher_balance{network, publisher}`: Balance of a publisher. (in ETH)
- `vrf_balance{network}`: Balance of the VRF contract. (in ETH)
- `vrf_requests_count{network, status}`: Number of VRF requests handled for a given network.
- `vrf_time_since_last_handle_request{network}`: Time since the last VRF request was handled for a given network.
- `vrf_time_since_oldest_request_in_pending_status{network}`: Time in seconds that the oldest pending VRF request has been in the pending status for a given network.

Metrics specifics to our Pragma App Chain:
- `dispatch_event_latest_block`: The latest block that triggered a Dispatch event from Hyperlane,
- `dispatch_event_feed_latest_block_update`: The latest block that triggered a Dispatch event from Hyperlane for a specific Feed ID,
- `dispatch_event_nb_feeds_updated`: The number of feeds updated per Dispatch event at a given block.

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

# Pragma API key
PRAGMA_API_KEY=

# RPC URL
RPC_URL=

# Indexer Service URL
INDEXER_SERVICE_URL=

# Config
NETWORK=testnet
ORACLE_ADDRESS=0x
VRF_ADDRESS=0x
PAIRS=BTC/USD,ETH/USD
IGNORE_SOURCES=BITSTAMP,DEFILLAMA
IGNORE_PUBLISHERS=BINANCE

# Prometheus
TELEGRAM_TOKEN=
OPSGENIE_API_KEY=
```

In order for the full flow to work you will need to have tables following the table schemas defined <a href="src/schema.rs">here</a>.

You can use our [indexer service](https://github.com/Astraly-Labs/indexer-service) on this repository to spin off your indexer in a few commands very easily.
