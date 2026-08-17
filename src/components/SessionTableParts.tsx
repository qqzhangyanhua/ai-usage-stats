import { useState, type MouseEvent } from "react";
import { Icon } from "../icons";
import type { SortDir } from "../types";
import { Button } from "./ui/Button";

export function SortArrow({ active, dir }: { active: boolean; dir: SortDir }) {
  return (
    <Icon
      name="chevron"
      size={11}
      className={
        active ? (dir === "asc" ? "sort-arrow asc" : "sort-arrow desc") : "sort-arrow idle"
      }
    />
  );
}

export function SortButton({
  active,
  dir,
  onClick,
}: {
  active: boolean;
  dir: SortDir;
  onClick: () => void;
}) {
  return (
    <button type="button" className="sort-th" onClick={onClick} aria-label="排序">
      <SortArrow active={active} dir={dir} />
    </button>
  );
}

export function SessionIdCell({ sessionId }: { sessionId: string }) {
  const [copied, setCopied] = useState(false);

  async function copyId(event: MouseEvent<HTMLButtonElement>) {
    event.stopPropagation();
    try {
      await navigator.clipboard.writeText(sessionId);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  }

  return (
    <div className="session-id-cell">
      <span className="mono" title={sessionId}>
        {sessionId}
      </span>
      <Button
        variant="icon"
        className={copied ? "table-icon-btn is-copied" : "table-icon-btn"}
        onClick={copyId}
        title={copied ? "已复制" : "复制会话 ID"}
        aria-label={copied ? "已复制会话 ID" : "复制会话 ID"}
      >
        <Icon name={copied ? "check" : "copy"} size={12} />
      </Button>
    </div>
  );
}
