import { useState, type ReactNode } from "react";
import { readSectionOpen, writeSectionOpen } from "../lib/sectionCollapse";

export function CollapsibleSection({
  sectionId,
  title,
  collapsedSummary,
  defaultOpen = true,
  extra,
  className,
  children,
}: {
  sectionId: string;
  title: string;
  collapsedSummary: string;
  defaultOpen?: boolean;
  extra?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(() => readSectionOpen(sectionId, defaultOpen));
  const bodyId = `overview-section-${sectionId}`;

  function toggle() {
    setOpen((prev) => {
      const next = !prev;
      writeSectionOpen(sectionId, next);
      return next;
    });
  }

  return (
    <section
      className={["collapsible-section", open ? "is-open" : "is-collapsed", className]
        .filter(Boolean)
        .join(" ")}
    >
      <div className="panel-head collapsible-head">
        <h2>{title}</h2>
        <div className="collapsible-actions">
          {open ? extra : <span className="muted collapsible-summary">{collapsedSummary}</span>}
          <button
            type="button"
            className="icon-btn collapsible-toggle"
            aria-expanded={open}
            aria-controls={bodyId}
            title={open ? "收起" : "展开"}
            onClick={toggle}
          >
            <span className="caret" aria-hidden="true">
              ▾
            </span>
          </button>
        </div>
      </div>
      {open ? (
        <div className="collapsible-body" id={bodyId}>
          {children}
        </div>
      ) : null}
    </section>
  );
}
