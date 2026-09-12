# Oracle alerting

`grafana-rules.json` contains 19 Grafana rules for the production Mimir datasource
`eemnwzj4tmubke`. Queries were executed against live data on 11 and 12 September 2026.
The metric suffixes and `network="Mainnet"` label match the actual OTEL export.

The 19 rules were **enabled in production on 12 September 2026** in the
`pragma-oracle-safety` evaluation group, every 60 seconds, in folder `onchain`
(`bfdt3p63tx4w0d`). The Telegram contact point `pragma-oracle-alerts` was created
and a Grafana test notification was independently read back from the destination
group before activation. Live source-deviation, stopped-publisher, stale-feed and
missing-reference notifications were also read back after activation; all 19 rules
evaluated without execution errors. The bot credential is stored in Grafana secret
settings. Recovery delivery remains to be observed when incidents resolve.

This file reflects the enabled production state. For another installation, set
`isPaused` to true until its datasource, recipient group and delivery are verified.

## Coverage

- Individually track PRAGMA, ARGENT (Ready), STARKWARE and AVNU, including a
  publisher disappearing completely from telemetry. Add the Foundation explicitly
  after its registry name is agreed; do not infer required publishers from activity.
- Missing monitor metrics, a stalled indexer, and missing/error query results.
- BTC, ETH, STRK, WBTC, USDC and USDT feed freshness and minimum source count.
- Signed source/median deviations in either direction, with no upper cutoff.
- Nonpositive prices and USDC/USDT source deviations from $1. These alerts require
  investigation; a depeg is not permission to hardcode a peg.
- Independent-reference divergence and low STRK gas balances.

The current BTCFi publisher config allows a 600-second heartbeat. Publisher and
major-feed staleness thresholds are therefore 900 seconds plus one minute pending.
They are starting operational thresholds, not an agreed SLA. Calibrate gas warnings
against actual spend. Conversion-rate feeds, other assets and Miden need their own
cadence/source policy before extending the six-feed source-count check.

## Existing gap found during validation

The existing `onchain-vs-offchain-deviation` rule (`cfdt4hyfzd69sc`) uses an
exclusive range from 2.5% to 9.9%, which misses larger failures. It also queries
`pragma_deviation_onchain_vs_offchain_ratio`, which returned no series during this
review. The new reference rule is unbounded above and explicitly alerts on missing
or failed data. Restore the independent reference ingestion before declaring this
protection operational. Source/median agreement alone cannot detect all publishers
making the same conversion error.

ARGENT and STARKWARE had not reported for approximately 3.5 days at the initial
check. BROTHER/USDPLUS showed an extreme source/median discrepancy requiring
mapping/decimal investigation. These are observations, not diagnoses or authority
to change feeds automatically.

## Deployment and verification

1. Create the Telegram contact point with the alert bot and the private group chat ID.
   Keep the bot token in Grafana's secret settings, not this repository.
2. Send a test notification to the group and confirm receipt. Invite the nominated
   responders and assign a primary and backup for acknowledgement and escalation.
3. Provision this file with Grafana's alerting provisioning mechanism. The folder
   name and datasource UID must match the target installation. Enable only after
   testing delivery. The API deployment uses the existing folder UID above;
   file provisioning uses its case-sensitive name `onchain`.
4. Check ARGENT and STARKWARE fire, and PRAGMA and AVNU evaluate against their actual
   timestamps. Check that an absent expected publisher also fires. Restore the
   stopped publishers and confirm the recovery notification reaches Telegram.
5. Verify a 50% deviation triggers the extreme rule; verify both signs. Confirm
   the independent-reference rule fires on No Data until that ingestion is restored.
6. Retire or update the superseded legacy deviation rule after responders have
   moved to the new channel. It remains unchanged to preserve the existing Slack
   route while the Telegram responder roster is incomplete. Keep unrelated gas,
   trading, and infrastructure alerts intact.

The Telegram group created during this work is recorded in the private operational
handoff, not in this public repository. Rule summaries identify the affected
publisher/pair/source through Grafana labels; descriptions contain the first
response steps. Alerts never execute ownership changes, source removals or trades.
