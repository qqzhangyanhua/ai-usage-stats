import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { Icon } from "../icons";
import {
  applicationLabel,
  formatClock,
  humanStatus,
  projectLabel,
  relativeTime,
} from "../lib/format";
import type {
  ConversationDetailDto,
  ConversationPage,
  ConversationSessionRow,
} from "../types";
import { EmptyState } from "./EmptyState";
import { LoadingOverlay } from "./LoadingOverlay";
import { Pagination } from "./Pagination";
import { Spinner } from "./Spinner";
import { Button } from "./ui/Button";
import { SearchField } from "./ui/Field";

const PAGE_SIZE = 20;

function capabilityLabel(capability: string): string {
  return capability === "messages" ? "基础正文" : capability;
}

function statusLabel(status: string): string {
  return status === "experimental" ? "实验性" : status;
}

function sessionTime(session: ConversationSessionRow): string {
  return session.ended_at || session.started_at;
}

export function Conversations({
  revision,
  onError,
}: {
  revision: number;
  onError?: (error: unknown) => void;
}) {
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const [pageData, setPageData] = useState<ConversationPage>({ rows: [], total: 0 });
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [selected, setSelected] = useState<ConversationSessionRow | null>(null);
  const [detail, setDetail] = useState<ConversationDetailDto | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const catalogGeneration = useRef(0);
  const detailGeneration = useRef(0);

  useEffect(() => {
    const timer = window.setTimeout(() => setSearch(searchInput.trim()), 300);
    return () => window.clearTimeout(timer);
  }, [searchInput]);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- 搜索条件变化后分页必须回到第一页
    setPage(1);
  }, [search]);

  useEffect(() => {
    const generation = ++catalogGeneration.current;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- 请求开始时显示局部加载状态
    setCatalogLoading(true);
    setCatalogError(null);
    invoke<ConversationPage>("get_conversation_sessions_page", {
      query: {
        search: search || null,
        page,
        page_size: PAGE_SIZE,
      },
    })
      .then((result) => {
        if (generation === catalogGeneration.current) {
          setPageData(result);
        }
      })
      .catch((error) => {
        if (generation === catalogGeneration.current) {
          setCatalogError(humanStatus(error));
          onError?.(error);
        }
      })
      .finally(() => {
        if (generation === catalogGeneration.current) {
          setCatalogLoading(false);
        }
      });
  }, [revision, search, page, onError]);

  const loadDetail = useCallback(
    (session: ConversationSessionRow) => {
      const generation = ++detailGeneration.current;
      setSelected(session);
      setDetail(null);
      setDetailError(null);
      setDetailLoading(true);
      invoke<ConversationDetailDto>("get_conversation_detail", {
        source: session.source,
        sessionId: session.session_id,
      })
        .then((result) => {
          if (generation === detailGeneration.current) {
            setDetail(result);
          }
        })
        .catch((error) => {
          if (generation === detailGeneration.current) {
            setDetailError(humanStatus(error));
            onError?.(error);
          }
        })
        .finally(() => {
          if (generation === detailGeneration.current) {
            setDetailLoading(false);
          }
        });
    },
    [onError],
  );

  function closeDetail() {
    detailGeneration.current += 1;
    setSelected(null);
    setDetail(null);
    setDetailError(null);
    setDetailLoading(false);
  }

  if (selected) {
    const session = detail?.session ?? selected;
    return (
      <div className="conversation-detail-view">
        <section className="panel conversation-detail-head">
          <div className="conversation-detail-actions">
            <Button onClick={closeDetail} size="sm">
              <Icon name="chevron" size={13} />
              返回目录
            </Button>
            <span className={`conversation-status status-${session.support_status}`}>
              {statusLabel(session.support_status)}
            </span>
          </div>
          <div className="conversation-detail-title">
            <span>{applicationLabel(session.source)}</span>
            <h2>{session.title}</h2>
          </div>
          <dl className="conversation-meta">
            <div>
              <dt>会话 ID</dt>
              <dd className="mono" title={session.session_id}>
                {session.session_id}
              </dd>
            </div>
            <div>
              <dt>项目</dt>
              <dd title={session.project}>{projectLabel(session.project)}</dd>
            </div>
            <div>
              <dt>模型</dt>
              <dd>{session.model || "未标注"}</dd>
            </div>
            <div>
              <dt>开始时间</dt>
              <dd>{formatClock(session.started_at)}</dd>
            </div>
            <div>
              <dt>结束时间</dt>
              <dd>{formatClock(session.ended_at)}</dd>
            </div>
            <div>
              <dt>可用能力</dt>
              <dd>
                {session.capabilities.length > 0
                  ? session.capabilities.map(capabilityLabel).join("、")
                  : "仅元数据"}
              </dd>
            </div>
            <div className="conversation-source-file">
              <dt>原始文件</dt>
              <dd className="mono">{session.source_file}</dd>
            </div>
          </dl>
        </section>

        <section className="conversation-thread" aria-label="对话正文" aria-busy={detailLoading}>
          {detailLoading ? (
            <EmptyState icon="chat" title="正在读取原始会话…" />
          ) : detailError ? (
            <div className="conversation-load-error" role="alert">
              <EmptyState
                icon="alertTriangle"
                tone="warn"
                title="无法读取对话详情"
                hint={detailError}
              />
              <Button onClick={() => loadDetail(selected)}>重新读取</Button>
            </div>
          ) : detail && detail.messages.length > 0 ? (
            detail.messages.map((message, index) => (
              <article
                className={`conversation-message role-${message.role}`}
                key={`${message.occurred_at}-${message.role}-${index}`}
              >
                <header>
                  <strong>{message.role === "user" ? "用户" : "助手"}</strong>
                  <time dateTime={message.occurred_at}>{formatClock(message.occurred_at)}</time>
                </header>
                <div className="conversation-message-text">{message.text}</div>
              </article>
            ))
          ) : (
            <EmptyState
              icon="chat"
              title="这条会话暂无可读正文"
              hint="当前版本只展示 Codex 的用户与助手基础文本消息。"
            />
          )}
        </section>
      </div>
    );
  }

  const { rows, total } = pageData;
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));

  return (
    <section className="panel conversation-catalog">
      <div className="panel-head conversation-catalog-head">
        <div>
          <h2>本地会话目录</h2>
          <p className="panel-note">目录只索引元数据；正文仅在进入详情后读取。</p>
        </div>
        <SearchField
          value={searchInput}
          onChange={setSearchInput}
          placeholder="搜索标题、来源、项目、模型、ID 或时间"
          ariaLabel="搜索对话记录"
        />
        <span className="muted conversation-total">
          共 {total} 条
          {catalogLoading ? (
            <span className="inline-loading">
              <Spinner size={12} />
              加载中…
            </span>
          ) : null}
        </span>
      </div>

      {catalogError && rows.length === 0 ? (
        <div role="alert">
          <EmptyState
            icon="alertTriangle"
            tone="warn"
            title="无法加载对话目录"
            hint={catalogError}
          />
        </div>
      ) : (
        <LoadingOverlay
          active={catalogLoading && rows.length > 0}
          className="table-scroll conversation-table-scroll"
        >
          <table className="conversation-table">
            <thead>
              <tr>
                <th>标题</th>
                <th>来源</th>
                <th>项目</th>
                <th>模型</th>
                <th>时间</th>
                <th>能力</th>
                <th>状态</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => {
                const time = sessionTime(row);
                return (
                  <tr
                    key={`${row.source}-${row.session_id}`}
                    className="clickable"
                    tabIndex={0}
                    aria-label={`打开对话：${row.title}`}
                    onClick={() => loadDetail(row)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        loadDetail(row);
                      }
                    }}
                  >
                    <td title={row.title}>
                      <div className="conversation-title-cell">
                        <strong>{row.title}</strong>
                        <span className="mono">{row.session_id}</span>
                      </div>
                    </td>
                    <td>{applicationLabel(row.source)}</td>
                    <td title={row.project}>{projectLabel(row.project)}</td>
                    <td>{row.model || "未标注"}</td>
                    <td title={formatClock(time)}>{time ? relativeTime(time) : "—"}</td>
                    <td>
                      <div className="conversation-capabilities">
                        {row.capabilities.length > 0 ? (
                          row.capabilities.map((capability) => (
                            <span key={capability}>{capabilityLabel(capability)}</span>
                          ))
                        ) : (
                          <span>仅元数据</span>
                        )}
                      </div>
                    </td>
                    <td>
                      <span className={`conversation-status status-${row.support_status}`}>
                        {statusLabel(row.support_status)}
                      </span>
                    </td>
                  </tr>
                );
              })}
              {rows.length === 0 ? (
                <tr>
                  <td colSpan={7} className="analytics-empty">
                    {catalogLoading ? (
                      <EmptyState icon="chat" title="正在加载对话目录…" />
                    ) : (
                      <EmptyState
                        icon="chat"
                        title="当前条件下暂无对话记录"
                        hint="请确认本机已有 Codex 会话，并执行一次刷新。"
                      />
                    )}
                  </td>
                </tr>
              ) : null}
            </tbody>
          </table>
        </LoadingOverlay>
      )}
      <Pagination page={page} pageCount={pageCount} totalCount={total} onPageChange={setPage} />
    </section>
  );
}
