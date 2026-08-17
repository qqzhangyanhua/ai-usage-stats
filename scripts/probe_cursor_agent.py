#!/usr/bin/env python3
"""Probe cursor-agent stream-json for token field locations. No session body."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

TOKEN_RE = re.compile(
    r"(token|usage|cost|billing|cache|reason|input|output|prompt|complet)",
    re.I,
)

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = ROOT / "docs" / "probe" / ".captures"
STREAM_FILE = OUT_DIR / "cursor-agent-stream.jsonl"
SUMMARY_FILE = OUT_DIR / "cursor-agent-stream-summary.json"


def walk_keys(obj: object, prefix: str = "", depth: int = 0, acc: set[str] | None = None) -> set[str]:
    if acc is None:
        acc = set()
    if depth > 8:
        return acc
    if isinstance(obj, dict):
        for key, value in obj.items():
            path = f"{prefix}.{key}" if prefix else str(key)
            acc.add(path)
            walk_keys(value, path, depth + 1, acc)
    elif isinstance(obj, list) and obj:
        walk_keys(obj[0], f"{prefix}[]", depth + 1, acc)
    return acc


def numeric_token_fields(obj: object, prefix: str = "", depth: int = 0, acc: dict[str, float] | None = None) -> dict[str, float]:
    if acc is None:
        acc = {}
    if depth > 8:
        return acc
    if isinstance(obj, dict):
        for key, value in obj.items():
            path = f"{prefix}.{key}" if prefix else str(key)
            if TOKEN_RE.search(str(key)) and isinstance(value, (int, float)) and not isinstance(value, bool):
                acc[path] = float(value)
            numeric_token_fields(value, path, depth + 1, acc)
    elif isinstance(obj, list):
        for item in obj[:5]:
            numeric_token_fields(item, f"{prefix}[]", depth + 1, acc)
    return acc


def analyze(lines: list[str]) -> dict[str, object]:
    event_types: Counter[str] = Counter()
    all_keys: set[str] = set()
    token_keys: set[str] = set()
    numeric_by_type: dict[str, dict[str, float]] = {}
    parse_errors = 0

    for line in lines:
        text = line.strip()
        if not text:
            continue
        try:
            event = json.loads(text)
        except json.JSONDecodeError:
            parse_errors += 1
            continue
        if not isinstance(event, dict):
            continue
        kind = str(event.get("type") or event.get("event") or event.get("kind") or "unknown")
        event_types[kind] += 1
        keys = walk_keys(event)
        all_keys |= keys
        token_keys |= {key for key in keys if TOKEN_RE.search(key)}
        nums = numeric_token_fields(event)
        if nums:
            numeric_by_type.setdefault(kind, {}).update(nums)

    return {
        "probed_at": datetime.now(timezone.utc).isoformat(),
        "line_count": len(lines),
        "parse_errors": parse_errors,
        "event_types": dict(event_types),
        "tokenish_keys": sorted(token_keys),
        "numeric_token_fields_by_event": numeric_by_type,
        "all_keys": sorted(all_keys),
        "has_token": bool(numeric_by_type),
    }


def run_agent() -> int:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    command = [
        "cursor-agent",
        "-p",
        "--output-format",
        "stream-json",
        "--mode",
        "ask",
        "--trust",
        "--workspace",
        str(ROOT),
        "Reply with exactly the word ok. Do not use tools.",
    ]
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=180,
            check=False,
        )
    except FileNotFoundError:
        SUMMARY_FILE.write_text(
            json.dumps({"error": "cursor-agent not found", "has_token": False}, indent=2) + "\n",
            encoding="utf-8",
        )
        print("cursor-agent not found", file=sys.stderr)
        return 1
    except subprocess.TimeoutExpired:
        SUMMARY_FILE.write_text(
            json.dumps({"error": "cursor-agent timed out", "has_token": False}, indent=2) + "\n",
            encoding="utf-8",
        )
        print("cursor-agent timed out", file=sys.stderr)
        return 1

    STREAM_FILE.write_text(completed.stdout, encoding="utf-8")
    summary = analyze(completed.stdout.splitlines())
    summary["exit_code"] = completed.returncode
    summary["stderr_len"] = len(completed.stderr)
    summary["stderr_has_error"] = bool(re.search(r"error|failed|denied", completed.stderr, re.I))
    SUMMARY_FILE.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: summary[k] for k in (
        "has_token",
        "event_types",
        "tokenish_keys",
        "numeric_token_fields_by_event",
        "exit_code",
        "line_count",
        "parse_errors",
    )}, ensure_ascii=False, indent=2))
    return 0 if completed.returncode == 0 else completed.returncode


if __name__ == "__main__":
    raise SystemExit(run_agent())
