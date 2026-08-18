# Cursor 会话走本机文件摄取、独立维度

Cursor Agent 对话在本机落盘为 `agent-transcripts` jsonl，但不含 token；行为统计（轮次、工具调用、失败率）与代码量、账号用量语义不同，不应并入消耗记录或总览 token KPI。

**决定**：新增独立维度「Cursor 会话 (Cursor Session)」。在 `ingest_all` 时扫描 `~/.cursor/projects/*/agent-transcripts/*/*.jsonl`，并从 `ai-code-tracking.db` 的 `ai_code_hashes` enrich 模型/文件/时间；解析为会话级聚合写入独立缓存表 `cursor_sessions`；只存聚合字段，不存对话正文。orphan hash 不单独造会话。

边界：

- **独立维度**：不进入 `UsageRecord`、`Source` 枚举或本机 token 聚合；界面 Sidebar 独立入口。
- **本机文件**：与 Cursor 账号用量（联网、手动 refresh）严格分离；不得扩散联网路径。
- **可信缓存**：文件 `(mtime_ms, size)` 指纹未变跳过重解析；解析失败保留旧缓存；有失败时跳过对账删除；删除 transcript 后对账清理。
- **摄取失败不 abort**：Cursor 会话问题记入 `IngestReport.issues`（source=`cursor-session`），不阻断其它 Source。
- **不参与 ADAPTER_VERSION**：独立 `cursor_session_files` / `cursor_session_meta` 表。

## Consequences

- 无 hash enrich 时模型为空、时间退化为文件 mtime；纯问答会话仍计入。
- 会话正文留在磁盘 jsonl，App 不索引；详情页、搜索不在本 ADR 范围。
