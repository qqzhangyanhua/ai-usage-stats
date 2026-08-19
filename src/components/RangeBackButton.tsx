import { Icon } from "../icons";
import { Button } from "./ui/Button";

export function RangeBackButton({
  disabled,
  onClick,
}: {
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <Button
      variant="text"
      className="range-back-btn"
      disabled={disabled}
      onClick={onClick}
      title="返回上一级时间范围"
      aria-label="返回上一级时间范围"
    >
      <Icon name="chevron" size={14} />
      返回上一级
    </Button>
  );
}
