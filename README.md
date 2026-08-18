# 本机 AI 用量统计 (AI Usage Stats)

扫描本机各 AI 编程 CLI 的会话数据，归一成「消耗记录」并展示 token 消耗与可选费用。

## 主要统计

- 总览、时间趋势，以及按应用、模型、Provider、项目和会话拆分
- 总览按来源展示 5 小时计费窗与燃烧速率（由本地时间戳估计，非官方配额）
- 应用趋势堆叠图：按天、周、月比较 Claude Code、Codex、Droid 等应用
- 应用 × 项目交叉统计：查看每个项目在各应用中的 Token 分布
- 应用效率指标：
  - 缓存命中率（近似）= 缓存读 Token ÷（输入 Token + 缓存读 Token）
  - 平均会话 Token = 总 Token ÷ 按“来源 + 会话 ID”去重后的会话数
  - 推理占比 = 推理 Token ÷ 总 Token
- Cursor 代码量使用独立口径，不并入 Token 统计
- 设置页提供数据源健康检查、单来源重建和全部缓存重建
- 设置页支持配置月度预算（美元），本月费用达到 50% / 80% / 100% 时各弹一次系统通知（按自然月重置，仅本地估算）
- 摄取遇到损坏 JSONL 时保留上次正确缓存，并报告部分成功；源文件删除后自动清理对应缓存
- 菜单栏显示今日花费；关闭窗口后应用继续在菜单栏运行，点菜单可打开主窗口或退出

## 启动

```bash
pnpm install
pnpm tauri dev
```

开发时会弹出原生窗口，标题为「本机 AI 用量统计」。

打包：

```bash
pnpm tauri build
```

产物为可双击运行的 `.app`（macOS）。

## 技术栈

- Tauri 2 + Rust 核心（适配器、摄取、sqlite 缓存、聚合）
- React + Vite + ECharts 界面
- 包管理统一使用 `pnpm`（请勿使用 npm/yarn，避免产生多份 lockfile）

详见 `CONTEXT.md`、`docs/adr/`、`docs/probe/token-fields.md`。

## 开发脚本

```bash
pnpm lint       # ESLint 检查
pnpm lint:fix   # 自动修复
pnpm format     # Prettier 格式化
pnpm build      # tsc + vite build
```

复跑本机 token 字段探测：

```bash
cargo run --bin probe --manifest-path src-tauri/Cargo.toml
```
