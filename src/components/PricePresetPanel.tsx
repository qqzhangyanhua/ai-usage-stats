import { useState } from "react";
import { mergePricePresets } from "../lib/priceImport";
import {
  groupPresetsByProvider,
  matchObservedModel,
  PRICE_PRESETS,
} from "../lib/pricePresets";
import type { PriceTable } from "../types";
import { VendorIcon } from "./VendorIcon";
import { Button } from "./ui/Button";

export function PricePresetPanel({
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
    const result = mergePricePresets(prices.prices, chosen, observedModels);
    if (result.additions.length > 0) {
      onChange({ prices: [...prices.prices, ...result.additions] });
    }
    setChecked({});
    if (result.message) {
      setMessage(result.message);
    }
  }

  return (
    <section className="panel" id="settings-presets">
      <div className="panel-head">
        <div>
          <h2>从预设导入单价</h2>
          <p className="panel-note">
            价格来自各官网公开信息，仅作配置起点，请以官方最新价目为准；能匹配到本地已出现的模型名时会自动带入。
          </p>
        </div>
        <Button variant="accent" disabled={selectedCount === 0} onClick={importSelected}>
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
