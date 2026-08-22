import { useEffect } from "react";
import type { View } from "../types";

const SHORTCUT_VIEWS: View[] = [
  "overview",
  "trend",
  "conversations",
  "model",
  "project",
  "application",
  "provider",
  "cursor",
  "cursor-sessions",
  "settings",
];

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
}

export function useKeyboardShortcuts({
  onNavigate,
  onRefresh,
  onClearFilters,
}: {
  onNavigate: (view: View) => void;
  onRefresh: () => void;
  onClearFilters: () => void;
}): void {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.metaKey || event.ctrlKey || event.altKey) {
        return;
      }
      if (isTypingTarget(event.target)) {
        return;
      }
      if (event.key === "r" || event.key === "R") {
        event.preventDefault();
        onRefresh();
        return;
      }
      if (event.key === "Escape") {
        onClearFilters();
        return;
      }
      const index = event.key === "0" ? 9 : Number(event.key) - 1;
      const next = Number.isInteger(index) ? SHORTCUT_VIEWS[index] : undefined;
      if (next) {
        event.preventDefault();
        onNavigate(next);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onNavigate, onRefresh, onClearFilters]);
}
