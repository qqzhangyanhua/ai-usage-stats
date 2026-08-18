# 官方额度字段探测

只记账号级限额字段位置，不写会话正文。

## Claude statusline stdin

Claude Code 2.1.80+ 在 statusline 命令的 stdin JSON 里提供：

- `rate_limits.five_hour.used_percentage`（0–100；偶发泄漏 `resets_at` 的 epoch，需丢弃 >100）
- `rate_limits.five_hour.resets_at`（Unix 秒或 ISO）
- `rate_limits.seven_day.used_percentage`
- `rate_limits.seven_day.resets_at`

捕获文件：应用数据目录 `claude_statusline.json`。

## Codex app-server

一次性启动 `codex app-server`，`initialize` → `initialized` → `account/rateLimits/read`。

- `result.rateLimits.primary.usedPercent`
- `result.rateLimits.primary.windowDurationMins`
- `result.rateLimits.primary.resetsAt`
- 若有 `rateLimitsByLimitId`，按 bucket 展开 primary/secondary

进程不在或超时：该行 `unavailable`，不影响另外两路。

## Cursor

复用钥匙串 `WorkosCursorSessionToken`，`GET https://cursor.com/api/usage-summary`。

- `individualUsage.plan.totalPercentUsed`（或 `used` / `limit`）
- `billingCycleEnd`

与账号用量事件接口 `get-filtered-usage-events` 分开。结构变更时保留上次正确缓存。
