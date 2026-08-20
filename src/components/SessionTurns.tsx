import { useEffect, useMemo, useState } from "react";
import { Icon } from "../icons";
import {
  formatClock,
  formatCost,
  formatTokens,
  providerChannel,
  relativeTime,
} from "../lib/format";
import type { TurnRow } from "../types";
import { EmptyState } from "./EmptyState";
import { ExportButton } from "./ExportButton";
import { Spark } from "./Kpi";
import { LoadingOverlay } from "./LoadingOverlay";
import { Pagination } from "./Pagination";
import { Button } from "./ui/Button";
import { SessionResumeCommand } from "./SessionResumeCommand";
import { ModelLabel } from "./VendorIcon";

const PAGE_SIZE = 20;

type TokenField = {
  key:
    | "input_tokens"
    | "output_tokens"
    | "cache_read_tokens"
    | "cache_creation_tokens"
    | "reasoning_tokens"
    | "total_tokens";
  label: string;
};

const TOKEN_FIELDS: TokenField[] = [
  { key: "input_tokens", label: "输入" },
  { key: "output_tokens", label: "输出" },
  { key: "cache_read_tokens", label: "缓存读" },
  { key: "cache_creation_tokens", label: "缓存写" },
  { key: "reasoning_tokens", label: "推理" },
  { key: "total_tokens", label: "总量" },
];

const EXPORT_HEADERS = [
  "时间",
  "模型",
  "输入",
  "输出",
  "缓存读",
  "缓存写",
  "推理",
  "总量",
  "费用",
  "费用来源",
];

export function SessionTurns({
  sessionId,
  source,
  sourceLabel,
  turns,
  turnsLoading = false,
}: {
  sessionId: string;
  source: string;
  sourceLabel: string;
  turns: TurnRow[];
  turnsLoading?: boolean;
}) {
  const [page, setPage] = useState(1);
  const [detail, setDetail] = useState<TurnRow | null>(null);

  // 会话切换时重置分页与详情，改在渲染期间"调整状态"而非 effect 里同步 setState，
  // 避免多触发一次级联渲染（见 react-hooks/set-state-in-effect）。
  const [turnsKey, setTurnsKey] = useState(() => `${source}:${sessionId}`);
  const nextTurnsKey = `${source}:${sessionId}`;
  if (turnsKey !== nextTurnsKey) {
    setTurnsKey(nextTurnsKey);
    setPage(1);
    setDetail(null);
  }

  const pageCount = Math.max(1, Math.ceil(turns.length / PAGE_SIZE));
  const currentPage = Math.min(page, pageCount);
  const pagedTurns = useMemo(() => {
    const start = (currentPage - 1) * PAGE_SIZE;
    return turns.slice(start, start + PAGE_SIZE);
  }, [currentPage, turns]);

  const stats = useMemo(() => {
    const totalTokens = turns.reduce((sum, turn) => sum + turn.total_tokens, 0);
    const totalCost = turns.reduce((sum, turn) => sum + (turn.cost ?? 0), 0);
    const hasCost = turns.some((turn) => turn.cost != null);
    return { totalTokens, totalCost, hasCost };
  }, [turns]);

  return (
    <div className="panel">
      <div className="panel-head">
        <div>
          <h2>
            会话 {sessionId}（{sourceLabel}）每轮明细
          </h2>
          <p className="panel-note">
            共 {turns.length} 轮 · {formatTokens(stats.totalTokens)} Token
            {stats.hasCost ? ` · $${stats.totalCost.toFixed(4)}` : ""}
          </p>
          <SessionResumeCommand source={source} sessionId={sessionId} />
        </div>
        <div className="export-action">
          {turns.length > 1 ? (
            <Spark values={turns.map((turn) => turn.total_tokens)} color="#8b6cff" />
          ) : null}
          <ExportButton
            label="导出明细"
            filename={`会话-${sessionId}-明细`}
            headers={EXPORT_HEADERS}
            rows={turns.map((turn) => [
              formatClock(turn.occurred_at),
              turn.model || "（未知）",
              turn.input_tokens,
              turn.output_tokens,
              turn.cache_read_tokens,
              turn.cache_creation_tokens,
              turn.reasoning_tokens,
              turn.total_tokens,
              turn.cost ?? "",
              turn.cost_note ?? "",
            ])}
          />
        </div>
      </div>
      <LoadingOverlay active={turnsLoading && turns.length > 0} className="table-scroll">
        <table>
          <thead>
            <tr>
              <th>时间</th>
              <th>模型</th>
              <th>输入</th>
              <th>输出</th>
              <th>缓存读</th>
              <th>缓存写</th>
              <th>推理</th>
              <th>总量</th>
              <th>费用</th>
              <th>原始文件</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {pagedTurns.map((turn, index) => {
              const selected =
                detail?.occurred_at === turn.occurred_at &&
                detail.source_file === turn.source_file &&
                detail.total_tokens === turn.total_tokens;
              return (
                <tr
                  key={`${turn.occurred_at}-${(currentPage - 1) * PAGE_SIZE + index}`}
                  className={selected ? "clickable selected" : "clickable"}
                  onClick={() => setDetail(turn)}
                >
                  <td>{formatClock(turn.occurred_at)}</td>
                  <td>
                    <ModelLabel name={turn.model} provider={turn.provider} />
                  </td>
                  <td>{formatTokens(turn.input_tokens)}</td>
                  <td>{formatTokens(turn.output_tokens)}</td>
                  <td>{formatTokens(turn.cache_read_tokens)}</td>
                  <td>{formatTokens(turn.cache_creation_tokens)}</td>
                  <td>{formatTokens(turn.reasoning_tokens)}</td>
                  <td>
                    <strong>{formatTokens(turn.total_tokens)}</strong>
                  </td>
                  <td>
                    {formatCost(turn.cost, turn.unpriced)}
                    {turn.cost_note ? ` · ${turn.cost_note}` : ""}
                  </td>
                  <td className="mono" title={turn.source_file}>
                    {turn.source_file}
                  </td>
                  <td>
                    <Button
                      variant="text"
                      onClick={(event) => {
                        event.stopPropagation();
                        setDetail(turn);
                      }}
                    >
                      详情
                    </Button>
                  </td>
                </tr>
              );
            })}
            {turns.length === 0 ? (
              <tr>
                <td colSpan={11} className="analytics-empty">
                  {turnsLoading ? (
                    <EmptyState icon="chat" title="正在加载明细…" />
                  ) : (
                    <EmptyState icon="chat" title="该会话暂无明细" />
                  )}
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </LoadingOverlay>
      <Pagination
        page={currentPage}
        pageCount={pageCount}
        totalCount={turns.length}
        onPageChange={setPage}
      />
      {detail ? <TurnDetailDialog turn={detail} onClose={() => setDetail(null)} /> : null}
    </div>
  );
}

function TurnDetailDialog({ turn, onClose }: { turn: TurnRow; onClose: () => void }) {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const provider = turn.provider || "（未标注）";

  return (
    <div
      className="turn-detail-backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        className="turn-detail-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="turn-detail-title"
      >
        <div className="turn-detail-head">
          <div>
            <h3 id="turn-detail-title">每轮详情</h3>
            <p className="panel-note">
              {formatClock(turn.occurred_at)} · {relativeTime(turn.occurred_at)}
            </p>
          </div>
          <Button variant="icon" onClick={onClose} aria-label="关闭详情">
            <Icon name="close" size={14} />
          </Button>
        </div>
        <dl className="turn-detail-meta">
          <div>
            <dt>模型</dt>
            <dd>
              <ModelLabel name={turn.model} provider={turn.provider} />
            </dd>
          </div>
          <div>
            <dt>Provider</dt>
            <dd>
              {provider}
              {turn.provider ? ` · ${providerChannel(turn.provider)}` : ""}
            </dd>
          </div>
        </dl>
        <div className="turn-detail-stats">
          {TOKEN_FIELDS.map((field) => (
            <div key={field.key} className={field.key === "total_tokens" ? "is-total" : undefined}>
              <span>{field.label}</span>
              <strong>{formatTokens(turn[field.key])}</strong>
            </div>
          ))}
        </div>
        <dl className="turn-detail-meta">
          <div>
            <dt>费用</dt>
            <dd>{formatCost(turn.cost, turn.unpriced)}</dd>
          </div>
          <div>
            <dt>费用来源</dt>
            <dd>{turn.cost_note ?? "—"}</dd>
          </div>
          <div className="turn-detail-file">
            <dt>原始文件</dt>
            <dd className="mono" title={turn.source_file}>
              {turn.source_file || "—"}
            </dd>
          </div>
        </dl>
      </div>
    </div>
  );
}
