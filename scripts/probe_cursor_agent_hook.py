#!/usr/bin/env python3
"""Cursor hook probe: record field paths only. Never persist conversation text."""

from __future__ import annotations

import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

TOKEN_RE = re.compile(
    r"(token|usage|cost|billing|cache|reason|input|output|prompt|complet)",
    re.I,
)
TEXT_KEYS = {
    "text",
    "content",
    "prompt",
    "message",
    "user_message",
    "agent_message",
    "transcript",
    "title",
}

CAPTURE_DIR = Path(__file__).resolve().parent.parent / "docs" / "probe" / ".captures"
CAPTURE_FILE = CAPTURE_DIR / "cursor-agent-hook.jsonl"


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
        for item in obj[:3]:
            numeric_token_fields(item, f"{prefix}[]", depth + 1, acc)
    return acc


def redacted_types(obj: object, prefix: str = "", depth: int = 0, acc: dict[str, str] | None = None) -> dict[str, str]:
    if acc is None:
        acc = {}
    if depth > 6:
        return acc
    if isinstance(obj, dict):
        for key, value in obj.items():
            path = f"{prefix}.{key}" if prefix else str(key)
            if str(key) in TEXT_KEYS and isinstance(value, str):
                acc[path] = f"str:{len(value)}"
            elif isinstance(value, (int, float)) and not isinstance(value, bool):
                acc[path] = "number"
            elif isinstance(value, bool):
                acc[path] = "bool"
            elif value is None:
                acc[path] = "null"
            elif isinstance(value, str):
                acc[path] = f"str:{len(value)}"
            elif isinstance(value, list):
                acc[path] = f"list:{len(value)}"
                redacted_types(value, path, depth + 1, acc)
            elif isinstance(value, dict):
                acc[path] = f"object:{len(value)}"
                redacted_types(value, path, depth + 1, acc)
    return acc


def main() -> None:
    raw = sys.stdin.read()
    try:
        payload = json.loads(raw) if raw.strip() else {}
    except json.JSONDecodeError:
        payload = {"_parse_error": True, "_raw_len": len(raw)}

    record = {
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "hook_event_name": payload.get("hook_event_name") if isinstance(payload, dict) else None,
        "keys": sorted(walk_keys(payload)),
        "tokenish_keys": sorted(key for key in walk_keys(payload) if TOKEN_RE.search(key)),
        "numeric_token_fields": numeric_token_fields(payload),
        "field_types": redacted_types(payload),
    }
    CAPTURE_DIR.mkdir(parents=True, exist_ok=True)
    with CAPTURE_FILE.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, ensure_ascii=False) + "\n")
    print("{}")


if __name__ == "__main__":
    main()
