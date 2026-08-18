import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { formatBytes, formatClock, humanStatus } from "../lib/format";
import type {
  GlobalInstructionDto,
  GlobalInstructionFile,
  InstructionEvidence,
  InstructionLoadStatus,
} from "../types";
import { EmptyState } from "./EmptyState";
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

  const load = useCallback(() => {
    setBusy(true);
    setError(null);
    invoke<GlobalInstructionDto>("get_global_instructions")
      .then((next) => {
        setData(next);
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

  return (
    <article className="panel instruction-panel">
      <div className="panel-head">
        <div>
          <h2>全局指令</h2>
          <p className="muted">每次进入或切回应用时重新读盘，不缓存。</p>
        </div>
        <Button type="button" variant="ghost" disabled={busy} onClick={load}>
          重新读取
        </Button>
      </div>
      {error ? <EmptyState tone="warn" title="读取失败" hint={error} /> : null}
      {actionError ? <EmptyState tone="warn" title="无法跳转" hint={actionError} /> : null}
      {!error && !files.length && !busy ? (
        <EmptyState title="尚未发现全局指令" hint="当前扫描 Claude、Codex、Gemini 与 Cursor。" />
      ) : null}
      {data
        ? data.sources.map((row) => (
            <section className="instruction-source" key={row.source}>
              <h3>{row.application}</h3>
              <ul className="instruction-list">
                {row.files.map((file) => (
                  <InstructionRow
                    key={`${row.source}:${file.display_path}`}
                    file={file}
                    open={openPath === `${row.source}:${file.display_path}`}
                    onToggle={() =>
                      setOpenPath((current) => {
                        const id = `${row.source}:${file.display_path}`;
                        return current === id ? null : id;
                      })
                    }
                    onCursorSettings={openCursorSettings}
                  />
                ))}
              </ul>
            </section>
          ))
        : null}
    </article>
  );
}

function InstructionRow({
  file,
  open,
  onToggle,
  onCursorSettings,
}: {
  file: GlobalInstructionFile;
  open: boolean;
  onToggle: () => void;
  onCursorSettings: () => void;
}) {
  return (
    <li
      className={`instruction-row status-${file.load_status} evidence-${file.evidence}`}
    >
      <button type="button" className="instruction-row-head" onClick={onToggle}>
        <div className="instruction-row-title">
          <strong>{file.display_path}</strong>
          <span className="instruction-badges">
            <em className="instruction-status">{STATUS_LABEL[file.load_status]}</em>
            <em className="instruction-evidence">{EVIDENCE_LABEL[file.evidence]}</em>
          </span>
        </div>
        <div className="instruction-row-meta">
          <span>{formatBytes(file.byte_size)}</span>
          <span>{formatClock(file.modified_at)}</span>
        </div>
        {file.note ? <p className="instruction-note">{file.note}</p> : null}
      </button>
      {open ? (
        <div className="instruction-body">
          {file.error ? <p className="instruction-error">{file.error}</p> : null}
          {file.action === "cursor_settings" ? (
            <Button type="button" variant="ghost" onClick={onCursorSettings}>
              在 Cursor 中打开设置
            </Button>
          ) : null}
          {file.load_status === "not_created" ? (
            <p className="muted">文件尚未创建。</p>
          ) : null}
          {file.load_status === "locally_invisible" ? (
            <p className="muted">内容在账号服务端，本机无法展示。</p>
          ) : null}
          {file.load_status === "loaded" || file.load_status === "present_unloaded" ? (
            <pre>{file.content || "（空文件）"}</pre>
          ) : null}
        </div>
      ) : null}
    </li>
  );
}
