#!/usr/bin/env python3
"""Check Foundation publisher coverage using the actual Grafana queries."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile


rules = {
    rule["uid"]: rule
    for group in json.loads(Path(__file__).with_name("grafana-rules.json").read_text())["groups"]
    for rule in group["rules"]
}
uids = ["pragma-gas-low", "pragma-publisher-foundation"]
prometheus_rules = []
for uid in uids:
    rule = rules[uid]
    query = rule["data"][0]["model"]["expr"]
    threshold = rule["data"][1]["model"]["conditions"][0]["evaluator"]["params"][0]
    prometheus_rules.append({"alert": uid, "expr": f"({query}) > {threshold}", "for": rule["for"]})

tests = []
labels = {"network": "Mainnet", "publisher": "STARKNET_FOUNDATION"}


def case(name, uid, series, fires):
    tests.append({
        "name": name,
        "interval": "1m",
        "input_series": [{"series": metric, "values": f"{value}x10"} for metric, value in series.items()],
        "alert_rule_test": [
            {"eval_time": "4m", "alertname": uid, "exp_alerts": []},
            {"eval_time": "5m", "alertname": uid,
             "exp_alerts": [{"exp_labels": labels}] if fires else []},
        ],
    })


balance = 'pragma_publisher_balance_strk_STRK{network="Mainnet",publisher="STARKNET_FOUNDATION"}'
others = {
    f'pragma_publisher_balance_strk_STRK{{network="Mainnet",publisher="{publisher}"}}': 1000
    for publisher in ["PRAGMA", "ARGENT", "STARKWARE", "AVNU"]
}
case("Foundation low while other publishers are healthy", uids[0], {**others, balance: 499}, True)
case("500 STRK is not below the threshold", uids[0], {**others, balance: 500}, False)
case("Sepolia balance does not trigger Mainnet warning", uids[0],
     {**others, balance: 1000, balance.replace("Mainnet", "Sepolia"): 3}, False)

age = 'pragma_publisher_last_update_seconds{network="Mainnet",publisher="STARKNET_FOUNDATION"}'
case("Foundation at freshness boundary", uids[1], {age: 2400}, False)
case("Foundation stopped publishing", uids[1], {age: 2401}, True)
case("Foundation missing while another publisher remains", uids[1], {age.replace("STARKNET_FOUNDATION", "AVNU"): 10}, True)

with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    rule_path = root / "rules.json"
    rule_path.write_text(json.dumps({"groups": [{"name": "publisher", "rules": prometheus_rules}]}))
    test_path = root / "tests.json"
    test_path.write_text(json.dumps({"rule_files": [str(rule_path)], "evaluation_interval": "1m", "tests": tests}))
    subprocess.run([sys.argv[1] if len(sys.argv) > 1 else "promtool", "test", "rules", str(test_path)], check=True)
print(f"Passed {len(tests)} publisher scenarios")
