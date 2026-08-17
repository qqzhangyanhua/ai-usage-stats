import type { ReactNode } from "react";
import { Spinner } from "./Spinner";

/**
 * 在保留旧数据可见的前提下，为局部区域叠加一层加载态遮罩。
 * 用于页面切换 / 筛选变化 / 分页搜索等异步刷新场景。
 */
export function LoadingOverlay({
  active,
  label = "加载中…",
  children,
  className,
}: {
  active: boolean;
  label?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={["loading-overlay-wrap", className].filter(Boolean).join(" ")}>
      {children}
      {active ? (
        <div className="loading-overlay" role="status" aria-live="polite">
          <Spinner size={20} />
          <span>{label}</span>
        </div>
      ) : null}
    </div>
  );
}
