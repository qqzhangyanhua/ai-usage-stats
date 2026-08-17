import { useMemo, useState } from "react";
import { Icon } from "../../icons";
import { useAnchoredPanel } from "../../hooks/useAnchoredPanel";
import { useDismissible } from "../../hooks/useDismissible";
import {
  calendarCells,
  formatDateLabel,
  monthTitle,
  parseDateValue,
  shiftMonth,
  toDateValue,
  weekdayLabels,
} from "../../lib/calendar";

export function DatePicker({
  value,
  min,
  max,
  disabled,
  ariaLabel,
  onChange,
}: {
  value: string;
  min?: string;
  max?: string;
  disabled?: boolean;
  ariaLabel: string;
  onChange: (value: string) => void;
}) {
  const { open, setOpen, rootRef } = useDismissible();
  const panelStyle = useAnchoredPanel(open, rootRef, "left", 320);
  const selected = parseDateValue(value);
  const today = useMemo(() => toDateValue(new Date()), []);
  const initial = selected ?? new Date();
  const [cursor, setCursor] = useState({ year: initial.getFullYear(), month: initial.getMonth() });

  function openPicker() {
    const next = parseDateValue(value) ?? new Date();
    setCursor({ year: next.getFullYear(), month: next.getMonth() });
    setOpen(true);
  }

  const cells = calendarCells(cursor.year, cursor.month);

  return (
    <div className="date-picker" ref={rootRef}>
      <button
        type="button"
        className="date-picker-trigger"
        disabled={disabled}
        onClick={() => (open ? setOpen(false) : openPicker())}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={ariaLabel}
      >
        <Icon name="calendar" size={13} />
        <span>{formatDateLabel(value)}</span>
      </button>
      {open ? (
        <div className="date-picker-panel" role="dialog" aria-label={ariaLabel} style={panelStyle}>
          <div className="date-picker-head">
            <button
              type="button"
              className="date-nav-btn"
              onClick={() => setCursor((current) => shiftMonth(current.year, current.month, -1))}
              aria-label="上个月"
            >
              <Icon name="chevron" size={13} />
            </button>
            <strong>{monthTitle(cursor.year, cursor.month)}</strong>
            <button
              type="button"
              className="date-nav-btn"
              onClick={() => setCursor((current) => shiftMonth(current.year, current.month, 1))}
              aria-label="下个月"
            >
              <Icon name="chevron" size={13} className="flip" />
            </button>
          </div>
          <div className="date-picker-weekdays">
            {weekdayLabels().map((label) => (
              <span key={label}>{label}</span>
            ))}
          </div>
          <div className="date-picker-grid">
            {cells.map((cell) => {
              const outOfRange = Boolean((min && cell.value < min) || (max && cell.value > max));
              const selectedDay = cell.value === value;
              const isToday = cell.value === today;
              return (
                <button
                  key={cell.value}
                  type="button"
                  disabled={outOfRange}
                  className={[
                    "date-cell",
                    cell.inMonth ? "" : "muted-day",
                    selectedDay ? "selected" : "",
                    isToday ? "today" : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  onClick={() => {
                    onChange(cell.value);
                    setOpen(false);
                  }}
                >
                  {Number(cell.value.slice(8))}
                </button>
              );
            })}
          </div>
        </div>
      ) : null}
    </div>
  );
}
