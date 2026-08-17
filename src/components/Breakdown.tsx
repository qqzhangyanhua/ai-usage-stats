import { memo, useCallback, useMemo } from "react";
import type { IconName } from "../icons";
import { breakdownBarOption } from "../lib/chartTheme";
import type { ResolvedTheme } from "../hooks/useTheme";
import { formatCost, formatTokens, projectLabel, providerChannel } from "../lib/format";
import type { NamedAmount } from "../types";
import { EmptyState } from "./EmptyState";
import { ExportableChart } from "./ExportableChart";
import { ExportButton } from "./ExportButton";
import { KpiCard } from "./Kpi";
import { ModelLabel } from "./VendorIcon";

function channelClass(channel: string): string {
  if (channel === "官方") return "official";
  if (channel === "中转") return "relay";
  return "unlabeled";
}

export const Breakdown = memo(function Breakdown({
  title,
  icon,
  rows,
  showProviderChannel,
  showVendorIcon,
  projectNames,
  theme,
}: {
  title: string;
  icon: IconName;
  rows: NamedAmount[];
  showProviderChannel?: boolean;
  showVendorIcon?: boolean;
  projectNames?: boolean;
  theme: ResolvedTheme;
}) {
  const label = useCallback(
    (row: NamedAmount): string => {
      const name = projectNames ? projectLabel(row.name) : row.name;
      return showProviderChannel ? `${name}（${providerChannel(row.name)}）` : name;
    },
    [projectNames, showProviderChannel],
  );

  const option = useMemo(() => {
    const labels = rows.map(label).reverse();
    const values = rows.map((row) => row.total_tokens).reverse();
    return breakdownBarOption(labels, values, theme);
  }, [rows, label, theme]);

  const stats = useMemo(() => {
    const totalTokens = rows.reduce((sum, row) => sum + row.total_tokens, 0);
    const unpricedCount = rows.filter((row) => row.unpriced).length;
    const totalCost = rows.reduce((sum, row) => sum + (row.cost ?? 0), 0);
    const hasCost = rows.some((row) => row.cost != null);
    return { totalTokens, unpricedCount, totalCost, hasCost };
  }, [rows]);

  const top = rows.slice(0, 6);
  const maxTotal = Math.max(1, ...rows.map((row) => row.total_tokens));

  return (
    <div className="stack">
      <section className="kpi-row">
        <KpiCard icon={icon} tone="purple" label="覆盖条目" value={formatTokens(rows.length)} />
        <KpiCard
          icon="tokens"
          tone="cyan"
          label="合计 Token"
          value={formatTokens(stats.totalTokens)}
        />
        <KpiCard
          icon="cost"
          tone="orange"
          label="合计费用"
          value={stats.hasCost ? `$${stats.totalCost.toFixed(2)}` : "—"}
        />
        <KpiCard icon="filter" tone="blue" label="单价未配置" value={`${stats.unpricedCount} 项`} />
      </section>

      {top.length > 0 ? (
        <div className="panel">
          <div className="panel-head">
            <h2>Top {top.length}</h2>
            <span className="muted">按 Token 用量排序</span>
          </div>
          <ol className="rank-list">
            {top.map((row, index) => (
              <li key={row.name}>
                <span className="rank">{index + 1}</span>
                <span className="rank-name" title={row.name}>
                  {showVendorIcon ? (
                    <ModelLabel name={row.name} fallback={label(row)} />
                  ) : (
                    label(row)
                  )}
                </span>
                <span className="rank-bar">
                  <i style={{ width: `${(row.total_tokens / maxTotal) * 100}%` }} />
                </span>
                <span className="rank-val">{formatTokens(row.total_tokens)}</span>
              </li>
            ))}
          </ol>
        </div>
      ) : null}

      <div className="panel">
        <div className="panel-head">
          <h2>{title}</h2>
        </div>
        <ExportableChart option={option} style={{ height: 360 }} filename={`${title}图`} />
      </div>
      <div className="panel">
        <div className="panel-head">
          <h2>明细列表</h2>
          <ExportButton
            filename={title}
            headers={["名称", ...(showProviderChannel ? ["渠道"] : []), "占比", "Token", "费用"]}
            rows={rows.map((row) => [
              projectNames ? projectLabel(row.name) : row.name,
              ...(showProviderChannel ? [providerChannel(row.name)] : []),
              `${(row.share * 100).toFixed(1)}%`,
              row.total_tokens,
              row.cost ?? "",
            ])}
          />
        </div>
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>名称</th>
                {showProviderChannel ? <th>渠道</th> : null}
                <th>占比</th>
                <th>Token</th>
                <th>费用</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.name}>
                  <td title={row.name}>
                    {showVendorIcon ? (
                      <ModelLabel
                        name={row.name}
                        fallback={projectNames ? projectLabel(row.name) : row.name}
                      />
                    ) : projectNames ? (
                      projectLabel(row.name)
                    ) : (
                      row.name
                    )}
                  </td>
                  {showProviderChannel ? (
                    <td>
                      <span className={`channel-badge ${channelClass(providerChannel(row.name))}`}>
                        {providerChannel(row.name)}
                      </span>
                    </td>
                  ) : null}
                  <td>
                    <span className="cell-bar">
                      <i style={{ width: `${row.share * 100}%` }} />
                    </span>
                    <span className="cell-bar-label">{(row.share * 100).toFixed(1)}%</span>
                  </td>
                  <td>{formatTokens(row.total_tokens)}</td>
                  <td>
                    {formatCost(row.cost, row.unpriced)}
                    {row.unpriced ? " · 单价未配置" : ""}
                  </td>
                </tr>
              ))}
              {rows.length === 0 ? (
                <tr>
                  <td colSpan={showProviderChannel ? 5 : 4} className="analytics-empty">
                    <EmptyState icon={icon} title="当前筛选条件下暂无数据" />
                  </td>
                </tr>
              ) : null}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
});
