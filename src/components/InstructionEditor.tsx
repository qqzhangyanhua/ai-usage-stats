import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { humanStatus } from "../lib/format";
import type { GlobalInstructionFile, WriteUserFileRequest, WriteUserFileResult } from "../types";
import { Button } from "./ui/Button";

export function InstructionEditor({
  file,
  draft,
  onDraft,
  onSaved,
}: {
  file: GlobalInstructionFile;
  draft: string;
  onDraft: (value: string) => void;
  onSaved: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dirty = draft !== file.content;

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const request: WriteUserFileRequest = {
        abs_path: file.abs_path,
        content: draft,
        expected_mtime: file.modified_at,
      };
      await invoke<WriteUserFileResult>("write_global_instruction", { request });
      onSaved();
    } catch (err: unknown) {
      setError(humanStatus(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="instruction-editor">
      <textarea
        className="instruction-textarea"
        value={draft}
        spellCheck={false}
        onChange={(event) => onDraft(event.target.value)}
        aria-label={`编辑 ${file.display_path}`}
      />
      <div className="instruction-editor-actions">
        <Button type="button" variant="accent" disabled={busy || !dirty} onClick={() => void save()}>
          保存
        </Button>
        {error ? <p className="instruction-error">{error}</p> : null}
      </div>
    </div>
  );
}

export function canEditInstruction(file: GlobalInstructionFile): boolean {
  if (file.abs_path.length === 0 || file.load_status === "locally_invisible") {
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
