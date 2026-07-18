#!/usr/bin/env python3
from __future__ import annotations

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fail(msg: str) -> None:
    raise SystemExit(f"[agent-regression] {msg}")


index = json.loads((ROOT / "skills" / "index.json").read_text(encoding="utf-8"))
skills = {s["id"]: s for s in index["skills"]}

cases = json.loads((ROOT / "scripts" / "agent-regression-cases.json").read_text(encoding="utf-8"))
if not (10 <= len(cases) <= 20):
    fail(f"expected 10-20 regression cases, got {len(cases)}")

for case in cases:
    for field in ["id", "query", "expected_skill", "expected_playbook", "requires_confirmation"]:
        if field not in case:
            fail(f"case missing field {field}: {case}")

    sid = case["expected_skill"]
    if sid not in skills:
        fail(f"case references unknown skill {sid}: {case['id']}")

    skill_requires_confirmation = bool(skills[sid]["requires_confirmation"])
    if case["requires_confirmation"] and not skill_requires_confirmation:
        fail(f"case {case['id']} expects confirmation but skill {sid} does not require it")

playbooks_text = (ROOT / "docs" / "agent-playbooks.md").read_text(encoding="utf-8")
for case in cases:
    if case["expected_playbook"] not in playbooks_text:
        fail(f"case {case['id']} references missing playbook: {case['expected_playbook']}")

binary = ROOT / "target" / "debug" / "cargo-work"
if not binary.exists():
    fail("target/debug/cargo-work does not exist, run cargo build --bin cargo-work first")

health_help = subprocess.run(
    [str(binary), "work", "health", "--help"],
    cwd=str(ROOT),
    check=True,
    capture_output=True,
    text=True,
).stdout
if "--json" not in health_help:
    fail("health --help does not expose --json")

exec_help = subprocess.run(
    [str(binary), "work", "exec", "--help"],
    cwd=str(ROOT),
    check=True,
    capture_output=True,
    text=True,
).stdout
if "--no-sync" not in exec_help:
    fail("exec --help does not expose --no-sync")

for cmd in (
    [str(binary), "work", "job", "--help"],
    [str(binary), "work", "logs", "--help"],
    [str(binary), "work", "ping", "--help"],
):
    subprocess.run(cmd, cwd=str(ROOT), check=True, capture_output=True, text=True)

print("[agent-regression] OK")
