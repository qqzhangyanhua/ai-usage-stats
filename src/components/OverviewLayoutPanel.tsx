import { defaultOverviewLayout, type OverviewLayout } from "../lib/overviewLayout";
import { OverviewLayoutControls } from "./OverviewLayoutControls";
import { Button } from "./ui/Button";

export function OverviewLayoutPanel({
  layout,
  detectedSources,
  onChange,
}: {
  layout: OverviewLayout;
  detectedSources: string[];
  onChange: (layout: OverviewLayout) => void;
}) {
  return (
    <section className="panel" id="settings-overview">
      <div className="panel-head">
        <div>
          <h2>概览显示</h2>
          <p className="panel-note">
            配置首页展示哪些模块，以及额度区块里显示哪些来源。偏好保存在本机，不影响统计缓存。
          </p>
        </div>
        <Button onClick={() => onChange(defaultOverviewLayout())}>恢复默认</Button>
      </div>
      <OverviewLayoutControls
        layout={layout}
        detectedSources={detectedSources}
        onChange={onChange}
      />
    </section>
  );
}
