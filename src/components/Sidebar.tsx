import { useEffect, useState } from "react";
import { Icon, type IconName } from "../icons";
import type { ThemeMode } from "../hooks/useTheme";
import type { View } from "../types";
import { Select } from "./ui/Select";

const SIDEBAR_COLLAPSED_KEY = "ai-usage-stats:sidebar-collapsed";

function loadCollapsed(): boolean {
  try {
    return window.localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === "1";
  } catch {
    return false;
  }
}

const nav: { id: View; label: string; icon: IconName }[] = [
  { id: "overview", label: "概览", icon: "overview" },
  { id: "trend", label: "使用统计", icon: "trend" },
  { id: "sessions", label: "会话管理", icon: "sessions" },
  { id: "model", label: "模型统计", icon: "model" },
  { id: "project", label: "项目统计", icon: "project" },
  { id: "application", label: "应用统计", icon: "source" },
  { id: "provider", label: "Provider", icon: "provider" },
  { id: "cursor", label: "Cursor 代码量", icon: "cursor" },
  { id: "settings", label: "设置", icon: "settings" },
];

export const AUTO_REFRESH_OPTIONS: { value: string; label: string }[] = [
  { value: "off", label: "关闭" },
  { value: "1", label: "每 1 分钟" },
  { value: "5", label: "每 5 分钟" },
  { value: "10", label: "每 10 分钟" },
  { value: "30", label: "每 30 分钟" },
  { value: "60", label: "每 1 小时" },
];

const THEME_OPTIONS: { value: ThemeMode; label: string; icon: IconName }[] = [
  { value: "system", label: "跟随系统", icon: "monitor" },
  { value: "light", label: "浅色", icon: "sun" },
  { value: "dark", label: "深色", icon: "moon" },
];

export function Sidebar({
  view,
  busy,
  connected,
  status,
  autoRefresh,
  themeMode,
  onNavigate,
  onAutoRefreshChange,
  onThemeModeChange,
}: {
  view: View;
  busy: boolean;
  connected: boolean;
  status: string;
  autoRefresh: string;
  themeMode: ThemeMode;
  onNavigate: (view: View) => void;
  onAutoRefreshChange: (value: string) => void;
  onThemeModeChange: (mode: ThemeMode) => void;
}) {
  const [collapsed, setCollapsed] = useState(loadCollapsed);

  useEffect(() => {
    try {
      window.localStorage.setItem(SIDEBAR_COLLAPSED_KEY, collapsed ? "1" : "0");
    } catch {
      // localStorage 不可用时忽略，仅影响下次启动是否记住折叠状态
    }
  }, [collapsed]);

  return (
    <aside className={collapsed ? "sidebar collapsed" : "sidebar"}>
      <div className="brand">
        <Icon name="logo" size={34} />
        <div className={collapsed ? "sr-only" : undefined}>
          <div className="brand-name">本机用量</div>
          <div className="brand-meta">
            Token 统计
            <span className="badge">本地</span>
          </div>
        </div>
      </div>
      <nav className="nav">
        {nav.map((item) => (
          <button
            key={item.id}
            className={view === item.id ? "nav-btn active" : "nav-btn"}
            disabled={busy}
            onClick={() => onNavigate(item.id)}
            title={collapsed ? item.label : undefined}
          >
            <Icon name={item.icon} size={16} />
            <span className={collapsed ? "sr-only" : undefined}>{item.label}</span>
          </button>
        ))}
      </nav>
      <button
        type="button"
        className="sidebar-collapse-btn"
        onClick={() => setCollapsed((value) => !value)}
        aria-pressed={collapsed}
        aria-label={collapsed ? "展开侧边栏" : "收起侧边栏"}
        title={collapsed ? "展开侧边栏" : "收起侧边栏"}
      >
        <Icon name="chevron" size={14} className={collapsed ? "flip" : undefined} />
        <span className={collapsed ? "sr-only" : undefined}>收起</span>
      </button>
      {!collapsed ? (
        <div className="sidebar-foot">
          <div className="theme-toggle" role="group" aria-label="外观主题">
            {THEME_OPTIONS.map((option) => (
              <button
                key={option.value}
                type="button"
                className={
                  themeMode === option.value ? "theme-toggle-btn active" : "theme-toggle-btn"
                }
                title={option.label}
                aria-label={option.label}
                aria-pressed={themeMode === option.value}
                onClick={() => onThemeModeChange(option.value)}
              >
                <Icon name={option.icon} size={14} />
              </button>
            ))}
          </div>
          <div className="auto-refresh">
            <Icon name="clock" size={14} />
            <span>自动刷新</span>
            <Select
              variant="plain"
              align="left"
              ariaLabel="自动刷新间隔"
              value={autoRefresh}
              options={AUTO_REFRESH_OPTIONS}
              onChange={onAutoRefreshChange}
            />
          </div>
          <div className="conn-card">
            <span className={connected ? "live-dot" : "live-dot off"} />
            <div>
              <div className="conn-title">
                {connected ? (busy ? "正在同步" : "连接正常") : "连接异常"}
              </div>
              <div className="conn-sub" title={status}>
                {status}
              </div>
            </div>
          </div>
          <div className="version">版本 0.1.0</div>
        </div>
      ) : (
        <div className="sidebar-foot collapsed-foot">
          <span
            className={connected ? "live-dot" : "live-dot off"}
            title={connected ? "连接正常" : "连接异常"}
          />
        </div>
      )}
    </aside>
  );
}

export function viewTitle(view: View): { title: string; subtitle: string } {
  switch (view) {
    case "overview":
      return { title: "概览", subtitle: "全局 Token 使用概览" };
    case "trend":
      return { title: "使用统计", subtitle: "按时间查看 Token 消耗" };
    case "sessions":
      return { title: "会话管理", subtitle: "按会话下钻每轮明细" };
    case "model":
      return { title: "模型统计", subtitle: "按模型拆分 Token 与费用" };
    case "project":
      return { title: "项目统计", subtitle: "按项目拆分 Token 与费用" };
    case "application":
      return { title: "应用统计", subtitle: "趋势、项目交叉与效率指标" };
    case "provider":
      return { title: "Provider", subtitle: "按官方 / 中转渠道拆分" };
    case "cursor":
      return { title: "Cursor 代码量", subtitle: "独立口径，不计入 Token" };
    case "settings":
      return { title: "设置", subtitle: "模型单价配置" };
  }
}
