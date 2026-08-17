import { useState } from "react";
import { applicationLabel, formatTokens } from "../lib/format";
import { Button } from "./ui/Button";
import { Field } from "./ui/Field";
import {
  groupPresetsByProvider,
  matchObservedModel,
  presetToPriceEntry,
  PRICE_PRESETS,
} from "../lib/pricePresets";
import type { IngestReport, PriceEntry, PriceTable, SourceDiagnostic } from "../types";
import { VendorIcon } from "./VendorIcon";

export function Settings({
  prices,
  diagnostics,
  ingestReport,
  rebuilding,
  operationBusy,
  observedModels,
  onChange,
  onSave,
  onRebuild,
}: {
  prices: PriceTable;
  diagnostics: SourceDiagnostic[];
  ingestReport: IngestReport | null;
  rebuilding: string | null;
  operationBusy: boolean;
  observedModels: string[];
  onChange: (prices: PriceTable) => void;
  onSave: () => void;
  onRebuild: (source: string | null) => void;
}) {
  function update(index: number, patch: Partial<PriceEntry>) {
    const next = prices.prices.map((row, i) => (i === index ? { ...row, ...patch } : row));
    onChange({ prices: next });
  }

  return (
    <div className="stack">
      <section className="panel">
        <div className="panel-head">
          <div>
            <h2>数据源健康</h2>
            <p className="panel-note">
              只展示扫描状态和用量元数据，不读取或保存会话正文。关闭窗口后应用会留在菜单栏，显示今日花费。
            </p>
          </div>
          <Button disabled={operationBusy || rebuilding !== null} onClick={() => onRebuild(null)}>
            {rebuilding === "all" ? "正在重建…" : "重建全部缓存"}
          </Button>
        </div>
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>应用</th>
                <th>状态</th>
                <th>统计口径</th>
                <th>缓存文件</th>
                <th>记录</th>
                <th>Token</th>
                <th>扫描位置</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {diagnostics.map((row) => (
                <tr key={row.source}>
                  <td>
                    <strong>{row.application || applicationLabel(row.source)}</strong>
                  </td>
                  <td>
                    <span className={row.detected ? "health-state ok" : "health-state"}>
                      {row.detected ? "已检测" : "未检测"}
                    </span>
                  </td>
                  <td>{row.coverage}</td>
                  <td>{formatTokens(row.cached_files)}</td>
                  <td>{formatTokens(row.record_count)}</td>
                  <td>{formatTokens(row.total_tokens)}</td>
                  <td className="mono" title={row.root_path}>
                    {row.root_path}
                  </td>
                  <td>
                    <Button
                      disabled={operationBusy || rebuilding !== null || !row.detected}
                      onClick={() => onRebuild(row.source)}
                    >
                      {rebuilding === row.source ? "重建中…" : "重建"}
                    </Button>
                  </td>
                </tr>
              ))}
              {diagnostics.length === 0 ? (
                <tr>
                  <td colSpan={8} className="analytics-empty">
                    正在读取来源状态…
                  </td>
                </tr>
              ) : null}
            </tbody>
          </table>
        </div>
        {ingestReport && ingestReport.issues.length > 0 ? (
          <div className="ingest-issues" role="status">
            <strong>本次摄取有 {ingestReport.issues.length} 个文件保留了上次正确缓存</strong>
            <ul>
              {ingestReport.issues.slice(0, 8).map((issue, index) => (
                <li key={`${issue.source}-${issue.path}-${index}`}>
                  <span>{applicationLabel(issue.source)}</span>
                  <code title={issue.path}>{issue.path}</code>
                  <em>{issue.message}</em>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </section>

      <PricePresetPanel prices={prices} observedModels={observedModels} onChange={onChange} />

      <section className="panel">
        <div className="panel-head">
          <div>
            <h2>单价配置</h2>
            <p className="panel-note">当前单价按每 Token 美元计算；后续将迁移为 USD / 1M Token。</p>
          </div>
          <div className="row-actions">
            <Button
              onClick={() =>
                onChange({
                  prices: [
                    ...prices.prices,
                    {
                      model: "",
                      provider: null,
                      input: 0,
                      output: 0,
                      cache_read: 0,
                      cache_creation: 0,
                    },
                  ],
                })
              }
            >
              新增
            </Button>
            <Button onClick={onSave}>保存</Button>
          </div>
        </div>
        {prices.prices.map((row, index) => (
          <div className="price-row" key={index}>
            <Field
              label="模型"
              placeholder="模型名"
              value={row.model}
              onChange={(event) => update(index, { model: event.target.value })}
            />
            <Field
              label="Provider"
              placeholder="可空"
              value={row.provider ?? ""}
              onChange={(event) => update(index, { provider: event.target.value || null })}
            />
            <Field
              label="输入"
              type="number"
              min="0"
              step="any"
              value={row.input}
              onChange={(event) => update(index, { input: Number(event.target.value) })}
            />
            <Field
              label="输出"
              type="number"
              min="0"
              step="any"
              value={row.output}
              onChange={(event) => update(index, { output: Number(event.target.value) })}
            />
            <Field
              label="缓存读"
              type="number"
              min="0"
              step="any"
              value={row.cache_read}
              onChange={(event) => update(index, { cache_read: Number(event.target.value) })}
            />
            <Field
              label="缓存写"
              type="number"
              min="0"
              step="any"
              value={row.cache_creation}
              onChange={(event) => update(index, { cache_creation: Number(event.target.value) })}
            />
            <Button
              variant="danger"
              className="price-row-delete"
              onClick={() => onChange({ prices: prices.prices.filter((_, i) => i !== index) })}
            >
              删除
            </Button>
          </div>
        ))}
      </section>
    </div>
  );
}

function PricePresetPanel({
  prices,
  observedModels,
  onChange,
}: {
  prices: PriceTable;
  observedModels: string[];
  onChange: (prices: PriceTable) => void;
}) {
  const [checked, setChecked] = useState<Record<string, boolean>>({});
  const [message, setMessage] = useState<string | null>(null);
  const groups = groupPresetsByProvider(PRICE_PRESETS);
  const selectedCount = Object.values(checked).filter(Boolean).length;

  function toggle(id: string) {
    setChecked((prev) => ({ ...prev, [id]: !prev[id] }));
    setMessage(null);
  }

  function importSelected() {
    const chosen = PRICE_PRESETS.filter((preset) => checked[preset.id]);
    if (chosen.length === 0) {
      return;
    }
    const existing = new Set(prices.prices.map((row) => `${row.model}::${row.provider ?? ""}`));
    const additions: PriceEntry[] = [];
    let skipped = 0;
    for (const preset of chosen) {
      const matched = matchObservedModel(preset, observedModels);
      const entry = presetToPriceEntry(preset, matched ?? undefined);
      const key = `${entry.model}::${entry.provider ?? ""}`;
      if (existing.has(key)) {
        skipped += 1;
        continue;
      }
      existing.add(key);
      additions.push(entry);
    }
    if (additions.length > 0) {
      onChange({ prices: [...prices.prices, ...additions] });
    }
    setChecked({});
    setMessage(
      additions.length > 0
        ? `已添加 ${additions.length} 条${skipped > 0 ? `，跳过 ${skipped} 条已存在` : ""}，别忘了点保存`
        : `未添加新条目（${skipped} 条已存在于当前单价表）`,
    );
  }

  return (
    <section className="panel">
      <div className="panel-head">
        <div>
          <h2>从预设导入单价</h2>
          <p className="panel-note">
            价格来自各官网公开信息，仅作配置起点，请以官方最新价目为准；能匹配到本地已出现的模型名时会自动带入。
          </p>
        </div>
        <Button disabled={selectedCount === 0} onClick={importSelected}>
          导入所选（{selectedCount}）
        </Button>
      </div>
      {message ? <p className="panel-note preset-message">{message}</p> : null}
      <div className="preset-groups">
        {groups.map(([providerLabel, presets]) => (
          <div className="preset-group" key={providerLabel}>
            <div className="preset-group-title">
              <VendorIcon name={providerLabel} size={14} />
              {providerLabel}
            </div>
            <div className="preset-list">
              {presets.map((preset) => {
                const matched = matchObservedModel(preset, observedModels);
                return (
                  <label className="preset-row" key={preset.id}>
                    <input
                      type="checkbox"
                      checked={checked[preset.id] ?? false}
                      onChange={() => toggle(preset.id)}
                    />
                    <span className="preset-name">{preset.displayName}</span>
                    <span className="preset-price">
                      ${preset.inputPerM} / ${preset.outputPerM} 每百万 Token（输入/输出）
                    </span>
                    {matched ? (
                      <span className="preset-detected" title={matched}>
                        已检测到：{matched}
                      </span>
                    ) : (
                      <span className="preset-missing">未检测到，将按 {preset.model} 添加</span>
                    )}
                  </label>
                );
              })}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
