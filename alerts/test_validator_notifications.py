#!/usr/bin/env python3
"""Render notification regressions in Grafana without saving or sending alerts."""

from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import re
import urllib.request


root = Path(__file__).parent
rules = json.loads((root / "validator-rules.json").read_text())["groups"][0]["rules"]
policy = json.loads((root / "validator-telegram-policy.json").read_text())
template = (root / "validator-telegram-message.tmpl").read_text()

# A query timeout must never become a service-down, disk-full or missed-epoch claim.
assert all(r["execErrState"] == "Error" and r["noDataState"] == "NoData" for r in rules)
error_route = policy["routes"][0]
assert "rulename" not in error_route["group_by"]
assert "datasource_uid" in error_route["group_by"]
for name in ("DatasourceError", "DatasourceNoData"):
    assert re.fullmatch(error_route["object_matchers"][0][2], name)
assert not re.fullmatch(error_route["object_matchers"][0][2], rules[0]["title"])
assert error_route["group_wait"] == "2m" and error_route["repeat_interval"] == "6h"

# The deployed sender reads the plural key; its UI schema's singular key is ignored.
request = urllib.request.Request(
    os.environ["GRAFANA_URL"].rstrip("/") + "/api/v1/provisioning/contact-points?name=pragma-validator-alerts",
    headers={"Authorization": "Bearer " + os.environ["GRAFANA_API_KEY"]},
)
with urllib.request.urlopen(request, timeout=30) as response:
    contact = next(c for c in json.load(response) if c["name"] == "pragma-validator-alerts")
assert contact["settings"].get("disable_notifications") is True
assert contact["settings"]["message"] == template and "@" not in template
assert contact["settings"]["parse_mode"] == "None"

now = datetime.now(timezone.utc)
cases = [
    ("DatasourceError", False, "MONITORING UNAVAILABLE", "ALERT:"),
    ("DatasourceNoData", False, "MONITORING UNAVAILABLE", "ALERT:"),
    ("DatasourceError", True, "MONITORING RESTORED", "RECOVERED"),
    (rules[0]["title"], False, "ALERT: 1 active", "MONITORING UNAVAILABLE"),
    (rules[0]["title"], True, "RECOVERED", "MONITORING UNAVAILABLE"),
]
for alertname, resolved, expected, forbidden in cases:
    alert = {
        "labels": {"alertname": alertname, "rulename": rules[0]["title"],
                   "service": "pragma-oracle", "component": "starknet-validator",
                   "datasource_uid": "cffjiwjazlvy8b"},
        "annotations": {"summary": "SERVICE FAILURE CLAIM", "Error": "query timeout"},
        "startsAt": (now - timedelta(minutes=10)).isoformat(),
        "endsAt": (now + timedelta(minutes=-1 if resolved else 10)).isoformat(),
    }
    request = urllib.request.Request(
        os.environ["GRAFANA_URL"].rstrip("/") + "/api/alertmanager/grafana/config/api/v1/templates/test",
        data=json.dumps({"name": "validator-regression", "alerts": [alert],
                         "template": '{{ define "validator-regression" }}' + template + '{{ end }}'}).encode(),
        headers={"Authorization": "Bearer " + os.environ["GRAFANA_API_KEY"],
                 "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        result = json.load(response)
    assert not result.get("errors"), result.get("errors")
    rendered = next(r["text"] for r in result["results"] if r["name"] == "validator-regression")
    assert expected in rendered and forbidden not in rendered, rendered
    assert "@" not in rendered, rendered
    if alertname.startswith("Datasource"):
        assert "SERVICE FAILURE CLAIM" not in rendered, rendered
    print(f"Passed {alertname} {'resolved' if resolved else 'firing'}")
