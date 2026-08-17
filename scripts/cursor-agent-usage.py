#!/usr/bin/env python3
"""cursor-agent 用量采集包装。

用法：把日常无头调用从
    cursor-agent -p "..."
换成
    python3 scripts/cursor-agent-usage.py "..."

透传所有参数给 `cursor-agent`，强制走 `-p --output-format stream-json`，把子进程 stdout
原样转发给你，同时把 `system` 事件的 model/cwd/session_id 与每条 `type=result` 事件的
usage 追加落盘到 `~/.cursor-agent-usage/<session_id>.jsonl`。

只落元数据 + usage，剔除 result 正文（回答内容），符合本项目「不存会话正文」的原则。
仅覆盖走本包装的无头调用；交互式会话与 IDE Agent 不会被采集。
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

USAGE_DIR = Path(
    os.environ.get("CURSOR_AGENT_USAGE_DIR", str(Path.home() / ".cursor-agent-usage"))
)


def build_command(argv: list[str]) -> tuple[list[str], bool]:
    """在用户参数基础上补齐无头 stream-json 所需开关。返回 (命令, 是否可解析落盘)。"""
    args = list(argv)
    has_print = any(a in ("-p", "--print") for a in args)
    fmt_index = next((i for i, a in enumerate(args) if a == "--output-format"), None)
    parseable = True

    if fmt_index is not None and fmt_index + 1 < len(args):
        # 用户显式选了其它格式，尊重它，但无法解析落盘。
        parseable = args[fmt_index + 1] == "stream-json"
    else:
        args += ["--output-format", "stream-json"]

    if not has_print:
        args = ["-p", *args]

    return (["cursor-agent", *args], parseable)


def slim_system(event: dict) -> dict:
    return {
        "type": "system",
        "subtype": event.get("subtype"),
        "model": event.get("model"),
        "cwd": event.get("cwd"),
        "session_id": event.get("session_id"),
    }


def slim_result(event: dict) -> dict:
    # 丢弃 `result` 正文文本，只保留计量所需字段。
    return {
        "type": "result",
        "subtype": event.get("subtype"),
        "is_error": event.get("is_error"),
        "session_id": event.get("session_id"),
        "request_id": event.get("request_id"),
        "model": event.get("model"),
        "duration_ms": event.get("duration_ms"),
        "usage": event.get("usage"),
        "captured_at": datetime.now(timezone.utc).isoformat(),
    }


def append_record(session_id: str | None, record: dict) -> None:
    sid = session_id or "unknown-session"
    USAGE_DIR.mkdir(parents=True, exist_ok=True)
    target = USAGE_DIR / f"{sid}.jsonl"
    with target.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, ensure_ascii=False) + "\n")


def handle_line(line: str, seen_system: set[str]) -> None:
    try:
        event = json.loads(line)
    except json.JSONDecodeError:
        return
    if not isinstance(event, dict):
        return
    kind = event.get("type")
    session_id = event.get("session_id")
    if kind == "system":
        # 每个 session 只落一次 system，避免重复行。
        key = str(session_id)
        if key not in seen_system:
            seen_system.add(key)
            append_record(session_id, slim_system(event))
    elif kind == "result" and event.get("usage") is not None:
        append_record(session_id, slim_result(event))


def main() -> int:
    command, parseable = build_command(sys.argv[1:])
    try:
        process = subprocess.Popen(
            command,
            stdout=subprocess.PIPE,
            stderr=None,
            text=True,
            bufsize=1,
        )
    except FileNotFoundError:
        print("cursor-agent not found on PATH", file=sys.stderr)
        return 127

    seen_system: set[str] = set()
    assert process.stdout is not None
    try:
        for line in process.stdout:
            sys.stdout.write(line)
            sys.stdout.flush()
            if parseable:
                handle_line(line.strip(), seen_system)
    except KeyboardInterrupt:
        process.terminate()
        return 130
    finally:
        process.stdout.close()

    return process.wait()


if __name__ == "__main__":
    raise SystemExit(main())
