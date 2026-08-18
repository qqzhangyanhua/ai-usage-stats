import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import {
  getCachedCursorSessionDetail,
  setCachedCursorSessionDetail,
} from "../lib/cursorSessionDetailCache";
import {
  formatClock,
  formatDuration,
  formatTokens,
  projectLabel,
  relativeTime,
} from "../lib/format";
import {
  cursorSessionDetailToolTable,
  cursorSessionHashFileTable,
  cursorSessionPathTable,
} from "../lib/exportRows";
import type { CursorSessionDetailDto } from "../types";
import { EmptyState } from "./EmptyState";
import { ExportButton } from "./ExportButton";
import { LoadingOverlay } from "./LoadingOverlay";
import { SessionIdCell } from "./SessionTableParts";

function fileLabel(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function CursorSessionDetail({
  sourceFile,
  onError,
}: {
  sourceFile: string;
  onError?: (error: unknown) => void;
}) {
  const cached = getCachedCursorSessionDetail(sourceFile);
  const [fetched, setFetched] = useState<CursorSessionDetailDto | null>(null);
  const [loading, setLoading] = useState(() => !getCachedCursorSessionDetail(sourceFile));
  const generationRef = useRef(0);
  const detail = cached ?? fetched;

  useEffect(() => {
    if (getCachedCursorSessionDetail(sourceFile)) {
      return;
    }
    const generation = ++generationRef.current;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- 发起请求前先置 loading
    setLoading(true);
    invoke<CursorSessionDetailDto>("get_cursor_session_detail", { sourceFile })
      .then((next) => {
        if (generation === generationRef.current) {
          setCachedCursorSessionDetail(sourceFile, next);
          setFetched(next);
        }
      })
      .catch((error: unknown) => {
        if (generation === generationRef.current) {
          onError?.(error);
        }
      })
      .finally(() => {
        if (generation === generationRef.current) {
          setLoading(false);
        }
      });
  }, [sourceFile, onError]);

  if (!detail && loading) {
    return (
      <LoadingOverlay active className="panel partition">
        <EmptyState icon="sessions" title="正在加载会话详情…" />
      </LoadingOverlay>
    );
  }

  if (!detail) {
    return (
      <div className="panel partition">
        <EmptyState icon="sessions" title="无法加载会话详情" />
      </div>
    );
  }

  const session = detail.session;
  const duration = formatDuration(session.first_seen_at, session.last_seen_at);
  const toolTable = cursorSessionDetailToolTable(detail);
  const pathTable = cursorSessionPathTable(detail);
  const hashTable = cursorSessionHashFileTable(detail);

  return (
    <LoadingOverlay active={loading} className="stack">
      <section className="panel partition">
        <div className="panel-head">
          <h2>会话详情</h2>
          <SessionIdCell sessionId={session.session_id} />
        </div>
        <p className="note">
          {projectLabel(session.project)} · {session.models.join(", ") || "无模型"} ·{" "}
          {session.sources.join(", ") || "无来源"}
          {duration ? ` · ${duration}` : ""}
          {session.last_seen_at
            ? ` · ${relativeTime(session.last_seen_at)}（${formatClock(session.last_seen_at)}）`
            : ""}
        </p>
        <p className="note">
          轮次 {formatTokens(session.turn_count)} · 提问 {formatTokens(session.user_prompt_count)} ·
          失败 {formatTokens(session.error_count)} · 中止 {formatTokens(session.aborted_count)} ·
          子代理 {formatTokens(session.subagent_count)}
        </p>
        {detail.transcript_missing ? (
          <p className="note">原 transcript 已不在磁盘，工具次数来自缓存，读写路径无法重算。</p>
        ) : null}
      </section>

      <div className="split-2">
        <section className="panel partition">
          <div className="panel-head">
            <h2>工具次数</h2>
            <ExportButton filename="Cursor会话工具明细" headers={toolTable.headers} rows={toolTable.rows} />
          </div>
          {detail.tools.length > 0 ? (
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>工具</th>
                    <th>次数</th>
                  </tr>
                </thead>
                <tbody>
                  {detail.tools.map((row) => (
                    <tr key={row.name}>
                      <td>{row.name}</td>
                      <td>{formatTokens(row.call_count)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <p className="note">该会话没有工具调用。</p>
          )}
        </section>

        <section className="panel partition">
          <div className="panel-head">
            <h2>读写路径</h2>
            <ExportButton filename="Cursor会话路径" headers={pathTable.headers} rows={pathTable.rows} />
          </div>
          <p className="note">
            只统计工具参数里的 path，不含命令和正文。读 {formatTokens(detail.read_paths.length)}{" "}
            · 写 {formatTokens(detail.write_paths.length)}
          </p>
          <PathList title="读" paths={detail.read_paths} />
          <PathList title="写" paths={detail.write_paths} />
        </section>
      </div>

      <section className="panel partition">
        <div className="panel-head">
          <h2>AI 记过哈希的文件</h2>
          <ExportButton filename="Cursor会话哈希文件" headers={hashTable.headers} rows={hashTable.rows} />
        </div>
        <p className="note">来自 ai_code_hashes，不是「读过的文件」，也不是代码行数。</p>
        {detail.hash_files.length > 0 ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>文件</th>
                  <th>扩展名</th>
                  <th>来源</th>
                </tr>
              </thead>
              <tbody>
                {detail.hash_files.map((row) => (
                  <tr key={row.path}>
                    <td title={row.path}>{fileLabel(row.path)}</td>
                    <td>{row.extension || "—"}</td>
                    <td>{row.source || "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <p className="note">该会话没有关联的代码哈希。</p>
        )}
      </section>
    </LoadingOverlay>
  );
}

function PathList({ title, paths }: { title: string; paths: string[] }) {
  if (paths.length === 0) {
    return <p className="note">无{title}路径。</p>;
  }
  return (
    <div className="table-scroll">
      <table>
        <thead>
          <tr>
            <th>{title}</th>
          </tr>
        </thead>
        <tbody>
          {paths.map((path) => (
            <tr key={path}>
              <td title={path}>{fileLabel(path)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
