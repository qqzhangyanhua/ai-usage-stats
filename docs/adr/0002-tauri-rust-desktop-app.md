# 桌面端采用 Tauri（Rust 核心 + webview 界面）

需要一个双击即用的原生桌面 App，同时要展示 8 类图表视图（趋势、按工具/模型/provider/项目、Top 会话、代码量等），并高效解析大量异构本地数据（124MB 的 opencode.db、成百上千个 jsonl、zstd 压缩会话）。

**决定**：使用 Tauri。核心逻辑（适配器、摄取、sqlite 缓存、聚合查询）用 Rust 实现；界面在系统 webview 中用 HTML + ECharts 渲染。

**理由**：Rust 保证解析大文件的性能与单一原生产物；webview + ECharts 以最低成本拿到丰富图表，避免纯 Rust GUI（egui）在复杂图表与布局上的短板。

## Consequences
- 引入 Rust/Tauri 工具链与前端构建；相较此前设想的 Python web 方案，构建复杂度上升，但换来原生 App 体验与性能。
- Rust 侧依赖：`rusqlite`(缓存)、`serde_json`(jsonl)、`zstd`(dsh 解压) 等。
- 前后端通过 Tauri command / IPC 交换聚合后的 JSON，webview 只做展示。
