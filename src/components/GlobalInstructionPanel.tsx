import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { formatBytes, formatClock, humanStatus } from "../lib/format";
import type {
  GlobalInstructionDto,
  GlobalInstructionFile,
  InstructionLoadStatus,
} from "../types";
import { EmptyState } from "./EmptyState";
import { Button } from "./ui/Button";

const STATUS_LABEL: Record<InstructionLoadStatus, string> = {
  loaded: "已加载",
  present_unloaded: "存在但未加载",
  locally_invisible: "本地不可见",
  not_created: "未创建",
};

export function GlobalInstructionPanel() {
  const [data, setData] = useState<GlobalInstructionDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [openPath, setOpenPath] = useState<string | null>(null);

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

  const files = data?.sources.flatMap((row) =>
    row.files.map((file) => ({ source: row.application, file })),
  );

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
      {!error && !files?.length && !busy ? (
        <EmptyState title="尚未发现全局指令" hint="当前只扫描 Claude 的主文件与用户级指令目录。" />
      ) : null}
      {data
        ? data.sources.map((row) => (
            <section className="instruction-source" key={row.source}>
              <h3>{row.application}</h3>
              <ul className="instruction-list">
                {row.files.map((file) => (
                  <InstructionRow
                    key={file.display_path}
                    file={file}
                    open={openPath === file.display_path}
                    onToggle={() =>
                      setOpenPath((current) =>
                        current === file.display_path ? null : file.display_path,
                      )
                    }
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
}: {
  file: GlobalInstructionFile;
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <li className={`instruction-row status-${file.load_status}`}>
      <button type="button" className="instruction-row-head" onClick={onToggle}>
        <div className="instruction-row-title">
          <strong>{file.display_path}</strong>
          <em>{STATUS_LABEL[file.load_status]}</em>
        </div>
        <div className="instruction-row-meta">
          <span>{formatBytes(file.byte_size)}</span>
          <span>{formatClock(file.modified_at)}</span>
        </div>
      </button>
      {open ? (
        <div className="instruction-body">
          {file.error ? <p className="instruction-error">{file.error}</p> : null}
          {file.load_status === "not_created" ? (
            <p className="muted">文件尚未创建。</p>
          ) : (
            <pre>{file.content || "（空文件）"}</pre>
          )}
        </div>
      ) : null}
    </li>
  );
}
