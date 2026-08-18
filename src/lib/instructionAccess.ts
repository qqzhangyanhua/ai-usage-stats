import type { GlobalInstructionFile } from "../types";

export function canEditInstruction(file: GlobalInstructionFile): boolean {
  if (
    file.kind === "directory" ||
    file.abs_path.length === 0 ||
    file.load_status === "locally_invisible"
  ) {
    return false;
  }
  const path = file.display_path;
  return (
    path === "~/.claude/CLAUDE.md" ||
    path.startsWith("~/.claude/rules/") ||
    path === "~/.codex/AGENTS.md" ||
    path === "~/.codex/AGENTS.override.md" ||
    path === "~/.gemini/GEMINI.md"
  );
}

export function canOpenInstruction(file: GlobalInstructionFile): boolean {
  return file.load_status !== "locally_invisible" && file.abs_path.length > 0;
}

export function showsLoadStatus(file: GlobalInstructionFile): boolean {
  return file.evidence !== "no_mechanism";
}
