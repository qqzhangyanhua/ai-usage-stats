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

进程不在或超时：该行 `unavailable`，不影响另外三路。

## Cursor

复用钥匙串 `WorkosCursorSessionToken`，`GET https://cursor.com/api/usage-summary`。

Cursor 订阅限额是多档并行，不能只取总量：

- `individualUsage.plan.totalPercentUsed`（或 `used` / `limit`）→ 窗口 `billing_cycle` / 总量
- `individualUsage.plan.autoPercentUsed` → 窗口 `auto` / Auto
- `individualUsage.plan.apiPercentUsed` → 窗口 `api` / API
- `individualUsage.onDemand.used` / `limit`（无 limit 时回退 `teamUsage.onDemand`）→ 窗口 `on_demand` / 按需
- `billingCycleEnd`

与账号用量事件接口 `get-filtered-usage-events` 分开。结构变更时保留上次正确缓存。

## Grok CLI-proxy billing

读取本机 `~/.grok/auth.json`（`GROK_HOME` 可覆盖）里未过期的会话 token。优先 `https://auth.x.ai…` 作用域，其次 `https://accounts.x.ai/sign-in`。token 字段为 `key`（兼容 `access_token`）。跳过 `web_login` 与纯 API key（`xai::api_key` / `auth_mode=api_key`）。

请求头：`Authorization: Bearer <token>`、`X-XAI-Token-Auth: xai-grok-cli`。

- `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits`
  - `config.creditUsagePercent`（0–100）→ 窗口 `weekly` / 周额度
  - 缺百分比但 `currentPeriod.type=USAGE_PERIOD_TYPE_WEEKLY` 且有 `end` → 周额度 0%
  - 无周百分比时回退 `productUsage[GrokBuild].usagePercent`
  - 同时有周百分比时另开窗口 `product_grokbuild` / Grok Build
  - `config.onDemandUsed.val` / `onDemandCap.val`（cap > 0）→ 窗口 `on_demand` / 按需
  - 重置时间：`config.currentPeriod.end`，其次 `billingPeriodEnd`
- `GET https://cli-chat-proxy.grok.com/v1/billing`（失败不影响周额度）
  - `used` / `monthlyLimit`（或 `usage.totalUsed`，支持 `{val}` 包装）→ 窗口 `monthly` / 月额度
  - 缺 `used` 不当成 0%

文件缺失、过期或结构变更：该行 `unavailable`，保留上次正确缓存。不把 token 或 billing 原文写入日志。
