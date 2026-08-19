import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { formatBytes, formatClock, humanStatus } from "../lib/format";
import type {
  GlobalInstructionDto,
  GlobalInstructionFile,
  InstructionEvidence,
  InstructionLoadStatus,
} from "../types";
import { EmptyState } from "./EmptyState";
import {
  canEditInstruction,
  canOpenInstruction,
  showsLoadStatus,
} from "../lib/instructionAccess";
import { InstructionCheckup } from "./InstructionCheckup";
import { InstructionEditor } from "./InstructionEditor";
import { Button } from "./ui/Button";

const STATUS_LABEL: Record<InstructionLoadStatus, string> = {
  loaded: "已加载",
  present_unloaded: "存在但未被加载",
  locally_invisible: "本地不可见",
  not_created: "未创建",
};

const EVIDENCE_LABEL: Record<InstructionEvidence, string> = {
  verified: "已验证",
  inferred: "推测",
  no_mechanism: "无机制",
};

export function GlobalInstructionPanel() {
  const [data, setData] = useState<GlobalInstructionDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [openPath, setOpenPath] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const draftsRef = useRef(drafts);
  useEffect(() => {
    draftsRef.current = drafts;
  }, [drafts]);

  const load = useCallback((force = false) => {
    if (!force && Object.keys(draftsRef.current).length > 0) {
      return;
    }
    setBusy(true);
    setError(null);
    invoke<GlobalInstructionDto>("get_global_instructions")
      .then((next) => {
        setData(next);
        if (force) {
          setDrafts({});
        }
      })
      .catch((err: unknown) => {
        setError(humanStatus(err));
      })
      .finally(() => {
        setBusy(false);
      });
  }, []);

  useEffect(() => {
    load();
    function onFocus() {
      load();
    }
    function onVisibility() {
      if (document.visibilityState === "visible") {
        load();
      }
    }
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [load]);

  const files = data?.sources.flatMap((row) => row.files) ?? [];

  async function openCursorSettings() {
    setActionError(null);
    try {
      await invoke("open_cursor_instruction_settings");
    } catch (err: unknown) {
      setActionError(humanStatus(err));
    }
  }

  async function openExternal(absPath: string) {
    setActionError(null);
    try {
      await invoke("open_global_instruction", { abs_path: absPath });
    } catch (err: unknown) {
      setActionError(humanStatus(err));
    }
  }

  return (
    <article className="panel instruction-panel">
      <div className="panel-head">
        <div>
          <h2>全局指令</h2>
          <p className="muted">每次进入或切回应用时重新读盘，不缓存。</p>
        </div>
        <Button type="button" variant="ghost" disabled={busy} onClick={() => load(true)}>
          重新读取
        </Button>
      </div>
      {error ? <EmptyState tone="warn" title="读取失败" hint={error} /> : null}
      {actionError ? <EmptyState tone="warn" title="无法打开" hint={actionError} /> : null}
      {data ? <InstructionCheckup findings={data.findings} /> : null}
      {!error && !files.length && !busy ? (
        <EmptyState
          title="尚未发现全局指令"
          hint="已覆盖全部已支持来源。未创建与无机制不是同一回事。"
        />
      ) : null}
      {data
        ? data.sources.map((row) => (
            <section className="instruction-source" key={row.source}>
              <h3>{row.application}</h3>
              <ul className="instruction-list">
                {row.files.map((file) => {
                  const id = `${row.source}:${file.display_path}`;
                  return (
                    <InstructionRow
                      key={id}
                      file={file}
                      draft={drafts[id] ?? file.content}
                      open={openPath === id}
                      onToggle={() => setOpenPath((current) => (current === id ? null : id))}
                      onDraft={(value) =>
                        setDrafts((current) => ({ ...current, [id]: value }))
                      }
                      onSaved={() => {
                        setDrafts((current) => {
                          const next = { ...current };
                          delete next[id];
                          return next;
                        });
                        load(true);
                      }}
                      onCursorSettings={openCursorSettings}
                      onOpenExternal={() => void openExternal(file.abs_path)}
                    />
                  );
                })}
              </ul>
            </section>
          ))
        : null}
    </article>
  );
}

function InstructionRow({
  file,
  draft,
  open,
  onToggle,
  onDraft,
  onSaved,
  onCursorSettings,
  onOpenExternal,
}: {
  file: GlobalInstructionFile;
  draft: string;
  open: boolean;
  onToggle: () => void;
  onDraft: (value: string) => void;
  onSaved: () => void;
  onCursorSettings: () => void;
  onOpenExternal: () => void;
}) {
  return (
    <li className={`instruction-row status-${file.load_status} evidence-${file.evidence}`}>
      <div className="instruction-row-bar">
        <button type="button" className="instruction-row-head" onClick={onToggle}>
          <div className="instruction-row-title">
            <strong>{file.display_path}</strong>
            <span className="instruction-badges">
              {showsLoadStatus(file) ? (
                <em className="instruction-status">{STATUS_LABEL[file.load_status]}</em>
              ) : null}
              <em className="instruction-evidence">{EVIDENCE_LABEL[file.evidence]}</em>
            </span>
          </div>
          <div className="instruction-row-meta">
            <span>{file.kind === "directory" ? "目录" : formatBytes(file.byte_size)}</span>
            <span>{formatClock(file.modified_at)}</span>
          </div>
          {file.note ? <p className="instruction-note">{file.note}</p> : null}
        </button>
        {canOpenInstruction(file) ? (
          <Button type="button" variant="ghost" onClick={onOpenExternal}>
            在外部打开
          </Button>
        ) : null}
      </div>
      {open ? (
        <div className="instruction-body">
          {file.error ? <p className="instruction-error">{file.error}</p> : null}
          {file.action === "cursor_settings" ? (
            <Button type="button" variant="ghost" onClick={onCursorSettings}>
              在 Cursor 中打开设置
            </Button>
          ) : null}
          {file.load_status === "locally_invisible" ? (
            <p className="muted">内容在账号服务端，本机无法展示。</p>
          ) : null}
          {file.evidence === "no_mechanism" ? (
            <p className="muted">该来源没有用户级全局指令机制，不必按路径去创建文件。</p>
          ) : null}
          {canEditInstruction(file) ? (
            <InstructionEditor
              file={file}
              draft={draft}
              onDraft={onDraft}
              onSaved={onSaved}
            />
          ) : null}
        </div>
      ) : null}
    </li>
  );
}
