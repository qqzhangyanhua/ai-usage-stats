import { useState, type ReactNode } from "react";
import { Icon, type IconName } from "../icons";
import { useAnchoredPanel } from "../hooks/useAnchoredPanel";
import { useDismissible } from "../hooks/useDismissible";
import {
  applicationLabel,
  customRangeFilter,
  formatRangeLabel,
  projectLabel,
  providerChannel,
} from "../lib/format";
import type { Filter, FilterOptions, View } from "../types";
import { viewTitle } from "./Sidebar";
import { Button } from "./ui/Button";
import { DatePicker } from "./ui/DatePicker";
import { Select } from "./ui/Select";
import { VendorIcon } from "./VendorIcon";

const RANGE_OPTIONS = [
  { value: "all", label: "全部历史" },
  { value: "7", label: "近 7 天" },
  { value: "30", label: "近 30 天" },
  { value: "custom", label: "自定义区间" },
];

export function Topbar({
  view,
  filter,
  preset,
  options,
  disabled,
  refreshDisabled = false,
  onPreset,
  onChange,
  onRefresh,
}: {
  view: View;
  filter: Filter;
  preset: string;
  options: FilterOptions;
  disabled: boolean;
  refreshDisabled?: boolean;
  onPreset: (preset: string, range?: { from: string | null; to: string | null }) => void;
  onChange: (filter: Filter) => void;
  onRefresh: () => void;
}) {
  const { title, subtitle } = viewTitle(view);
  const hideFilters =
    view === "cursor" ||
    view === "cursor-sessions" ||
    view === "instructions" ||
    view === "settings";
  const [customOpen, setCustomOpen] = useState(preset === "custom");
  const [customFrom, setCustomFrom] = useState(() => (filter.from ?? "").slice(0, 10));
  const [customTo, setCustomTo] = useState(() => (filter.to ?? "").slice(0, 10));

  function selectPreset(value: string) {
    if (value === "custom") {
      setCustomOpen(true);
      return;
    }
    setCustomOpen(false);
    onPreset(value);
  }

  function applyCustomRange() {
    if (!customFrom || !customTo) {
      return;
    }
    onPreset("custom", customRangeFilter(customFrom, customTo));
  }

  return (
    <header className="topbar">
      <div>
        <h1>{title}</h1>
        <p>{subtitle}</p>
      </div>
      {!hideFilters ? (
        <div className="topbar-actions">
          <Select
            icon="calendar"
            ariaLabel="时间范围"
            disabled={disabled}
            value={customOpen ? "custom" : preset}
            displayLabel={customOpen ? "自定义区间" : formatRangeLabel(filter, preset)}
            options={RANGE_OPTIONS}
            onChange={selectPreset}
          />
          {customOpen ? (
            <div className="custom-range">
              <DatePicker
                ariaLabel="开始日期"
                disabled={disabled}
                value={customFrom}
                max={customTo || undefined}
                onChange={setCustomFrom}
              />
              <span>至</span>
              <DatePicker
                ariaLabel="结束日期"
                disabled={disabled}
                value={customTo}
                min={customFrom || undefined}
                onChange={setCustomTo}
              />
              <Button
                variant="text"
                disabled={disabled || !customFrom || !customTo}
                onClick={applyCustomRange}
              >
                应用
              </Button>
            </div>
          ) : null}
          <Button
            variant="icon"
            disabled={disabled || refreshDisabled}
            onClick={onRefresh}
            title="刷新"
            aria-label="刷新数据"
          >
            <Icon name="refresh" size={15} />
          </Button>
          <MultiSelect
            label="全部项目"
            options={options.projects}
            selected={filter.projects}
            renderLabel={projectLabel}
            disabled={disabled}
            onChange={(projects) => onChange({ ...filter, projects })}
          />
          <MultiSelect
            label="全部应用"
            icon="filter"
            options={options.sources}
            selected={filter.sources}
            renderLabel={applicationLabel}
            disabled={disabled}
            onChange={(sources) => onChange({ ...filter, sources })}
          />
          <MultiSelect
            label="全部模型"
            options={options.models}
            selected={filter.models}
            disabled={disabled}
            renderIcon={(model) => <VendorIcon name={model} size={14} />}
            onChange={(models) => onChange({ ...filter, models })}
          />
          <MultiSelect
            label="全部 Provider"
            options={options.providers}
            selected={filter.providers}
            disabled={disabled}
            renderLabel={(name) => `${name}（${providerChannel(name)}）`}
            onChange={(providers) => onChange({ ...filter, providers })}
          />
        </div>
      ) : null}
    </header>
  );
}

function MultiSelect({
  label,
  icon,
  options,
  selected,
  renderLabel,
  renderIcon,
  disabled,
  onChange,
}: {
  label: string;
  icon?: IconName;
  options: string[];
  selected: string[];
  renderLabel?: (value: string) => string;
  renderIcon?: (value: string) => ReactNode;
  disabled?: boolean;
  onChange: (values: string[]) => void;
}) {
  const { open, setOpen, rootRef } = useDismissible();
  const panelStyle = useAnchoredPanel(open, rootRef);

  function toggleValue(value: string) {
    if (selected.includes(value)) {
      onChange(selected.filter((item) => item !== value));
    } else {
      onChange([...selected, value]);
    }
  }

  const summary =
    selected.length === 0
      ? label
      : selected.length === 1
        ? renderLabel
          ? renderLabel(selected[0])
          : selected[0]
        : `已选 ${selected.length} 项`;

  return (
    <div className="multi-select" ref={rootRef}>
      <button
        type="button"
        className="chip-field multi-select-trigger"
        disabled={disabled}
        onClick={() => setOpen((value) => !value)}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`${label}：${summary}`}
      >
        {selected.length === 1 && renderIcon ? renderIcon(selected[0]) : null}
        {icon ? <Icon name={icon} size={14} /> : null}
        <span className="chip-range">{summary}</span>
        <Icon name="chevron" size={12} className={open ? "select-caret open" : "select-caret"} />
      </button>
      {open ? (
        <div className="multi-select-panel" role="listbox" aria-label={label} style={panelStyle}>
          <div className="multi-select-actions">
            <Button variant="text" onClick={() => onChange([])}>
              清空
            </Button>
            <Button variant="text" onClick={() => onChange(options)}>
              全选
            </Button>
          </div>
          <div className="multi-select-list">
            {options.map((option) => (
              <label className="multi-select-item" key={option}>
                <input
                  type="checkbox"
                  checked={selected.includes(option)}
                  onChange={() => toggleValue(option)}
                />
                {renderIcon ? renderIcon(option) : null}
                <span>{renderLabel ? renderLabel(option) : option}</span>
              </label>
            ))}
            {options.length === 0 ? <div className="multi-select-empty">暂无选项</div> : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}
