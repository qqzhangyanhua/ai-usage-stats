import { useState } from "react";
import { defaultOverviewLayout, type OverviewLayout } from "../lib/overviewLayout";
import { OverviewLayoutControls } from "./OverviewLayoutControls";
import { Button } from "./ui/Button";

export function OverviewLayoutBar({
  layout,
  onChange,
  onOpenSettings,
}: {
  layout: OverviewLayout;
  onChange: (layout: OverviewLayout) => void;
  onOpenSettings?: () => void;
}) {
  const [open, setOpen] = useState(false);

  return (
    <section className="overview-layout-bar">
      <div className="overview-layout-bar-head">
        <Button
          size="sm"
          aria-expanded={open}
          aria-controls="overview-layout-editor"
          onClick={() => setOpen((prev) => !prev)}
        >
          {open ? "收起显示配置" : "配置显示"}
        </Button>
        <span className="muted">选择首页模块，以及额度里的 Codex、Cursor 等来源</span>
        <div className="overview-layout-bar-actions">
          {open ? (
            <Button onClick={() => onChange(defaultOverviewLayout())}>恢复默认</Button>
          ) : null}
          {onOpenSettings ? (
            <Button variant="text" onClick={onOpenSettings}>
              打开设置
            </Button>
          ) : null}
        </div>
      </div>
      {open ? (
        <div id="overview-layout-editor" className="overview-layout-editor">
          <OverviewLayoutControls layout={layout} onChange={onChange} />
        </div>
      ) : null}
    </section>
  );
}
