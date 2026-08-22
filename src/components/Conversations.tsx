import { invoke } from "@tauri-apps/api/core";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
  type UIEvent,
} from "react";
import { Icon } from "../icons";
import {
  conversationJumpBehavior,
  conversationJumpScrollTop,
  conversationTimelineScrollTarget,
  createConversationRequestGate,
  isConversationResponseCurrent,
  isNearConversationBottom,
  isNearConversationTop,
  nextConversationFollowState,
  nextConversationRevisionPollState,
  type ConversationJumpEdge,
} from "../lib/conversationFollow";
import { ConversationMarkdown } from "../lib/conversationMarkdown";
import {
  currentConversationFrame,
  type ConversationDetailTab,
  initialConversationNavigationState,
  shouldRequestConversationDetail,
  transitionConversationNavigation,
} from "../lib/conversationNavigation";
import { formatClock, formatTokens, humanStatus } from "../lib/format";
import type {
  ConversationAttachment,
  ConversationAgentLink,
  ConversationAttachmentContentDto,
  ConversationDetailDto,
  ConversationDetailStateDto,
  ConversationEvent,
  ConversationEventActor,
  ConversationEventCapabilityStatus,
  ConversationEventContentDto,
  ConversationEventKind,
  ConversationFocus,
  ConversationPage,
  ConversationSessionRow,
  ConversationUsageRecord,
  Filter,
} from "../types";
import { ConversationCatalogRow } from "./ConversationCatalogRow";
import { ConversationDetailHead } from "./ConversationDetailHead";
import { ConversationJumpBar } from "./ConversationJumpBar";
import { CursorSessionDetail } from "./CursorSessionDetail";
import { EmptyState } from "./EmptyState";
import { LoadingOverlay } from "./LoadingOverlay";
import { Pagination } from "./Pagination";
import { Spinner } from "./Spinner";
import type { ConversationExportFormat } from "./type";
import { Button } from "./ui/Button";
import { SearchField } from "./ui/Field";
import { Segmented } from "./ui/Segmented";
import { ModelLabel } from "./VendorIcon";

const PAGE_SIZE = 20;

type ConversationDetailRequestIntent = {
  session: ConversationSessionRow;
  key: string;
  generation: number;
  followUpdates: boolean;
};
const DETAIL_TABS: { value: ConversationDetailTab; label: string }[] = [
  { value: "events", label: "完整事件" },
  { value: "usage", label: "用量明细" },
];
const BEHAVIOR_TAB: { value: ConversationDetailTab; label: string } = {
  value: "behavior",
  label: "行为统计",
};

const AGENT_LINK_LABELS = {
  linked: "已关联",
  missing_source: "子会话源不可用",
  unresolved: "无法确定子会话",
  conflict: "关联冲突",
  cycle: "循环关联",
} as const;

const AGENT_CAPABILITY_MESSAGES = {
  partial: "部分子代理关系可确定，其余会话保持分离。",
  unavailable: "无法确定子代理关系，相关会话保持独立。",
} as const;

function conversationKey(session: Pick<ConversationSessionRow, "source" | "session_id">) {
  return `${session.source}\u{1f}${session.session_id}`;
}

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

function actorLabel(actor: ConversationEventActor): string {
  return ACTOR_LABELS[actor];
}

function capabilityStatusLabel(status: ConversationEventCapabilityStatus): string {
  return CAPABILITY_STATUS_LABELS[status];
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

function formatBytes(bytes: number | null): string {
  if (bytes === null) {
    return "大小未知";
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = units[0];
  for (let index = 1; value >= 1024 && index < units.length; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${unit}`;
}

function attachmentStatusText(attachment: ConversationAttachment): string {
  if (attachment.status === "missing") {
    return "原附件已不存在";
  }
  if (attachment.status === "unsupported") {
    return "无法在应用内加载";
  }
  return attachment.status === "embedded" ? "已嵌入" : "可用";
}

function attachmentSignature(attachment: ConversationAttachment): string {
  return `${attachment.kind}\u0000${attachment.status}\u0000${attachment.original_path}\u0000${attachment.size_bytes ?? ""}`;
}

function attachmentRequestKey(attachment: ConversationAttachment): string {
  return `${attachment.id}\u0000${attachmentSignature(attachment)}`;
}

type ImageCacheEntry = { signature: string; dataUrl: string };
type AsyncLoadState = "loading" | "error";

function useKeyedAsyncLoad<Key extends string | number>() {
  const [states, setStates] = useState<Partial<Record<Key, AsyncLoadState>>>({});
  const [errors, setErrors] = useState<Partial<Record<Key, string>>>({});
  const mounted = useRef(true);
  const inFlight = useRef(new Set<Key>());

  useEffect(() => {
    const activeRequests = inFlight.current;
    mounted.current = true;
    return () => {
      mounted.current = false;
      activeRequests.clear();
    };
  }, []);

  const run = useCallback(
    async <Result,>(key: Key, task: () => Promise<Result>, onSuccess: (result: Result) => void) => {
      if (inFlight.current.has(key)) {
        return;
      }
      inFlight.current.add(key);
      setStates((current) => ({ ...current, [key]: "loading" }));
      setErrors((current) => {
        const next = { ...current };
        delete next[key];
        return next;
      });
      try {
        const result = await task();
        if (!mounted.current) {
          return;
        }
        onSuccess(result);
        setStates((current) => {
          const next = { ...current };
          delete next[key];
          return next;
        });
      } catch (error) {
        if (mounted.current) {
          setStates((current) => ({ ...current, [key]: "error" }));
          setErrors((current) => ({ ...current, [key]: humanStatus(error) }));
        }
      } finally {
        inFlight.current.delete(key);
      }
    },
    [],
  );

  return { states, errors, run };
}

function ImageDialog({
  name,
  dataUrl,
  onClose,
}: {
  name: string;
  dataUrl: string;
  onClose: () => void;
}) {
  const titleId = `conversation-image-${encodeURIComponent(name).replaceAll("%", "")}`;
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previousFocus =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const dialog = dialogRef.current;
    const focusable = () =>
      Array.from(
        dialog?.querySelectorAll<HTMLElement>(
          'button:not([disabled]), a[href], input:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ) ?? [],
      );
    focusable()[0]?.focus();

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
        return;
      }
      if (event.key !== "Tab") {
        return;
      }
      const controls = focusable();
      if (controls.length === 0) {
        event.preventDefault();
        dialog?.focus();
        return;
      }
      const first = controls[0];
      const last = controls[controls.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      previousFocus?.focus();
    };
  }, [onClose]);

  return (
    <div
      className="conversation-image-backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        ref={dialogRef}
        className="conversation-image-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
      >
        <header>
          <h3 id={titleId}>{name}</h3>
          <Button variant="icon" onClick={onClose} aria-label="关闭图片预览">
            <Icon name="close" size={15} />
          </Button>
        </header>
        <div className="conversation-image-stage">
          <img src={dataUrl} alt={name} />
        </div>
      </div>
    </div>
  );
}

type EventTimelineProps = {
  events: ConversationEvent[];
  source: string;
  sessionId: string;
  agentLinks: ConversationAgentLink[];
  expandedRelationshipIds: string[];
  childDetails: Record<string, ConversationDetailDto>;
  childLoading: Record<string, boolean>;
  depth?: number;
  onToggleChild: (link: ConversationAgentLink) => void;
  onOpenChild: (link: ConversationAgentLink) => void;
  onEventContentLoaded: (
    source: string,
    sessionId: string,
    content: ConversationEventContentDto,
  ) => void;
  timelineRef?: RefObject<HTMLDivElement | null>;
  onScroll?: (event: UIEvent<HTMLDivElement>) => void;
};

function EventTimeline({
  events,
  source,
  sessionId,
  agentLinks,
  expandedRelationshipIds,
  childDetails,
  childLoading,
  depth = 0,
  onToggleChild,
  onOpenChild,
  onEventContentLoaded,
  timelineRef,
  onScroll,
}: EventTimelineProps) {
  const {
    states: eventLoads,
    errors: eventErrors,
    run: runEventLoad,
  } = useKeyedAsyncLoad<string>();
  const {
    states: thumbnailLoads,
    errors: thumbnailErrors,
    run: runThumbnailLoad,
  } = useKeyedAsyncLoad<string>();
  const {
    states: imageLoads,
    errors: imageErrors,
    run: runImageLoad,
  } = useKeyedAsyncLoad<string>();
  const [thumbnailData, setThumbnailData] = useState<Record<string, ImageCacheEntry>>({});
  const requestedThumbnails = useRef(new Map<string, string>());
  const [imageData, setImageData] = useState<Record<string, ImageCacheEntry>>({});
  const [openImage, setOpenImage] = useState<{ name: string; dataUrl: string } | null>(null);
  const currentAttachmentSignatures = useMemo(
    () =>
      new Map(
        events.flatMap((event) =>
          event.attachments.map(
            (attachment) => [attachment.id, attachmentSignature(attachment)] as const,
          ),
        ),
      ),
    [events],
  );
  const attachmentSignatures = useRef(currentAttachmentSignatures);
  useEffect(() => {
    attachmentSignatures.current = currentAttachmentSignatures;
  }, [currentAttachmentSignatures]);

  async function loadFullEvent(eventId: string) {
    await runEventLoad(
      eventId,
      () =>
        invoke<ConversationEventContentDto>("get_conversation_event_content", {
          source,
          sessionId,
          eventId,
        }),
      (content) => onEventContentLoaded(source, sessionId, content),
    );
  }

  const loadThumbnail = useCallback(
    async (attachment: ConversationAttachment, retry = false) => {
      if (retry) {
        requestedThumbnails.current.delete(attachment.id);
      }
      const signature = attachmentSignature(attachment);
      if (requestedThumbnails.current.get(attachment.id) === signature) {
        return;
      }
      requestedThumbnails.current.set(attachment.id, signature);
      await runThumbnailLoad(
        attachmentRequestKey(attachment),
        () =>
          invoke<ConversationAttachmentContentDto>("get_conversation_attachment_thumbnail", {
            source,
            sessionId,
            attachmentId: attachment.id,
          }),
        (result) => {
          if (attachmentSignatures.current.get(attachment.id) === signature) {
            setThumbnailData((current) => ({
              ...current,
              [attachment.id]: { signature, dataUrl: result.data_url },
            }));
          }
        },
      );
    },
    [runThumbnailLoad, sessionId, source],
  );

  useEffect(() => {
    for (const event of events) {
      for (const attachment of event.attachments) {
        if (
          attachment.kind === "image" &&
          (attachment.status === "available" || attachment.status === "embedded")
        ) {
          void loadThumbnail(attachment);
        }
      }
    }
  }, [events, loadThumbnail]);

  async function loadImage(attachment: ConversationAttachment) {
    const signature = attachmentSignature(attachment);
    const cached = imageData[attachment.id];
    if (cached?.signature === signature) {
      setOpenImage({ name: attachment.name, dataUrl: cached.dataUrl });
      return;
    }
    await runImageLoad(
      attachmentRequestKey(attachment),
      () =>
        invoke<ConversationAttachmentContentDto>("get_conversation_attachment", {
          source,
          sessionId,
          attachmentId: attachment.id,
        }),
      (result) => {
        if (attachmentSignatures.current.get(attachment.id) === signature) {
          setImageData((current) => ({
            ...current,
            [attachment.id]: { signature, dataUrl: result.data_url },
          }));
          setOpenImage({ name: attachment.name, dataUrl: result.data_url });
        }
      },
    );
  }

  const eventIds = new Set(events.map((event) => event.event_id));
  const linksForEvent = (eventId: string) =>
    agentLinks.filter((link) => link.launch_event_id === eventId);
  const trailingLinks = agentLinks.filter(
    (link) => link.launch_event_id === null || !eventIds.has(link.launch_event_id),
  );

  function renderAgentLinks(links: ConversationAgentLink[]) {
    return links.map((link) => {
      const linkedSession = link.status === "linked" ? link.session : null;
      const expanded = expandedRelationshipIds.includes(link.relationship_id);
      const nestedDetail = linkedSession ? childDetails[conversationKey(linkedSession)] : null;
      const nestedLoading = linkedSession ? childLoading[conversationKey(linkedSession)] : false;
      const controlsId = `agent-timeline-${link.relationship_id.replaceAll(/[^a-zA-Z0-9_-]/g, "-")}`;
      return (
        <section
          className={`conversation-agent-link depth-${Math.min(depth, 3)} status-${link.status}`}
          key={link.relationship_id}
        >
          <div className="conversation-agent-link-head">
            <Button
              variant="icon"
              size="sm"
              onClick={() => onToggleChild(link)}
              disabled={!linkedSession}
              aria-label={expanded ? "收起子代理时间线" : "展开子代理时间线"}
              aria-expanded={expanded}
              aria-controls={controlsId}
              title={expanded ? "收起子代理时间线" : "展开子代理时间线"}
            >
              <Icon name="chevron" size={13} />
            </Button>
            <div className="conversation-agent-link-title">
              <strong>{linkedSession?.title || link.session_id || "未解析的子代理"}</strong>
              <span>{AGENT_LINK_LABELS[link.status]}</span>
              {link.session_id ? <code>{link.session_id}</code> : null}
            </div>
            {linkedSession ? (
              <Button
                variant="text"
                size="sm"
                onClick={() => onOpenChild(link)}
                data-relationship-id={link.relationship_id}
              >
                打开详情
              </Button>
            ) : null}
          </div>
          {expanded && linkedSession ? (
            <div className="conversation-agent-link-body" id={controlsId}>
              {nestedLoading && !nestedDetail ? (
                <div className="conversation-agent-loading">
                  <Spinner size={14} />
                </div>
              ) : nestedDetail ? (
                <EventTimeline
                  events={nestedDetail.events}
                  source={nestedDetail.session.source}
                  sessionId={nestedDetail.session.session_id}
                  agentLinks={nestedDetail.agent_relations.children}
                  expandedRelationshipIds={expandedRelationshipIds}
                  childDetails={childDetails}
                  childLoading={childLoading}
                  depth={depth + 1}
                  onToggleChild={onToggleChild}
                  onOpenChild={onOpenChild}
                  onEventContentLoaded={onEventContentLoaded}
                />
              ) : (
                <span className="conversation-inline-error">子会话内容不可用</span>
              )}
            </div>
          ) : null}
        </section>
      );
    });
  }

  if (events.length === 0 && agentLinks.length === 0) {
    return (
      <EmptyState icon="chat" title="这条会话暂无事件" hint="当前会话没有可展示的语义事件。" />
    );
  }

  return (
    <>
      <div
        className="conversation-timeline"
        aria-label="完整事件列表"
        ref={timelineRef}
        onScroll={onScroll}
      >
        <div className="conversation-timeline-stack">
          {events.map((event) => {
            const label = EVENT_LABELS[event.kind];
            const showDetails =
              event.kind === "unadapted" ||
              ((event.kind === "plan" ||
                event.kind === "tool_call" ||
                event.kind === "tool_result") &&
                hasEventDetails(event.details));
            const showCapabilityStatus =
              event.capability_status !== "complete" &&
              event.kind !== "unadapted" &&
              event.occurred_at !== null;
            const usesMarkdown =
              event.kind === "message" ||
              event.kind === "plan" ||
              event.kind === "error" ||
              event.kind === "tool_result";
            const isDeferred = event.content_status === "deferred";
            return (
              <div className="conversation-event-group" key={event.event_id}>
                <article className={`conversation-event event-${event.kind.replaceAll("_", "-")}`}>
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
                    {event.text ? (
                      usesMarkdown ? (
                        <ConversationMarkdown markdown={event.text} />
                      ) : (
                        <pre className="conversation-event-text conversation-event-command">
                          {event.text}
                        </pre>
                      )
                    ) : null}
                    {isDeferred ? (
                      <div className="conversation-deferred" aria-live="polite">
                        <span>仅显示前部内容</span>
                        <Button
                          variant="text"
                          onClick={() => void loadFullEvent(event.event_id)}
                          disabled={eventLoads[event.event_id] === "loading"}
                        >
                          {eventLoads[event.event_id] === "loading" ? <Spinner size={12} /> : null}
                          加载全文
                        </Button>
                        {eventLoads[event.event_id] === "error" ? (
                          <span className="conversation-inline-error" role="alert">
                            {eventErrors[event.event_id]}
                          </span>
                        ) : null}
                      </div>
                    ) : null}
                    {showDetails ? (
                      <details className="conversation-event-details">
                        <summary>
                          {event.kind === "unadapted" ? "查看原始事件" : "查看详细数据"}
                        </summary>
                        <pre>{prettyDetails(event.details)}</pre>
                      </details>
                    ) : null}
                    {event.attachments.length > 0 ? (
                      <div className="conversation-attachments" aria-label="附件">
                        {event.attachments.map((attachment) => {
                          const signature = attachmentSignature(attachment);
                          const requestKey = attachmentRequestKey(attachment);
                          const cachedThumbnail = thumbnailData[attachment.id];
                          const thumbnailUrl =
                            cachedThumbnail?.signature === signature
                              ? cachedThumbnail.dataUrl
                              : null;
                          const thumbnailState = thumbnailLoads[requestKey];
                          const imageState = imageLoads[requestKey];
                          const canLoadImage =
                            attachment.kind === "image" &&
                            (attachment.status === "available" || attachment.status === "embedded");
                          return (
                            <div className="conversation-attachment" key={attachment.id}>
                              <div className="conversation-attachment-main">
                                <strong>{attachment.name}</strong>
                                <code>{attachment.original_path || "—"}</code>
                                <div className="conversation-attachment-meta">
                                  <span>{attachment.media_type || "未知类型"}</span>
                                  <span>{formatBytes(attachment.size_bytes)}</span>
                                  <span className={`attachment-status status-${attachment.status}`}>
                                    {attachmentStatusText(attachment)}
                                  </span>
                                </div>
                                {thumbnailState === "error" ? (
                                  <div className="conversation-attachment-action">
                                    <span className="conversation-inline-error" role="alert">
                                      {thumbnailErrors[requestKey]}
                                    </span>
                                    <Button
                                      variant="text"
                                      onClick={() => void loadThumbnail(attachment, true)}
                                    >
                                      重试缩略图
                                    </Button>
                                  </div>
                                ) : null}
                                {imageState === "error" ? (
                                  <span className="conversation-inline-error" role="alert">
                                    {imageErrors[requestKey]}
                                  </span>
                                ) : null}
                              </div>
                              {canLoadImage ? (
                                thumbnailUrl ? (
                                  <button
                                    type="button"
                                    className="conversation-image-thumb"
                                    onClick={() => void loadImage(attachment)}
                                    disabled={imageState === "loading"}
                                    aria-label={`查看原图：${attachment.name}`}
                                  >
                                    <img src={thumbnailUrl} alt="" />
                                    {imageState === "loading" ? (
                                      <span className="conversation-image-loading" aria-hidden>
                                        <Spinner size={14} />
                                      </span>
                                    ) : null}
                                  </button>
                                ) : (
                                  <div
                                    className="conversation-image-placeholder"
                                    aria-label={
                                      thumbnailState === "error" ? undefined : "正在生成缩略图"
                                    }
                                    aria-hidden={thumbnailState === "error" || undefined}
                                  >
                                    {thumbnailState === "loading" ? (
                                      <Spinner size={14} />
                                    ) : (
                                      <Icon name="alertTriangle" size={14} />
                                    )}
                                  </div>
                                )
                              ) : null}
                            </div>
                          );
                        })}
                      </div>
                    ) : null}
                    {!event.text &&
                    !showDetails &&
                    !event.actor &&
                    !event.name &&
                    event.attachments.length === 0 ? (
                      <span className="muted">无附加内容</span>
                    ) : null}
                  </div>
                </article>
                {renderAgentLinks(linksForEvent(event.event_id))}
              </div>
            );
          })}
          {renderAgentLinks(trailingLinks)}
        </div>
      </div>
      {openImage ? (
        <ImageDialog
          name={openImage.name}
          dataUrl={openImage.dataUrl}
          onClose={() => setOpenImage(null)}
        />
      ) : null}
    </>
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
  filter,
  revision,
  focus,
  onFocusConsumed,
  onError,
}: {
  filter: Filter;
  revision: number;
  focus?: ConversationFocus | null;
  onFocusConsumed?: () => void;
  onError?: (error: unknown) => void;
}) {
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const [pageData, setPageData] = useState<ConversationPage>({ rows: [], total: 0 });
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [navigation, setNavigation] = useState(initialConversationNavigationState);
  const [details, setDetails] = useState<Record<string, ConversationDetailDto>>({});
  const [detailLoadingByKey, setDetailLoadingByKey] = useState<Record<string, boolean>>({});
  const [detailErrorsByKey, setDetailErrorsByKey] = useState<Record<string, string>>({});
  const [fileAvailableByKey, setFileAvailableByKey] = useState<Record<string, boolean>>({});
  const [pollErrorsByKey, setPollErrorsByKey] = useState<Record<string, string>>({});
  const [unseenCount, setUnseenCount] = useState(0);
  const [atTop, setAtTop] = useState(true);
  const [atBottom, setAtBottom] = useState(true);
  const currentFrame = currentConversationFrame(navigation);
  const selected = currentFrame?.session ?? null;
  const selectedKey = selected ? conversationKey(selected) : null;
  const detail = selectedKey ? (details[selectedKey] ?? null) : null;
  const detailTab: ConversationDetailTab = currentFrame?.tab ?? "events";
  const detailLoading = selectedKey ? Boolean(detailLoadingByKey[selectedKey]) : false;
  const detailError = selectedKey ? (detailErrorsByKey[selectedKey] ?? null) : null;
  const detailFileAvailable = selectedKey
    ? (fileAvailableByKey[selectedKey] ?? selected?.file_available ?? true)
    : true;
  const pollError = selectedKey ? (pollErrorsByKey[selectedKey] ?? null) : null;
  const [exportFormat, setExportFormat] = useState<ConversationExportFormat | null>(null);
  const [exportStatus, setExportStatus] = useState<string | null>(null);
  const [exportError, setExportError] = useState(false);
  const catalogGeneration = useRef(0);
  const detailGenerations = useRef(new Map<string, number>());
  const detailRequestGates = useRef(
    new Map<
      string,
      ReturnType<typeof createConversationRequestGate<ConversationDetailRequestIntent>>
    >(),
  );
  const mountedRef = useRef(true);
  const selectedKeyRef = useRef<string | null>(selectedKey);
  selectedKeyRef.current = selectedKey;
  const detailsRef = useRef<Record<string, ConversationDetailDto>>({});
  const observedDetailRevisions = useRef(new Map<string, string>());
  const timelineRef = useRef<HTMLDivElement>(null);
  const wasAtBottomRef = useRef(true);
  const pendingScrollRef = useRef(false);
  const savedTimelineScrollTopRef = useRef(0);
  const unseenCountRef = useRef(0);
  const jumpingRef = useRef(false);
  const jumpTokenRef = useRef(0);
  const jumpTimerRef = useRef(0);

  const getDetailRequestGate = useCallback((key: string) => {
    let gate = detailRequestGates.current.get(key);
    if (!gate) {
      gate = createConversationRequestGate<ConversationDetailRequestIntent>();
      detailRequestGates.current.set(key, gate);
    }
    return gate;
  }, []);

  const isDetailResponseCurrent = useCallback(
    (key: string, generation: number) =>
      isConversationResponseCurrent({
        mounted: mountedRef.current,
        generation,
        currentGeneration: detailGenerations.current.get(key) ?? 0,
      }),
    [],
  );

  const replaceDetail = useCallback(
    (key: string, result: ConversationDetailDto, followUpdates: boolean) => {
      if (selectedKeyRef.current === key) {
        if (followUpdates) {
          const follow = nextConversationFollowState({
            previousCount: detailsRef.current[key]?.events.length ?? 0,
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
      }
      detailsRef.current = { ...detailsRef.current, [key]: result };
      setDetails(detailsRef.current);
      observedDetailRevisions.current.set(key, result.revision);
      setFileAvailableByKey((current) => ({ ...current, [key]: result.session.file_available }));
      setDetailErrorsByKey((current) => {
        const next = { ...current };
        delete next[key];
        return next;
      });
      setPollErrorsByKey((current) => {
        const next = { ...current };
        delete next[key];
        return next;
      });
    },
    [],
  );

  const performDetailRequest = useCallback(
    async ({ session, key, generation, followUpdates }: ConversationDetailRequestIntent) => {
      try {
        const result = await invoke<ConversationDetailDto>("get_conversation_detail", {
          source: session.source,
          sessionId: session.session_id,
        });
        if (isDetailResponseCurrent(key, generation)) {
          replaceDetail(key, result, followUpdates);
        }
      } catch (error) {
        if (isDetailResponseCurrent(key, generation)) {
          setDetailErrorsByKey((current) => ({ ...current, [key]: humanStatus(error) }));
          onError?.(error);
        }
      } finally {
        if (isDetailResponseCurrent(key, generation)) {
          setDetailLoadingByKey((current) => ({ ...current, [key]: false }));
        }
      }
    },
    [isDetailResponseCurrent, onError, replaceDetail],
  );

  const drainDetailRequests = useCallback(
    async (initialIntent: ConversationDetailRequestIntent) => {
      let intent: ConversationDetailRequestIntent | null = initialIntent;
      while (intent) {
        await performDetailRequest(intent);
        intent = getDetailRequestGate(initialIntent.key).release();
      }
    },
    [getDetailRequestGate, performDetailRequest],
  );

  useEffect(() => {
    const requestGates = detailRequestGates.current;
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      jumpTokenRef.current += 1;
      jumpingRef.current = false;
      window.clearTimeout(jumpTimerRef.current);
      for (const gate of requestGates.values()) {
        gate.clearPending();
      }
    };
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => setSearch(searchInput.trim()), 300);
    return () => window.clearTimeout(timer);
  }, [searchInput]);

  useEffect(() => {
    if (!navigation.focus_relationship_id) return;
    const relationshipId = navigation.focus_relationship_id;
    const frame = window.requestAnimationFrame(() => {
      const target = [...document.querySelectorAll<HTMLElement>("[data-relationship-id]")].find(
        (element) => element.dataset.relationshipId === relationshipId,
      );
      target?.focus();
      setNavigation((current) =>
        transitionConversationNavigation(current, { type: "focus_restored" }),
      );
    });
    return () => window.cancelAnimationFrame(frame);
  }, [navigation.focus_relationship_id]);

  useEffect(() => {
    setPage(1);
  }, [filter, search]);

  useEffect(() => {
    const generation = ++catalogGeneration.current;
    setCatalogLoading(true);
    setCatalogError(null);
    invoke<ConversationPage>("get_conversation_sessions_page", {
      query: {
        search: search || null,
        page,
        page_size: PAGE_SIZE,
        sources: filter.sources,
        projects: filter.projects,
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
  }, [filter, revision, search, page, onError]);

  const fetchDetail = useCallback(
    (session: ConversationSessionRow, followUpdates = false) => {
      const shouldRequest = shouldRequestConversationDetail(session);
      const key = conversationKey(session);
      const gate = getDetailRequestGate(key);
      const acquired = !shouldRequest || gate.acquire();
      const generation = (detailGenerations.current.get(key) ?? 0) + 1;
      detailGenerations.current.set(key, generation);
      setFileAvailableByKey((current) => ({
        ...current,
        [key]: session.file_available,
      }));
      setDetailErrorsByKey((current) => {
        const next = { ...current };
        delete next[key];
        return next;
      });
      setPollErrorsByKey((current) => {
        const next = { ...current };
        delete next[key];
        return next;
      });

      if (!shouldRequest) {
        gate.clearPending();
        setDetailLoadingByKey((current) => ({ ...current, [key]: false }));
        return;
      }
      setDetailLoadingByKey((current) => ({ ...current, [key]: true }));
      const intent = { session, key, generation, followUpdates };
      if (acquired) {
        void drainDetailRequests(intent);
      } else {
        gate.queueLatest(intent);
      }
    },
    [drainDetailRequests, getDetailRequestGate],
  );

  const selectedSource = selected?.source ?? null;
  const selectedSessionId = selected?.session_id ?? null;

  useEffect(() => {
    if (!selectedSource || !selectedSessionId || !selectedKey) {
      return;
    }

    let cancelled = false;
    const poll = async () => {
      const gate = getDetailRequestGate(selectedKey);
      if (!gate.acquire()) {
        return;
      }
      const generation = detailGenerations.current.get(selectedKey) ?? 0;
      try {
        const state = await invoke<ConversationDetailStateDto>("get_conversation_detail_state", {
          source: selectedSource,
          sessionId: selectedSessionId,
          knownRevision: observedDetailRevisions.current.get(selectedKey) ?? "",
        });
        if (cancelled || !isDetailResponseCurrent(selectedKey, generation)) {
          return;
        }

        const revisionPollState = nextConversationRevisionPollState({
          revision: state.revision,
          changed: state.changed,
          fileAvailable: state.file_available,
        });
        observedDetailRevisions.current.set(selectedKey, revisionPollState.knownRevision);
        setFileAvailableByKey((current) => ({
          ...current,
          [selectedKey]: state.file_available,
        }));
        setPollErrorsByKey((current) => {
          const next = { ...current };
          delete next[selectedKey];
          return next;
        });
        if (revisionPollState.shouldReload) {
          const result = await invoke<ConversationDetailDto>("get_conversation_detail", {
            source: selectedSource,
            sessionId: selectedSessionId,
          });
          if (!cancelled && isDetailResponseCurrent(selectedKey, generation)) {
            replaceDetail(selectedKey, result, true);
          }
        }
      } catch (error) {
        if (!cancelled && isDetailResponseCurrent(selectedKey, generation)) {
          setPollErrorsByKey((current) => ({ ...current, [selectedKey]: humanStatus(error) }));
        }
      } finally {
        const pendingIntent = gate.release();
        if (pendingIntent) {
          void drainDetailRequests(pendingIntent);
        }
      }
    };

    const timer = window.setInterval(poll, 2_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [
    drainDetailRequests,
    getDetailRequestGate,
    isDetailResponseCurrent,
    replaceDetail,
    selectedKey,
    selectedSessionId,
    selectedSource,
  ]);

  const syncTimelineEdge = useCallback((timeline: HTMLElement) => {
    const nextAtTop = isNearConversationTop(timeline);
    const nextAtBottom = isNearConversationBottom(timeline);
    setAtTop(nextAtTop);
    setAtBottom(nextAtBottom);
    if (!jumpingRef.current) {
      wasAtBottomRef.current = nextAtBottom;
      savedTimelineScrollTopRef.current = timeline.scrollTop;
    }
    if (nextAtBottom) {
      unseenCountRef.current = 0;
      setUnseenCount(0);
    }
  }, []);

  useLayoutEffect(() => {
    if (!detail || detailTab !== "events") {
      return;
    }
    const timeline = timelineRef.current;
    if (!timeline) {
      setAtTop(true);
      setAtBottom(true);
      return;
    }

    const pinToFollowedEdge = () => {
      timeline.scrollTop = conversationTimelineScrollTarget({
        wasAtBottom: pendingScrollRef.current || wasAtBottomRef.current,
        savedScrollTop: savedTimelineScrollTopRef.current,
        scrollHeight: timeline.scrollHeight,
      });
      pendingScrollRef.current = false;
      syncTimelineEdge(timeline);
    };

    pinToFollowedEdge();
    const stack = timeline.firstElementChild;
    if (!(stack instanceof HTMLElement)) {
      return;
    }
    const observer = new ResizeObserver(() => {
      if (jumpingRef.current) {
        return;
      }
      if (wasAtBottomRef.current) {
        timeline.scrollTop = timeline.scrollHeight;
      }
      syncTimelineEdge(timeline);
    });
    observer.observe(stack);
    return () => observer.disconnect();
  }, [detail, detailTab, syncTimelineEdge]);

  function handleTimelineScroll(event: UIEvent<HTMLDivElement>) {
    syncTimelineEdge(event.currentTarget);
  }

  function jumpTimeline(edge: ConversationJumpEdge) {
    const timeline = timelineRef.current;
    const token = ++jumpTokenRef.current;
    window.clearTimeout(jumpTimerRef.current);

    if (edge === "top") {
      pendingScrollRef.current = false;
      wasAtBottomRef.current = false;
    } else {
      wasAtBottomRef.current = true;
      unseenCountRef.current = 0;
      setUnseenCount(0);
    }

    if (!timeline) {
      jumpingRef.current = false;
      pendingScrollRef.current = edge === "bottom";
      setAtTop(edge === "top");
      setAtBottom(edge === "bottom");
      return;
    }

    const top = conversationJumpScrollTop(edge, timeline.scrollHeight);
    const maxTop = Math.max(0, timeline.scrollHeight - timeline.clientHeight);
    const targetTop = edge === "top" ? 0 : maxTop;
    const behavior = conversationJumpBehavior(
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
    );

    if (behavior === "auto" || Math.abs(timeline.scrollTop - targetTop) <= 40) {
      jumpingRef.current = false;
      timeline.scrollTop = top;
      syncTimelineEdge(timeline);
      return;
    }

    jumpingRef.current = true;
    timeline.scrollTo({ top, behavior: "smooth" });

    const settle = () => {
      if (token !== jumpTokenRef.current) {
        return;
      }
      jumpingRef.current = false;
      if (edge === "bottom") {
        timeline.scrollTop = timeline.scrollHeight;
      }
      syncTimelineEdge(timeline);
    };

    const onScrollEnd = () => {
      timeline.removeEventListener("scrollend", onScrollEnd);
      settle();
    };
    timeline.addEventListener("scrollend", onScrollEnd);
    jumpTimerRef.current = window.setTimeout(() => {
      timeline.removeEventListener("scrollend", onScrollEnd);
      settle();
    }, 1200);
  }

  function handleDetailTabChange(nextTab: ConversationDetailTab) {
    if (detailTab === "events" && nextTab !== "events") {
      const timeline = timelineRef.current;
      if (timeline) {
        savedTimelineScrollTopRef.current = timeline.scrollTop;
        wasAtBottomRef.current = isNearConversationBottom(timeline);
      }
    }
    setDetailTab(nextTab);
  }

  const loadDetail = useCallback(
    (session: ConversationSessionRow) => {
      setNavigation((current) =>
        transitionConversationNavigation(current, { type: "open_root", session }),
      );
      setExportFormat(null);
      setExportStatus(null);
      setExportError(false);
      savedTimelineScrollTopRef.current = 0;
      wasAtBottomRef.current = true;
      pendingScrollRef.current = true;
      jumpTokenRef.current += 1;
      jumpingRef.current = false;
      window.clearTimeout(jumpTimerRef.current);
      unseenCountRef.current = 0;
      setUnseenCount(0);
      fetchDetail(session);
    },
    [fetchDetail],
  );

  useEffect(() => {
    if (!focus) {
      return;
    }
    const source = focus.source;
    const sessionId = focus.session_id;
    onFocusConsumed?.();
    invoke<ConversationDetailDto>("get_conversation_detail", {
      source,
      sessionId,
    })
      .then((detail) => {
        loadDetail(detail.session);
      })
      .catch((error) => {
        setSearchInput(sessionId);
        setSearch(sessionId);
        onError?.(error);
      });
  }, [focus, loadDetail, onError, onFocusConsumed]);

  function closeDetail() {
    setNavigation((current) => transitionConversationNavigation(current, { type: "close" }));
    detailGenerations.current.clear();
    observedDetailRevisions.current.clear();
    for (const gate of detailRequestGates.current.values()) {
      gate.clearPending();
    }
    detailRequestGates.current.clear();
    detailsRef.current = {};
    setDetails({});
    setDetailLoadingByKey({});
    setDetailErrorsByKey({});
    setFileAvailableByKey({});
    setPollErrorsByKey({});
    savedTimelineScrollTopRef.current = 0;
    unseenCountRef.current = 0;
    setUnseenCount(0);
    pendingScrollRef.current = false;
    jumpTokenRef.current += 1;
    jumpingRef.current = false;
    window.clearTimeout(jumpTimerRef.current);
    setExportFormat(null);
    setExportStatus(null);
    setExportError(false);
  }

  function backToParent() {
    const scrollTop = navigation.frames.at(-2)?.scroll_top ?? 0;
    setNavigation((current) => transitionConversationNavigation(current, { type: "back" }));
    savedTimelineScrollTopRef.current = scrollTop;
    wasAtBottomRef.current = false;
    pendingScrollRef.current = false;
    jumpTokenRef.current += 1;
    jumpingRef.current = false;
    window.clearTimeout(jumpTimerRef.current);
    unseenCountRef.current = 0;
    setUnseenCount(0);
  }

  function setDetailTab(tab: ConversationDetailTab) {
    setNavigation((current) => transitionConversationNavigation(current, { type: "set_tab", tab }));
  }

  function toggleChild(link: ConversationAgentLink) {
    const isExpanded = currentFrame?.expanded_relationship_ids.includes(link.relationship_id);
    setNavigation((current) =>
      transitionConversationNavigation(current, {
        type: "toggle_child",
        relationship_id: link.relationship_id,
      }),
    );
    if (!isExpanded && link.session && !details[conversationKey(link.session)]) {
      fetchDetail(link.session);
    }
  }

  function openChild(link: ConversationAgentLink) {
    if (!link.session) return;
    const parentScrollTop = timelineRef.current?.scrollTop ?? 0;
    setNavigation((current) =>
      transitionConversationNavigation(current, {
        type: "enter_child",
        session: link.session!,
        relationship_id: link.relationship_id,
        parent_scroll_top: parentScrollTop,
      }),
    );
    savedTimelineScrollTopRef.current = 0;
    wasAtBottomRef.current = true;
    pendingScrollRef.current = true;
    jumpTokenRef.current += 1;
    jumpingRef.current = false;
    window.clearTimeout(jumpTimerRef.current);
    unseenCountRef.current = 0;
    setUnseenCount(0);
    fetchDetail(link.session);
  }

  async function exportConversation(format: ConversationExportFormat) {
    if (!selected) {
      return;
    }
    setExportFormat(format);
    setExportStatus(null);
    setExportError(false);
    try {
      const saved = await invoke<boolean>("export_conversation", {
        source: selected.source,
        sessionId: selected.session_id,
        format,
      });
      setExportStatus(saved ? "已导出" : "已取消");
    } catch (error) {
      setExportError(true);
      setExportStatus(humanStatus(error));
    } finally {
      setExportFormat(null);
    }
  }

  function updateEventContent(
    source: string,
    sessionId: string,
    content: ConversationEventContentDto,
  ) {
    const key = conversationKey({ source, session_id: sessionId });
    setDetails((current) => {
      const currentDetail = current[key];
      if (!currentDetail) return current;
      const next = {
        ...current,
        [key]: {
          ...currentDetail,
          events: currentDetail.events.map((event) =>
            event.event_id === content.event_id
              ? {
                  ...event,
                  text: content.text,
                  details: content.details,
                  content_status: "complete" as const,
                }
              : event,
          ),
        },
      };
      detailsRef.current = next;
      return next;
    });
  }

  if (selected) {
    const session = detail?.session ?? selected;
    return (
      <div className="conversation-detail-view">
        <ConversationDetailHead
          session={session}
          fileAvailable={detailFileAvailable}
          breadcrumb={
            navigation.frames.length > 1
              ? navigation.frames.map((frame) => frame.session.title).join(" / ")
              : null
          }
          parentAvailable={navigation.frames.length > 1}
          exportFormat={exportFormat}
          exportStatus={exportStatus}
          exportError={exportError}
          exportDisabled={!detailFileAvailable || !detail}
          onBack={navigation.frames.length > 1 ? backToParent : closeDetail}
          onExport={(format) => void exportConversation(format)}
        />

        <section className="conversation-detail-body" aria-busy={detailLoading}>
          <div className="conversation-detail-tabs">
            <Segmented
              value={detailTab}
              options={detail?.cursor_behavior ? [...DETAIL_TABS, BEHAVIOR_TAB] : DETAIL_TABS}
              disabled={detailLoading || Boolean(detailError)}
              ariaLabel="对话详情视图"
              onChange={handleDetailTabChange}
            />
            {detail ? (
              <span className="muted">
                {detailTab === "events"
                  ? `${detail.events.length} 条事件`
                  : detailTab === "behavior"
                    ? "Cursor 行为聚合"
                    : `${detail.usage_records.length} 条记录`}
              </span>
            ) : null}
          </div>
          {!detailFileAvailable ? (
            <div className="conversation-detail-notice" role="status">
              <Icon name="alertTriangle" size={16} />
              <div>
                <strong>
                  {session.source === "cursor_agent"
                    ? "缺少 Cursor transcript，对话正文不可读取"
                    : "原文件已删除，详情不可继续读取"}
                </strong>
                <span>
                  {session.source === "cursor_agent"
                    ? "仍可查看确定性关联的用量、行为统计与会话状态。"
                    : detail
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
              <Button onClick={() => fetchDetail(selected)}>重新读取</Button>
            </div>
          ) : detail ? (
            detailTab === "usage" ? (
              <UsageRecordsTable records={detail.usage_records} />
            ) : detailTab === "behavior" && detail.cursor_behavior ? (
              <CursorSessionDetail detail={detail.cursor_behavior} embedded />
            ) : (
              <div className="conversation-events-view">
                {detail.agent_relations.capability_status !== "complete" ? (
                  <div
                    className={`conversation-agent-capability status-${detail.agent_relations.capability_status}`}
                    role="status"
                  >
                    <Icon name="alertTriangle" size={14} />
                    <span>
                      {AGENT_CAPABILITY_MESSAGES[detail.agent_relations.capability_status]}
                    </span>
                  </div>
                ) : null}
                <EventTimeline
                  key={`${session.source}:${session.session_id}`}
                  events={detail.events}
                  source={session.source}
                  sessionId={session.session_id}
                  agentLinks={detail.agent_relations.children}
                  expandedRelationshipIds={currentFrame?.expanded_relationship_ids ?? []}
                  childDetails={details}
                  childLoading={detailLoadingByKey}
                  onToggleChild={toggleChild}
                  onOpenChild={openChild}
                  onEventContentLoaded={updateEventContent}
                  timelineRef={timelineRef}
                  onScroll={handleTimelineScroll}
                />
                <ConversationJumpBar
                  atTop={atTop}
                  atBottom={atBottom}
                  unseenCount={unseenCount}
                  onJumpTop={() => jumpTimeline("top")}
                  onJumpBottom={() => jumpTimeline("bottom")}
                />
              </div>
            )
          ) : null}
        </section>
      </div>
    );
  }

  const { rows, total } = pageData;
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const maxTotal = Math.max(1, ...rows.map((row) => row.total_tokens));

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
                <th>token</th>
                <th>费用</th>
                <th>起止</th>
                <th>能力</th>
                <th>状态</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <ConversationCatalogRow
                  key={`${row.source}-${row.session_id}`}
                  row={row}
                  maxTotal={maxTotal}
                  onOpen={loadDetail}
                />
              ))}
              {rows.length === 0 ? (
                <tr>
                  <td colSpan={9} className="analytics-empty">
                    {catalogLoading ? (
                      <EmptyState icon="chat" title="正在加载对话目录…" />
                    ) : (
                      <EmptyState
                        icon="chat"
                        title="当前条件下暂无对话记录"
                        hint="请确认本机已有会话文件，并执行一次刷新。Cursor 与其它来源共用此目录。"
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
