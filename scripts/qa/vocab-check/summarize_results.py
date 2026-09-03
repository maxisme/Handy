#!/usr/bin/env python3
"""Print one table comparing every run in results/. Optionally write RESULTS.md."""
import json
import sys
from pathlib import Path

HERE = Path(__file__).parent
rows = []
for path in sorted((HERE / "results").glob("*.json")):
    if path.name == "smoke.json":
        continue
    r = json.loads(path.read_text())
    c, m = r["config"], r["metrics"]
    if c.get("limit") or c.get("only"):
        continue
    rows.append({
        "file": path.name,
        "prompt": c["prompt"],
        "schema": c.get("schema", "bool"),
        "greedy": "yes" if c["greedy"] else "no",
        "mode": f'{c["mode"]}({c["batchSize"]})',
        "context": "on" if c["context"] else "off",
        "precision": m["precision"],
        "recall": m["recall"],
        "agreement": m["agreement"],
        "p95": m["latency"]["p95Ms"],
        "median": m["latency"]["medianMs"],
        "fp": m["falsePositives"],
        "fn": m["falseNegatives"],
        "parse": m["parseFailures"],
        "gate": "PASS" if not r["gateFailures"] else "FAIL",
    })

header = "| prompt | schema | greedy | mode | context | precision | recall | agreement | p95 ms | median ms | FP | FN | parse fail | gate |"
sep = "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
lines = [header, sep]
for r in rows:
    lines.append(f'| {r["prompt"]} | {r["schema"]} | {r["greedy"]} | {r["mode"]} | {r["context"]} | {r["precision"]:.2f} | {r["recall"]:.2f} | {r["agreement"]:.2f} | {r["p95"]:.0f} | {r["median"]:.0f} | {r["fp"]} | {r["fn"]} | {r["parse"]} | {r["gate"]} |')
table = "\n".join(lines)
print(table)

if "--write" in sys.argv:
    doc = ["# vocab-check results\n",
           "Every full run in `results/`, oldest first. 62 cases, 3 runs each, on this machine. Regenerate with `python3 summarize_results.py --write`.\n",
           table, ""]
    (HERE / "RESULTS.md").write_text("\n".join(doc))
    print("wrote RESULTS.md")
