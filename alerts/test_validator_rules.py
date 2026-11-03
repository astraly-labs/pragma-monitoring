#!/usr/bin/env python3
"""Exercise the deployed Grafana PromQL with promtool; no Python dependencies."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile


rules = {
    r["uid"].removeprefix("starknet-validator-"): r
    for r in json.loads(Path(__file__).with_name("validator-rules.json").read_text())[
        "groups"
    ][0]["rules"]
}
tests = []


def case(name, rule, series, expected, at="10m"):
    config = rules[rule]
    expr = config["data"][0]["model"]["expr"]
    threshold = config["data"][1]["model"]["conditions"][0]["evaluator"]["params"][0]
    tests.append(
        {
            "name": name,
            "interval": "15s",
            "input_series": [{"series": k, "values": v} for k, v in series.items()],
            "promql_expr_test": [
                {
                    "expr": f"(({expr}) > bool {threshold}) == 1",
                    "eval_time": at,
                    "exp_samples": [{"labels": label, "value": 1} for label in expected],
                }
            ],
        }
    )


jobs = ["pathfinder", "validator-attestation", "node-exporter"]
up = {f'up{{job="{job}"}}': "1x40" for job in jobs}
case("healthy components", "component-down", up, [])
for job in jobs:
    key = f'up{{job="{job}"}}'
    case(f"{job} down", "component-down", {**up, key: "1x39 0"}, [f'{{job="{job}"}}'])
    case(
        f"{job} disappeared",
        "component-down",
        {k: v for k, v in up.items() if k != key},
        [f'{{job="{job}"}}'],
    )
case(
    "scrapes stopped but old up=1 remains in lookback",
    "component-down",
    {k: "1x36 _x4" for k in up},
    [f'{{job="{job}"}}' for job in jobs],
)
case("entire datasource empty", "component-down", {}, [f'{{job="{job}"}}' for job in jobs])

node = 'current_block{job="pathfinder"}'
attestor = 'validator_attestation_starknet_latest_block_number{network="SN_MAIN",job="validator-attestation"}'
heads = {node: "100+1x40", attestor: "100+1x40"}
case("both heads advancing", "head-stale", heads, [])
for metric, job in [(node, "pathfinder"), (attestor, "validator-attestation")]:
    case(f"{job} frozen", "head-stale", {**heads, metric: "100x40"}, [f'{{job="{job}"}}'])
    case(
        f"{job} head absent",
        "head-stale",
        {k: v for k, v in heads.items() if k != metric},
        [f'{{job="{job}"}}'],
    )

failure = 'validator_attestation_attestation_failure_count{network="SN_MAIN"}'
missed = 'validator_attestation_missed_epochs_count{network="SN_MAIN"}'
counts = {failure: "18x40", missed: "0x40"}
case("historical failures do not page", "attestation-failed", counts, [])
case("new failure", "attestation-failed", {**counts, failure: "18x39 19"}, ["{}"])
case("missed epoch", "attestation-failed", {**counts, missed: "0x39 1"}, ["{}"])
case("counter reset without failure", "attestation-failed", {**counts, failure: "18x39 0"}, [])
case("failure window expired", "attestation-failed", {**counts, failure: "18 19x39"}, [])

avail = 'node_filesystem_avail_bytes{job="node-exporter",mountpoint="/",fstype="ext4"}'
size = 'node_filesystem_size_bytes{job="node-exporter",mountpoint="/",fstype="ext4"}'
case("disk below threshold", "disk-usage", {avail: "34x40", size: "100x40"}, [])
case("disk above threshold", "disk-usage", {avail: "14x40", size: "100x40"}, ["{}"])

with tempfile.TemporaryDirectory() as directory:
    path = Path(directory) / "validator-tests.json"
    path.write_text(json.dumps({"evaluation_interval": "30s", "tests": tests}))
    subprocess.run([sys.argv[1] if len(sys.argv) > 1 else "promtool", "test", "rules", str(path)], check=True)
print(f"Passed {len(tests)} validator scenarios")
