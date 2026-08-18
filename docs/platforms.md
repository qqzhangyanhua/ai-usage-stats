# 跨平台构建与运行

本机 AI 用量统计基于 **Tauri 2**，核心逻辑跨平台；当前文档与默认打包流程以 **macOS** 为主（菜单栏托盘、`.app` 产物）。

## 支持矩阵

| 平台 | 构建 | 菜单栏托盘 | 说明 |
|------|------|------------|------|
| macOS | ✅ 主要目标 | ✅ | `pnpm tauri build` → `.app`；关闭窗口后托盘继续刷新今日花费 |
| Linux | ⚠️ CI 可编译 | ⚠️ 未专门适配 | CI 已安装 `webkit2gtk` 等依赖并跑 `cargo test`；GUI 托盘行为未验证 |
| Windows | ⚠️ 理论可编译 | ❌ | 无 `Reopen` / template icon 等待机逻辑；需本机验证 |

## macOS（推荐）

```bash
pnpm install
pnpm tauri dev      # 开发
pnpm tauri build    # 发布 .app
```

## Linux

依赖（与 CI 一致，Debian/Ubuntu 示例）：

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev patchelf
```

```bash
pnpm install
pnpm tauri build
```

产物格式取决于 Tauri bundle 配置（`.deb` / AppImage 等）。**托盘**：代码使用 Tauri tray API，Linux 上可能表现为状态栏图标，但未作为一等公民测试。

## Windows

需安装 [Tauri 前置依赖](https://v2.tauri.app/start/prerequisites/)（WebView2、VS Build Tools 等）。

```bash
pnpm install
pnpm tauri build
```

`main.rs` 在 release 下使用 `windows_subsystem = "windows"` 隐藏控制台窗口。无 macOS 专属 `Reopen` 处理，点击任务栏图标行为取决于系统默认。

## 开发约定

- 包管理：**pnpm**（`tauri.conf.json` 的 `beforeDevCommand` / `beforeBuildCommand` 亦使用 `pnpm run`）
- 新增平台相关 UI 时，用 `#[cfg(target_os = "...")]` 隔离，并在本文件更新支持矩阵
- Cloud Agent / CI 详见根目录 `AGENTS.md`
