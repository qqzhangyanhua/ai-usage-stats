import { Icon, type IconName } from "../../icons";
import { useAnchoredPanel } from "../../hooks/useAnchoredPanel";
import { useDismissible } from "../../hooks/useDismissible";

export type SelectOption = {
  value: string;
  label: string;
};

export function Select({
  value,
  options,
  disabled,
  icon,
  ariaLabel,
  displayLabel,
  align = "right",
  variant = "chip",
  onChange,
}: {
  value: string;
  options: SelectOption[];
  disabled?: boolean;
  icon?: IconName;
  ariaLabel: string;
  displayLabel?: string;
  align?: "left" | "right";
  variant?: "chip" | "plain";
  onChange: (value: string) => void;
}) {
  const { open, setOpen, rootRef } = useDismissible();
  const panelStyle = useAnchoredPanel(open, rootRef, align);
  const selected = options.find((option) => option.value === value);
  const summary = displayLabel ?? selected?.label ?? value;

  return (
    <div className={`select ${variant === "plain" ? "select-plain" : ""}`} ref={rootRef}>
      <button
        type="button"
        className={variant === "plain" ? "select-plain-trigger" : "chip-field select-trigger"}
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`${ariaLabel}：${summary}`}
      >
        {icon ? <Icon name={icon} size={14} /> : null}
        <span className={variant === "plain" ? "select-plain-label" : "chip-range"}>{summary}</span>
        <Icon name="chevron" size={12} className={open ? "select-caret open" : "select-caret"} />
      </button>
      {open ? (
        <div className="select-panel" role="listbox" aria-label={ariaLabel} style={panelStyle}>
          {options.map((option) => {
            const active = option.value === value;
            return (
              <button
                key={option.value}
                type="button"
                role="option"
                aria-selected={active}
                className={active ? "select-option active" : "select-option"}
                onClick={() => {
                  onChange(option.value);
                  setOpen(false);
                }}
              >
                <span>{option.label}</span>
                {active ? <Icon name="check" size={13} /> : null}
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
