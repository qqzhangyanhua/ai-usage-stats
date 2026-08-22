# 官方额度字段探测

只记账号级限额字段位置，不写会话正文。

## Claude

首选 `GET https://api.anthropic.com/api/oauth/usage`（零配置），失败才回落下面的 statusline 捕获。

请求头缺一不可：`Authorization: Bearer <accessToken>`、`anthropic-beta: oauth-2025-04-20`、`User-Agent: claude-code/<版本>`、`Accept`/`Content-Type: application/json`。

凭证读 `~/.claude/.credentials.json` 的 `claudeAiOauth`：

- `accessToken` + `expiresAt`（毫秒）。**`expiresAt` 为 0 或缺失不当成过期**——第三方代理会写 0，交给接口判。
- 必须有 `user:profile` scope；`claude setup-token` 生成的纯推理 token 没有，接口会拒，先本地筛掉。
- 只读不刷新：刷新会把新 token 写回第三方文件，违反 ADR 0010。过期就提示打开一次 Claude Code。
- macOS 上 Claude Code 以钥匙串为准、文件为镜像；当前只读文件。

响应：

- `five_hour` / `seven_day` / `seven_day_sonnet` → `utilization`（0–100）+ `resets_at`
- `limits[]` 里 `kind == "weekly_scoped"` 的条目 → `percent` + `scope.model.display_name`，按模型拆的周窗口。老的 `seven_day_<model>` 顶层键现在返回 null，模型名不写死。
- `resets_at` 既可能是 ISO 字符串也可能是 epoch 秒
- `extra_usage.{is_enabled,used_credits,monthly_limit}`（分），当前不采

429 限流较紧，提示里要劝阻手动狂刷。

## Claude statusline stdin

Claude Code 2.1.80+ 在 statusline 命令的 stdin JSON 里提供：

- `rate_limits.five_hour.used_percentage`（0–100；偶发泄漏 `resets_at` 的 epoch，需丢弃 >100）
- `rate_limits.five_hour.resets_at`（Unix 秒或 ISO）
- `rate_limits.seven_day.used_percentage`
- `rate_limits.seven_day.resets_at`

捕获文件：应用数据目录 `claude_statusline.json`。

## Codex

首选 `GET https://chatgpt.com/backend-api/wham/usage`（不依赖 CLI 装没装），失败才回落下面的 app-server。

凭证读 `~/.codex/auth.json`（`CODEX_HOME` 可覆盖）的 `tokens.{access_token, account_id}`：

- 请求头：`Authorization: Bearer`、`Accept: application/json`、`User-Agent`，有 `account_id` 时加 `ChatGPT-Account-Id`。
- **只有 `OPENAI_API_KEY`、没有 `tokens` 的账号是按量计费，没有额度百分比**，直接判定不可用，别报解析错误。
- 只读不刷新（ADR 0010）。

响应 `rate_limit.{primary_window, secondary_window}`，每个窗口：

- `used_percent`（0–100）；缺了就取响应头 `x-codex-primary-used-percent` / `x-codex-secondary-used-percent`
- `limit_window_seconds` 决定窗口种类——**不能按 primary/secondary 的位置认**，Codex 会把临时只剩一条的周限额挪进 primary 槽。18000 → 5 小时，604800 → 7 天，其它按小时数命名。
- `reset_at`（epoch 秒）或 `reset_after_seconds`（相对量，要按当前时间换算）

`plan_type`、`rate_limit_reset_credits` 当前不采。

## Codex app-server

一次性启动 `codex app-server`，`initialize` → `initialized` → `account/rateLimits/read`。

- `result.rateLimits.primary.usedPercent`
- `result.rateLimits.primary.windowDurationMins`
- `result.rateLimits.primary.resetsAt`
- 若有 `rateLimitsByLimitId`，按 bucket 展开 primary/secondary

进程不在或超时：该行 `unavailable`，不影响另外三路。

## Cursor

凭证只有一个来源：本机 Cursor 客户端。没有手动粘贴通路，也不落钥匙串。`state.vscdb`（Win `%APPDATA%\Cursor\User\globalStorage`、mac `~/Library/Application Support/Cursor/...`、Linux `~/.config/Cursor/...`，三平台都在 `dirs::config_dir()` 下）的 `ItemTable`：

- `cursorAuth/accessToken`：WorkOS JWT，`iss=https://authentication.cursor.sh`，`sub` 形如 `google-oauth|user_01J…`。value 列可能是 TEXT 也可能是 BLOB。
- `cursorAuth/cachedEmail` / `cursorAuth/stripeMembershipType`：只用于设置页展示。
- cookie 值 = `<sub 里 "|" 之后那段>` + `%3A%3A` + `<jwt>`，即 `WorkosCursorSessionToken`。
- 过期判断用 JWT 的 `exp`（留 60s 容差）。

必须原地只读打开：库有几百 MB，且 `immutable=1` / 复制会跳过 WAL 读到陈旧值。

拿到 token 后 `GET https://cursor.com/api/usage-summary`。

Cursor 订阅限额是多档并行，不能只取总量：

- `individualUsage.plan.totalPercentUsed`（或 `used` / `limit`）→ 窗口 `billing_cycle` / 总量
- `individualUsage.plan.autoPercentUsed` → 窗口 `auto` / Auto
- `individualUsage.plan.apiPercentUsed` → 窗口 `api` / API
- `individualUsage.onDemand.used` / `limit`（无 limit 时回退 `teamUsage.onDemand`）→ 窗口 `on_demand` / 按需
- `billingCycleEnd`

与账号用量事件接口 `get-filtered-usage-events` 分开。结构变更时保留上次正确缓存。

## Antigravity

凭证读本机 Antigravity 的 `state.vscdb`（和 Cursor 同样落在 `dirs::config_dir()` 下，Win 是 `%APPDATA%\Antigravity\User\globalStorage`）的 `ItemTable`：

- `antigravityAuthStatus`：JSON，`apiKey` 是 Google OAuth access token（`ya29.`），只活约 1 小时，**基本总是过期，不能直接用**。
- `antigravityUnifiedStateSync.oauthToken`：嵌套 protobuf（外层 base64 → protobuf → 内层 base64 → protobuf），内层含 access token、`Bearer`、**refresh token（`1//` 开头）**。字段号不稳定，按形状找；内层 base64 的 padding 未必齐，要按无 padding 解。
- 刷新要用 Antigravity 自己的 OAuth 客户端。**不内嵌到本仓库**——那是 Google 发给 Antigravity 的凭证，GitHub 的 secret scanning 也会拦。改成运行时从本机安装的 `out/main.js` 里提取（先顺 PATH 上的 `antigravity` 启动器反查安装根目录，再退回各平台默认位置），拿去 `https://oauth2.googleapis.com/token` 换 access token。`main.js` 里 id 和 secret 各有多个且配对关系看不出来，全组合都试，错配会快速返回 `invalid_client`。
- 先用 `antigravityAuthStatus.apiKey` 直接打，401 了才走上面的刷新，省一次往返。

`POST https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary`，body `{}`（有 project 就传 `{"project": pid}`）：

- **RPC 名是 `retrieveUserQuotaSummary`，不是 `retrieveUserQuota`**；后者对消费级账号一律 403。
- **`User-Agent` 必须带 `Antigravity/` 标记**，否则 403「no valid license」。实测 `vscode/1.X.X (Antigravity/4.3.0)`、`…(Antigravity/0.0.0)`、`Antigravity/4.3.0` 都通，`vscode/1.X.X` 和其它 UA 都 403 —— 只认标记，不认版本号。那个 403 是 UA 门禁，不是真的没 license。
- 响应 `groups[].buckets[]`：`bucketId`（→ 窗口 kind，`-` 换 `_`）、`window`（`weekly` / `5h`）、`remainingFraction`（**剩余**，`(1-x)*100` 才是已用）、`resetTime`。
- 桶的 `displayName` 是「Weekly Limit Remaining」这种剩余口径，直接展示会和已用读反，所以按 `window` 自己起名，group 的 `displayName` 做前缀。
- 端点按 prod → daily → sandbox 兜底；401/403 不换环境直接结束。

`v1internal:fetchAvailableModels` 也能拿到每个模型的 `quotaInfo.{remainingFraction, resetTime}`，是同一个 5h 桶的数字，当前不采。

## Droid (Factory)

凭证读本机 `~/.factory`（`FACTORY_HOME_OVERRIDE` 可覆盖）：

- `auth.v2.file`：`base64(iv):base64(tag):base64(密文)`，AES-256-GCM，**iv 是 16 字节**（不是常见的 12），tag 单独一段而不是拼在密文尾。
- `auth.v2.key`：明文放在旁边的 base64 32 字节密钥。
- 解出 `{access_token, refresh_token, active_organization_id}`；`access_token` 是 WorkOS JWT（`iss=https://api.workos.com`）。
- 旧版 `auth.json` 是明文同结构，作为兜底。macOS 上 droid 可能改用系统钥匙串，那种情况读不到，该行 `unavailable`。

`GET https://api.factory.ai/api/billing/limits`，`Authorization: Bearer <access_token>`：

- `limits.standard.{fiveHour,weekly,monthly}` → 窗口 `five_hour` / `weekly` / `monthly`，标签「标准 …」
- `limits.core.{fiveHour,weekly,monthly}` → 窗口 `core_*`，标签「Core …」（Droid Core 池）
- 每档 `usedPercent`（0–100）、`windowEnd`（ISO，→ `resets_at`）、`secondsRemaining`
- 另有 `extraUsageBalanceCents` / `overagePreference` / `usesTokenRateLimitsBilling`，当前不采

`windowEnd` 已过去的档位跳过——对齐 droid 自己的显示逻辑（过期窗说明该桶不在计费窗内，不等于 0%）。全部过期时报结构异常，保留上次正确缓存。

EU 区是 `https://api.eu.factory.ai`，当前不自动识别。

## Grok CLI-proxy billing

读取本机 `~/.grok/auth.json`（`GROK_HOME` 可覆盖）里未过期的会话 token。优先 `https://auth.x.ai…` 作用域，其次 `https://accounts.x.ai/sign-in`。token 字段为 `key`（兼容 `access_token`）。跳过 `web_login` 与纯 API key（`xai::api_key` / `auth_mode=api_key`）。

请求头对齐官方 CLI：`Authorization: Bearer <token>`、`X-XAI-Token-Auth: xai-grok-cli`、`x-userid`（`auth.json` 的 `user_id`，缺则先 `GET /v1/user`）、`x-grok-client-version`（`~/.grok/.metadata_version`，否则 1.0.5）、`x-grok-client-mode: interactive`。

REST `?format=credits` 对部分账号会 500（`Failed to serialize billing response`）。此时回落到 `POST https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig`（空 gRPC-web 帧 + 同一套 Bearer），只取周额度百分比和重置时间。

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
