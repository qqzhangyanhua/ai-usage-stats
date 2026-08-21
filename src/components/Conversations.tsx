import { invoke } from "@tauri-apps/api/core";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
  type UIEvent,
} from "react";
import { Icon } from "../icons";
import {
  createConversationRequestGate,
  isConversationResponseCurrent,
  isNearConversationBottom,
  nextConversationFollowState,
} from "../lib/conversationFollow";
import {
  applicationLabel,
  formatClock,
  formatTokens,
  humanStatus,
  projectLabel,
  relativeTime,
} from "../lib/format";
import type {
  ConversationDetailDto,
  ConversationDetailStateDto,
  ConversationEvent,
  ConversationEventActor,
  ConversationEventCapabilityStatus,
  ConversationEventKind,
  ConversationPage,
  ConversationSessionRow,
  ConversationUsageRecord,
} from "../types";
import { EmptyState } from "./EmptyState";
import { LoadingOverlay } from "./LoadingOverlay";
import { Pagination } from "./Pagination";
import { SessionResumeCommand } from "./SessionResumeCommand";
import { Spinner } from "./Spinner";
import { Button } from "./ui/Button";
import { SearchField } from "./ui/Field";
import { Segmented } from "./ui/Segmented";
import { ModelLabel } from "./VendorIcon";

const PAGE_SIZE = 20;

type DetailTab = "events" | "usage";

const DETAIL_TABS = [
  { value: "events", label: "完整事件" },
  { value: "usage", label: "用量明细" },
] as const;

const EVENT_LABELS: Record<ConversationEventKind, string> = {
  message: "消息",
  plan: "计划",
  tool_call: "工具调用",
  tool_result: "工具结果",
  model_change: "模型切换",
  error: "错误",
  system_status: "系统状态",
  unadapted: "尚未适配",
};

const CAPABILITY_LABELS: Record<string, string> = {
  messages: "基础正文",
  events: "完整事件",
  usage: "用量明细",
};

const ACTOR_LABELS: Record<ConversationEventActor, string> = {
  user: "用户",
  assistant: "助手",
  tool: "工具",
};

const CAPABILITY_STATUS_LABELS: Record<ConversationEventCapabilityStatus, string> = {
  complete: "完整",
  missing_timestamp: "时间缺失",
  unadapted: "尚未适配",
  unadapted_missing_timestamp: "尚未适配、时间缺失",
};

function capabilityLabel(capability: string): string {
  return CAPABILITY_LABELS[capability] ?? capability;
}

function actorLabel(actor: ConversationEventActor): string {
  return ACTOR_LABELS[actor];
}

function capabilityStatusLabel(status: ConversationEventCapabilityStatus): string {
  return CAPABILITY_STATUS_LABELS[status];
}

function statusLabel(status: string): string {
  return status === "experimental" ? "实验性" : status;
}

function sessionTime(session: ConversationSessionRow): string {
  return session.ended_at || session.started_at;
}

function hasEventDetails(details: unknown): boolean {
  if (details == null) {
    return false;
  }
  if (Array.isArray(details)) {
    return details.length > 0;
  }
  if (typeof details === "object") {
    return Object.keys(details).length > 0;
  }
  return true;
}

function prettyDetails(details: unknown): string {
  try {
    return JSON.stringify(details, null, 2) ?? String(details);
  } catch {
    return String(details);
  }
}

function EventTimeline({
  events,
  timelineRef,
  onScroll,
}: {
  events: ConversationEvent[];
  timelineRef: RefObject<HTMLDivElement | null>;
  onScroll: (event: UIEvent<HTMLDivElement>) => void;
}) {
  return (
    <div
      className="conversation-timeline"
      aria-label="完整事件列表"
      ref={timelineRef}
      onScroll={onScroll}
    >
      {events.length === 0 ? (
        <EmptyState icon="chat" title="这条会话暂无事件" hint="当前会话没有可展示的语义事件。" />
      ) : null}
      {events.map((event) => {
        const label = EVENT_LABELS[event.kind];
        const showDetails =
          event.kind === "unadapted" ||
          ((event.kind === "plan" || event.kind === "tool_call" || event.kind === "tool_result") &&
            hasEventDetails(event.details));
        const showCapabilityStatus =
          event.capability_status !== "complete" &&
          event.kind !== "unadapted" &&
          event.occurred_at !== null;
        return (
          <article
            className={`conversation-event event-${event.kind.replaceAll("_", "-")}`}
            key={event.sequence}
          >
            <header className="conversation-event-meta">
              <strong>{label}</strong>
              {event.occurred_at ? (
                <time dateTime={event.occurred_at}>{formatClock(event.occurred_at)}</time>
              ) : (
                <span className="conversation-event-missing-time">时间缺失</span>
              )}
            </header>
            <div className="conversation-event-content">
              {event.kind === "unadapted" ? (
                <span className="conversation-unadapted-state">尚未适配</span>
              ) : showCapabilityStatus ? (
                <span className="conversation-capability-status">
                  {capabilityStatusLabel(event.capability_status)}
                </span>
              ) : null}
              {event.actor || event.name ? (
                <div className="conversation-event-identity">
                  {event.actor ? <span>{actorLabel(event.actor)}</span> : null}
                  {event.name ? <code>{event.name}</code> : null}
                </div>
              ) : null}
              {event.text ? <div className="conversation-event-text">{event.text}</div> : null}
              {showDetails ? (
                <details className="conversation-event-details">
                  <summary>{event.kind === "unadapted" ? "查看原始事件" : "查看详细数据"}</summary>
                  <pre>{prettyDetails(event.details)}</pre>
                </details>
              ) : null}
              {!event.text && !showDetails && !event.actor && !event.name ? (
                <span className="muted">无附加内容</span>
              ) : null}
            </div>
          </article>
        );
      })}
    </div>
  );
}

function UsageRecordsTable({ records }: { records: ConversationUsageRecord[] }) {
  return (
    <div className="table-scroll conversation-usage-scroll">
      <table className="conversation-usage-table">
        <thead>
          <tr>
            <th>时间</th>
            <th>模型</th>
            <th>Provider</th>
            <th>输入</th>
            <th>输出</th>
            <th>缓存读</th>
            <th>缓存写</th>
            <th>推理</th>
            <th>总量</th>
          </tr>
        </thead>
        <tbody>
          {records.map((record, index) => (
            <tr key={`${record.occurred_at}-${record.source_file}-${index}`}>
              <td>{formatClock(record.occurred_at)}</td>
              <td>
                <ModelLabel name={record.model} provider={record.provider} />
              </td>
              <td>{record.provider || "未标注"}</td>
              <td>{formatTokens(record.input_tokens)}</td>
              <td>{formatTokens(record.output_tokens)}</td>
              <td>{formatTokens(record.cache_read_tokens)}</td>
              <td>{formatTokens(record.cache_creation_tokens)}</td>
              <td>{formatTokens(record.reasoning_tokens)}</td>
              <td>
                <strong>{formatTokens(record.total_tokens)}</strong>
              </td>
            </tr>
          ))}
          {records.length === 0 ? (
            <tr>
              <td colSpan={9} className="analytics-empty">
                <EmptyState icon="chat" title="这条会话暂无用量明细" />
              </td>
            </tr>
          ) : null}
        </tbody>
      </table>
    </div>
  );
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
  const [detailTab, setDetailTab] = useState<DetailTab>("events");
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);
  const [detailFileAvailable, setDetailFileAvailable] = useState(true);
  const [unseenCount, setUnseenCount] = useState(0);
  const catalogGeneration = useRef(0);
  const detailGeneration = useRef(0);
  const detailRequestGate = useRef(createConversationRequestGate());
  const mountedRef = useRef(true);
  const detailRef = useRef<ConversationDetailDto | null>(null);
  const detailRevisionRef = useRef("");
  const timelineRef = useRef<HTMLDivElement>(null);
  const wasAtBottomRef = useRef(true);
  const pendingScrollRef = useRef(false);
  const unseenCountRef = useRef(0);

  const isDetailResponseCurrent = useCallback(
    (generation: number) =>
      isConversationResponseCurrent({
        mounted: mountedRef.current,
        generation,
        currentGeneration: detailGeneration.current,
      }),
    [],
  );

  const replaceDetail = useCallback((result: ConversationDetailDto, followUpdates: boolean) => {
    if (followUpdates) {
      const follow = nextConversationFollowState({
        previousCount: detailRef.current?.events.length ?? 0,
        nextCount: result.events.length,
        wasAtBottom: wasAtBottomRef.current,
        unseenCount: unseenCountRef.current,
      });
      pendingScrollRef.current = follow.shouldScroll;
      unseenCountRef.current = follow.unseenCount;
      setUnseenCount(follow.unseenCount);
    } else {
      pendingScrollRef.current = true;
      wasAtBottomRef.current = true;
      unseenCountRef.current = 0;
      setUnseenCount(0);
    }
    detailRef.current = result;
    detailRevisionRef.current = result.revision;
    setDetail(result);
    setDetailFileAvailable(result.session.file_available);
    setDetailError(null);
    setPollError(null);
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      detailGeneration.current += 1;
    };
  }, []);

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
      const needsRequest = session.file_available;
      if (needsRequest && !detailRequestGate.current.acquire()) {
        return;
      }
      const generation = ++detailGeneration.current;
      setSelected(session);
      setDetailTab("events");
      detailRef.current = null;
      detailRevisionRef.current = "";
      setDetail(null);
      setDetailError(null);
      setPollError(null);
      setDetailFileAvailable(session.file_available);
      unseenCountRef.current = 0;
      setUnseenCount(0);
      wasAtBottomRef.current = true;
      pendingScrollRef.current = false;

      if (!needsRequest) {
        setDetailLoading(false);
        return;
      }

      setDetailLoading(true);
      invoke<ConversationDetailDto>("get_conversation_detail", {
        source: session.source,
        sessionId: session.session_id,
      })
        .then((result) => {
          if (isDetailResponseCurrent(generation)) {
            replaceDetail(result, false);
          }
        })
        .catch((error) => {
          if (isDetailResponseCurrent(generation)) {
            setDetailError(humanStatus(error));
            onError?.(error);
          }
        })
        .finally(() => {
          detailRequestGate.current.release();
          if (isDetailResponseCurrent(generation)) {
            setDetailLoading(false);
          }
        });
    },
    [isDetailResponseCurrent, onError, replaceDetail],
  );

  const selectedSource = selected?.source ?? null;
  const selectedSessionId = selected?.session_id ?? null;

  useEffect(() => {
    if (!selectedSource || !selectedSessionId) {
      return;
    }

    let cancelled = false;
    const poll = async () => {
      if (!detailRequestGate.current.acquire()) {
        return;
      }
      const generation = detailGeneration.current;
      try {
        const state = await invoke<ConversationDetailStateDto>("get_conversation_detail_state", {
          source: selectedSource,
          sessionId: selectedSessionId,
          knownRevision: detailRevisionRef.current,
        });
        if (cancelled || !isDetailResponseCurrent(generation)) {
          return;
        }

        setDetailFileAvailable(state.file_available);
        setPollError(null);
        if (state.changed && state.file_available) {
          const result = await invoke<ConversationDetailDto>("get_conversation_detail", {
            source: selectedSource,
            sessionId: selectedSessionId,
          });
          if (!cancelled && isDetailResponseCurrent(generation)) {
            replaceDetail(result, true);
          }
        }
      } catch (error) {
        if (!cancelled && isDetailResponseCurrent(generation)) {
          setPollError(humanStatus(error));
        }
      } finally {
        detailRequestGate.current.release();
      }
    };

    const timer = window.setInterval(poll, 2_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [isDetailResponseCurrent, replaceDetail, selectedSessionId, selectedSource]);

  useLayoutEffect(() => {
    if (!detail || detailTab !== "events" || !pendingScrollRef.current) {
      return;
    }
    const frame = window.requestAnimationFrame(() => {
      const timeline = timelineRef.current;
      if (timeline) {
        timeline.scrollTop = timeline.scrollHeight;
        wasAtBottomRef.current = true;
      }
      pendingScrollRef.current = false;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [detail, detailTab]);

  function handleTimelineScroll(event: UIEvent<HTMLDivElement>) {
    const isAtBottom = isNearConversationBottom(event.currentTarget);
    wasAtBottomRef.current = isAtBottom;
    if (isAtBottom) {
      unseenCountRef.current = 0;
      setUnseenCount(0);
    }
  }

  function scrollToLatestEvent() {
    const timeline = timelineRef.current;
    if (timeline) {
      timeline.scrollTop = timeline.scrollHeight;
    } else {
      pendingScrollRef.current = true;
    }
    wasAtBottomRef.current = true;
    unseenCountRef.current = 0;
    setUnseenCount(0);
  }

  function closeDetail() {
    detailGeneration.current += 1;
    setSelected(null);
    setDetailTab("events");
    detailRef.current = null;
    detailRevisionRef.current = "";
    setDetail(null);
    setDetailError(null);
    setPollError(null);
    setDetailFileAvailable(true);
    unseenCountRef.current = 0;
    setUnseenCount(0);
    pendingScrollRef.current = false;
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
            <div className="conversation-detail-statuses">
              <span className={`conversation-status status-${session.support_status}`}>
                {statusLabel(session.support_status)}
              </span>
              {!detailFileAvailable ? (
                <span className="conversation-file-unavailable">
                  <Icon name="alertTriangle" size={12} />
                  原文件已删除
                </span>
              ) : null}
            </div>
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
          <SessionResumeCommand source={session.source} sessionId={session.session_id} />
        </section>

        <section className="conversation-detail-body" aria-busy={detailLoading}>
          <div className="conversation-detail-tabs">
            <Segmented
              value={detailTab}
              options={DETAIL_TABS}
              disabled={detailLoading || Boolean(detailError)}
              ariaLabel="对话详情视图"
              onChange={setDetailTab}
            />
            {detail ? (
              <span className="muted">
                {detailTab === "events"
                  ? `${detail.events.length} 条事件`
                  : `${detail.usage_records.length} 条记录`}
              </span>
            ) : null}
          </div>
          {!detailFileAvailable ? (
            <div className="conversation-detail-notice" role="status">
              <Icon name="alertTriangle" size={16} />
              <div>
                <strong>原文件已删除，详情不可继续读取</strong>
                <span>
                  {detail
                    ? "当前显示的是已加载快照；文件恢复后将自动更新。"
                    : "仍可查看目录元数据；文件恢复后将自动读取详情。"}
                </span>
              </div>
            </div>
          ) : null}
          {pollError ? (
            <div className="conversation-detail-notice" role="status">
              <Icon name="alertTriangle" size={16} />
              <div>
                <strong>暂时无法检查最新内容</strong>
                <span>{pollError}；后台将继续重试。</span>
              </div>
            </div>
          ) : null}
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
          ) : detail ? (
            detailTab === "events" ? (
              <div className="conversation-events-view">
                <EventTimeline
                  events={detail.events}
                  timelineRef={timelineRef}
                  onScroll={handleTimelineScroll}
                />
                <div className="conversation-follow-control" aria-live="polite">
                  {unseenCount > 0 ? (
                    <Button size="sm" onClick={scrollToLatestEvent}>
                      <Icon name="chevron" size={13} className="conversation-follow-icon" />
                      新增 {unseenCount} 条事件
                    </Button>
                  ) : null}
                </div>
              </div>
            ) : (
              <UsageRecordsTable records={detail.usage_records} />
            )
          ) : null}
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
                      <div className="conversation-row-statuses">
                        <span className={`conversation-status status-${row.support_status}`}>
                          {statusLabel(row.support_status)}
                        </span>
                        {!row.file_available ? (
                          <span className="conversation-file-unavailable">
                            <Icon name="alertTriangle" size={12} />
                            原文件已删除
                          </span>
                        ) : null}
                      </div>
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
