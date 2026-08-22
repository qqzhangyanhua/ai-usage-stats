# 码表 (Mabiao)

扫描本机各 AI 编程 CLI 的会话数据，归一成「消耗记录」并展示 token 消耗与可选费用。

## 下载安装

安装包由 GitHub Actions 打好后挂在 [Releases](https://github.com/qqzhangyanhua/mabiao/releases)（首次发版为 draft，发布后即可下载）：

| 平台 | 产物 |
|------|------|
| macOS Apple Silicon | `.dmg`（`aarch64-apple-darwin`） |
| macOS Intel | `.dmg`（`x86_64-apple-darwin`） |
| Linux x64 | `.deb` 或 AppImage |
| Windows x64 | NSIS `.exe` |

当前构建**未做代码签名**。macOS 首次打开若提示无法验证开发者，在访达中右键 → 打开，或：

```bash
xattr -cr "/Applications/Mabiao.app"
```

Windows 可能被 SmartScreen 拦截，选择「仍要运行」即可。发版步骤与产物说明见 [`docs/platforms.md`](docs/platforms.md)。

## 主要统计

- 总览、时间趋势（支持按时/日/周/月切换粒度），以及按应用、模型、Provider、项目和会话拆分
- 总览按来源展示 5 小时计费窗与燃烧速率（由本地时间戳估计，非官方配额）
- 总览按来源展示 7 天滚动用量（累计 Token/费用与日均值，贴近周度限额心智模型，非官方配额；Cursor 账号用量单独成行，缺价时用 LiteLLM 快照估算）
- 总览独立展示 Claude / Codex / Cursor / Grok 官方额度（账号级已用百分比与重置时间，带新鲜度；与本机估计窗分开）
- 官方额度达到 80% / 100% 时各弹一次系统通知（按窗口重置去重，过期数据不弹）；菜单栏在今日花费旁显示最紧的官方百分比
- 总览模块（指标、官方额度、计费窗、滚动用量、趋势、热力图、明细等）可在首页或设置页开关；额度区块内的 Codex、Cursor Agent 等来源也可单独配置显示
- 应用趋势堆叠图：按天、周、月比较 Claude Code、Codex、Droid 等应用
- 应用 × 项目交叉统计：查看每个项目在各应用中的 Token 分布
- 应用效率指标：
  - 缓存命中率（近似）= 缓存读 Token ÷（输入 Token + 缓存读 Token）
  - 平均会话 Token = 总 Token ÷ 按“来源 + 会话 ID”去重后的会话数
  - 推理占比 = 推理 Token ÷ 总 Token
- Cursor 代码量使用独立口径，不并入 Token 统计
- 设置页提供数据源健康检查、单来源重建和全部缓存重建
- 设置页支持配置月度预算（美元），本月费用达到 50% / 80% / 100% 时各弹一次系统通知（按自然月重置，仅本地估算）
- 设置页可备份/恢复本机用量缓存（sqlite）、单价表、月度预算、官方额度配置、通知状态和 LiteLLM 价目快照（不含 Cursor 钥匙串）
- 总览、应用分析、会话明细、Cursor 等面板支持导出 CSV/JSON；图表可另存为图片
- 摄取遇到损坏 JSONL 时保留上次正确缓存，并报告部分成功
- 源文件被工具自身清理或轮转后，对应记录转为「已归档」但仍计入统计，不会静默消失；设置页可按来源或全部显式永久删除归档
- 菜单栏显示今日花费，若有官方额度则附带最紧的已用百分比；关闭窗口后应用继续在菜单栏运行，点菜单可打开主窗口或退出
- 托盘心跳仅在源文件（含 sidecar）、Cursor 会话 transcript 或代码量 sqlite 有变化时全量摄取，无变化则只刷新今日花费展示

各来源默认路径、本机 token / 费用能力见 [`CONTEXT.md`](CONTEXT.md)。

## 数据范围

- 默认只读扫描本机各来源会话目录，**不上传本机消耗记录**
- 聚合结果缓存在本机 sqlite，可在设置页备份、恢复或重建
- Cursor 账号用量与部分官方额度需要你主动提供本机已有凭证；不会改写会话正文
- Cursor 会话 token 目前写入 macOS 钥匙串（`keyring` 仅启用 `apple-native`），Windows / Linux 打包后该入口可能不可用

## 从源码启动

```bash
pnpm install
pnpm tauri dev
```

开发时会弹出原生窗口，标题为「码表」。本地打包：

```bash
pnpm tauri build
```

macOS 产物为可双击的 `.app` / `.dmg`。Linux / Windows 的依赖与产物见 [`docs/platforms.md`](docs/platforms.md)。

## 技术栈

- Tauri 2 + Rust 核心（适配器、摄取、sqlite 缓存、聚合）
- React + Vite + ECharts 界面
- 包管理统一使用 `pnpm`（请勿使用 npm/yarn，避免产生多份 lockfile）

## 文档地图

| 文档 | 内容 |
|------|------|
| [`CONTEXT.md`](CONTEXT.md) | 领域词汇、各来源采集现状 |
| [`docs/platforms.md`](docs/platforms.md) | 跨平台构建、GitHub 打包与发版 |
| [`docs/adr/`](docs/adr/) | 架构决策 |
| [`AGENTS.md`](AGENTS.md) | Cloud Agent / CI 怎么测 |
| [`docs/probe/`](docs/probe/) | 本机字段探测记录 |

## 开发脚本

```bash
pnpm lint       # ESLint 检查
pnpm lint:fix   # 自动修复
pnpm format     # Prettier 格式化
pnpm test       # Vitest（src/lib 纯函数）
pnpm build      # tsc + vite build
```

复跑本机 token 字段探测：

```bash
cargo run --bin probe --manifest-path src-tauri/Cargo.toml
```
