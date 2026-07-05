#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fail(msg: str) -> None:
    raise SystemExit(f"[agent-doc-check] {msg}")


required_files = [
    ROOT / "AI_AGENT.md",
    ROOT / "docs" / "agent-playbooks.md",
    ROOT / "skills" / "index.json",
    ROOT / "README.md",
    ROOT / "README.en.md",
]
for path in required_files:
    if not path.exists():
        fail(f"missing required file: {path}")

index = json.loads((ROOT / "skills" / "index.json").read_text(encoding="utf-8"))
if "skills" not in index or not isinstance(index["skills"], list):
    fail("skills/index.json must contain a 'skills' array")

required_fields = {"id", "entry", "keywords", "preconditions", "recommended_commands", "risk", "requires_confirmation"}
skill_ids = set()
entry_paths = set()
for item in index["skills"]:
    missing = required_fields - set(item.keys())
    if missing:
        fail(f"skill entry missing fields: {sorted(missing)}")
    sid = item["id"]
    if sid in skill_ids:
        fail(f"duplicate skill id: {sid}")
    skill_ids.add(sid)

    entry = ROOT / item["entry"]
    if not entry.exists():
        fail(f"skill entry path does not exist: {entry}")
    entry_paths.add(entry.resolve())

    risk = item["risk"]
    if risk.get("level") not in {"low", "medium", "high"}:
        fail(f"invalid risk level for skill {sid}: {risk.get('level')}")

    if not isinstance(item["keywords"], list) or not item["keywords"]:
        fail(f"keywords must be a non-empty list for skill {sid}")

all_skill_docs = sorted((ROOT / "skills").glob("*/SKILL.md"))
all_skill_docs.append(ROOT / "skills" / "workhorse" / "SKILL.md")
all_skill_docs = sorted({p.resolve() for p in all_skill_docs})

# Keep only actual files (glob above may include duplicates from set ops)
all_skill_docs = [p for p in all_skill_docs if p.exists()]
for doc in all_skill_docs:
    if doc not in entry_paths:
        fail(f"skill doc not indexed in skills/index.json: {doc}")

ai_agent_text = (ROOT / "AI_AGENT.md").read_text(encoding="utf-8")
for token in ["Task Classification", "Standard Commands", "Success Criteria", "Safety Boundaries", "skills/index.json"]:
    if token not in ai_agent_text:
        fail(f"AI_AGENT.md missing section/token: {token}")

playbooks_text = (ROOT / "docs" / "agent-playbooks.md").read_text(encoding="utf-8")
for token in [
    "Remote Build Playbook",
    "Horsed Deploy Playbook",
    "Artifact Retrieval Playbook",
    "Health/Logs Troubleshooting Playbook",
    "Remote Exec Playbook",
]:
    if token not in playbooks_text:
        fail(f"agent-playbooks.md missing playbook: {token}")

for readme in [ROOT / "README.md", ROOT / "README.en.md"]:
    text = readme.read_text(encoding="utf-8")
    if "AI_AGENT.md" not in text:
        fail(f"{readme.name} must mention AI_AGENT.md")
    if "health --json" not in text:
        fail(f"{readme.name} must mention health --json")

print("[agent-doc-check] OK")
