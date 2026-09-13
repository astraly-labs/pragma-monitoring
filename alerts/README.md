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
settings. Recovery delivery was observed in the 13 September Telegram audit.

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

## Notification budget and thresholds (13 September 2026)

Telegram delivery now groups all `service=pragma-oracle` alerts together, waits five
minutes initially, sends changed groups six hours apart, and repeats unchanged
incidents every 24 hours. This targets at most four routine reports per day.
Underlying rules still evaluate every minute; Telegram can delay a new issue by
up to six hours. It is a summary channel, not a timely paging channel. A primary
and backup responder must actively monitor Grafana or have a separate pager.

`telegram-policy.json` is a child route to merge into the existing policy tree.
Remove each rule's direct `notification_settings` so all 19 rules use this route.
Preserve unrelated routes. Policies already provisioned as `api` must be updated
without `X-Disable-Provenance`; that header changes provenance and is rejected.
`telegram-message.tmpl` produces compact reports, capped at 16 affected
observations with an explicit link to the complete alert list.

| Check                           | Trigger                                      | Persistence |
| ------------------------------- | -------------------------------------------- | ----------- |
| Expected publisher / major feed | Age > 40 minutes or missing                  | 5 minutes   |
| Source versus median            | Absolute deviation > 5%                      | 5 minutes   |
| Extreme source deviation        | Absolute deviation > 25%                     | Immediate   |
| Nonpositive source price        | Price <= 0                                   | Immediate   |
| USDC / USDT source price        | Absolute deviation from $1 > 2%              | 5 minutes   |
| Independent reference           | Absolute deviation > 2.5%, missing or failed | 5 minutes   |
| Major feed source count         | Fewer than 4 sources                         | 5 minutes   |
| Publisher gas                   | Less than 500 STRK                           | 5 minutes   |
| Telemetry / indexer             | No telemetry for 5m / no progress for 20m    | 5 minutes   |

The deployed price-pusher v2.14.4 mainnet configuration uses an 1800-second
heartbeat. Observed core feed ages of 31–32 minutes are consistent with that
cadence plus ingestion delay; the initial 20-minute alert cutoff was too short.
The 40-minute cutoff plus five minutes of persistence leaves room for normal
submission and indexing, while detecting sustained misses. A 600-second heartbeat
appears in the BTCFi onboarding example, but it is not the deployed mainnet
configuration or an agreed service level. Tightening publishing cadence requires
an explicit operational decision and rechecking the corresponding alert cutoff.

A Telegram audit found 614 messages in 24 hours: 320 firing and 294 recovered,
mostly repeated PRAGMA and core-feed freshness transitions. Ready and StarkWare
were approximately 5.3 days stale. Independent reference metrics remained absent.

The source monitor used `10u32.pow(decimals)`, which overflows at 18 decimals and
explains the near-100% BROTHER/USDPLUS deviation. `price_scale.rs` replaces integer
powers across source, reference and event normalization. Its tests cover matching
18-decimal prices, a real >25% deviation, and 0/8/27 decimal scaling. Deploy this
code fix before treating the current BROTHER deviation as a market discrepancy;
verify its actual quote mapping separately. Do not raise the deviation threshold
to conceal this arithmetic bug. LORDS/DefiLlama showed approximately 7% disagreement
and still needs source and timestamp comparison.

### Actions to meet the operating rules

- Restore Ready and StarkWare publishing; add and verify the Foundation publisher.
- Keep required feeds within the deployed 30-minute heartbeat plus indexing delay;
  investigate sustained age over 40 minutes. Agree any tighter publishing cadence
  with operators and consumers before treating it as an operating rule.
- Deploy the normalization fix and verify 18-decimal feeds against raw contract
  outputs, including BROTHER/USDPLUS and its quote currency.
- Restore independent reference ingestion; verify all six required assets produce
  fresh reference comparisons, with a test that detects a >2.5% mismatch.
- Nominate a primary and backup responder, invite them to the alerts group, and
  establish acknowledgement/escalation ownership. The six-hour summary is not
  sufficient for fast incident response on its own.
- Validate recovery and the notification budget over the next complete 24 hours.

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
mapping/decimal investigation. The arithmetic cause of the BROTHER source-monitor discrepancy was identified on
13 September; the code correction still requires a production rollout.

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
