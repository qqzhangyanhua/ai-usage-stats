# 本机 AI 用量统计 (AI Usage Stats)

扫描本机各 AI 编程 CLI 的会话数据，归一成「消耗记录」并展示 token 消耗与可选费用。

## 启动

```bash
npm install
npm run tauri dev
```

开发时会弹出原生窗口，标题为「本机 AI 用量统计」。

打包：

```bash
npm run tauri build
```

产物为可双击运行的 `.app`（macOS）。

## 技术栈

- Tauri 2 + Rust 核心（适配器、摄取、sqlite 缓存、聚合）
- React + Vite + ECharts 界面

详见 `CONTEXT.md`、`docs/adr/`、`docs/probe/token-fields.md`。

复跑本机 token 字段探测：

```bash
cargo run --bin probe --manifest-path src-tauri/Cargo.toml
```
