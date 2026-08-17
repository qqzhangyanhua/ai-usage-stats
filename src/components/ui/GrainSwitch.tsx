import type { Grain } from "../../types";
import { Segmented } from "./Segmented";

const GRAIN_OPTIONS = [
  { value: "day", label: "按日" },
  { value: "week", label: "按周" },
  { value: "month", label: "按月" },
] as const;

export const grainUnit: Record<Grain, string> = {
  day: "天",
  week: "周",
  month: "月",
};

export function GrainSwitch({
  value,
  disabled,
  onChange,
}: {
  value: Grain;
  disabled?: boolean;
  onChange: (grain: Grain) => void;
}) {
  return (
    <Segmented
      value={value}
      options={GRAIN_OPTIONS}
      disabled={disabled}
      ariaLabel="时间粒度"
      onChange={onChange}
    />
  );
}
