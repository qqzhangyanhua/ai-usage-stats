# 本机 AI 用量统计 (Local AI Usage Statistics)

一个在本机运行的图形界面工具，扫描各类 AI 编程 CLI 工具留在本地的会话数据，聚合并展示 token 消耗（及可选费用）的明细。

## Language

**消耗记录 (Usage Record)**：
统一的标准化用量条目，是所有工具数据归一化后的通用模型。至少包含：时间、来源工具、模型、provider、项目、会话 ID（及原始文件定位）、各口径 token（输入/输出/缓存/推理/总量）、可选费用。不含会话正文。
_Avoid_: 日志、log、message（这些是原始数据，不是归一后的记录）

**来源 (Source)**：
一个被统计的 AI 工具（如 codex、claude code、pi、opencode、kimi 等）。每个 Source 有各自的本地存储格式与字段命名。
_Avoid_: 工具、tool、渠道

**适配器 (Adapter)**：
把某个 Source 的原始存储格式解析、归一化成「消耗记录」的模块。新增一个工具 = 新增一个 Adapter，统计与界面逻辑不受影响。
_Avoid_: parser、解析器、插件

**Token 口径 (Token Dimension)**：
token 的分类计量：输入 (input)、输出 (output)、缓存读 (cache read)、缓存写/创建 (cache creation)、推理 (reasoning)、总量 (total)。不同工具暴露的口径不完全一致。
_Avoid_: token 类型

**代码量 (Code Volume)**：
Cursor 一类工具本地只记录的「AI 生成代码行数、AI 占比」，与 token 无关，是独立维度，界面上与 token 严格分区。
_Avoid_: 用量、消耗（避免与 token 混淆）

**Cursor 账号用量 (Cursor Account Usage)**：
从 Cursor 云端仪表盘拉回的账号级 token 事件，含全部设备与全时段，self-serve 计划下仅有 token、没有费用。独立于本机消耗记录与代码量，不并入本机 token 总量。会话 token 在设置页或 Cursor 页粘贴进钥匙串；缓存可在设置页独立清空，不参与本机文件对账。
_Avoid_: 把它叫成本机用量、消耗记录，或与代码量混称

**Cursor 会话 (Cursor Session)**：
从本机 `~/.cursor/projects/*/agent-transcripts` jsonl 解析的行为统计（会话数、轮次、工具调用、失败率等）。Cursor IDE Agent 与 `cursor-agent` CLI 写同一套目录，无法从路径区分来源。只存聚合、不含对话正文。独立于消耗记录、代码量与账号用量；随 `ingest_all` 自动扫描，不进总览 token KPI。
_Avoid_: 与消耗记录、会话管理（token 会话列表）、代码量混称；不要把 `~/.cursor-agent-usage` 当成官方会话目录

## 采集源现状

| Source | 存储 | 本机 token | 本机费用 |
|--------|------|:---:|:---:|
| Codex | jsonl `~/.codex/sessions` | ✅ | ❌ |
| Claude Code | jsonl `~/.claude/projects` | ✅ | ✅ 自带 `costUSD` |
| pi | jsonl `~/.pi/agent/sessions` | ✅ | ✅ 自带 |
| dsh | zstd jsonl `~/.dsh/sessions` | ✅(需解压) | ❌ |
| opencode | sqlite+json `~/.local/share/opencode` | ✅ | ✅ 自带 |
| kimi | jsonl `~/.kimi/sessions/*/wire.jsonl` | ✅ | ❌ |
| gemini | json `~/.gemini/tmp/*/chats/session-*.json` | ✅ | ❌ |
| grok | `~/.grok/sessions` | ✅（`turn_completed.usage`） | ✅ 自带 `costUsdTicks` |
| qwen | `~/.qwen/tmp/*/logs.json` | ❌（本地无 Token） | ❌ |
| Factory/droid | `~/.factory/sessions/**/<id>.settings.json` | ✅（会话累计、无模型名） | ❌ |
| Cursor | sqlite（代码量）+ 账号级 token（联网）+ 会话 transcript（行为统计） | ⚠️ 账号级（手动刷新） | ❌ |
| cursor-agent | 会话与 IDE 共用 `~/.cursor/chats` + `agent-transcripts`；token 仅无头 stdout（需包装落盘到 `~/.cursor-agent-usage`） | ⚠️（仅包装） | ❌ |
| copilot | jsonl `~/.copilot/session-state/<id>/events.jsonl` | ✅（仅会话结束时，按模型累计） | ❌ |
| amp | 本机仅配置 | ❌（云端） | ❌ |

以上是各 Source 的默认扫描路径；每个 Source 都可以用环境变量整体覆盖（逗号分隔可指定多个目录，同时扫描），用于非默认安装位置或多份数据目录。默认路径与对应环境变量见 `docs/adr/0005-configurable-source-paths.md`。Claude Code 默认会同时扫 `~/.claude/projects` 和 XDG 路径 `~/.config/claude/projects`。Cursor 账号用量见 `docs/adr/0006-cursor-account-usage-network-ingest.md`，Cursor 会话见 `docs/adr/0007-cursor-session-local-ingest.md`。
