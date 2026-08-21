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
  conversationTimelineScrollTarget,
  createConversationRequestGate,
  isConversationResponseCurrent,
  isNearConversationBottom,
  nextConversationFollowState,
  nextConversationRevisionPollState,
} from "../lib/conversationFollow";
import {
  applicationLabel,
  formatClock,
  formatTokens,
  humanStatus,
  projectLabel,
  relativeTime,
} from "../lib/format";
import { ConversationMarkdown } from "../lib/conversationMarkdown";
import type {
  ConversationAttachment,
  ConversationAttachmentContentDto,
  ConversationDetailDto,
  ConversationDetailStateDto,
  ConversationEvent,
  ConversationEventActor,
  ConversationEventCapabilityStatus,
  ConversationEventContentDto,
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

type ConversationDetailRequestIntent = {
  session: ConversationSessionRow;
  generation: number;
};

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
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
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

function EventTimeline({
  events,
  source,
  sessionId,
  onEventContentLoaded,
  timelineRef,
  onScroll,
}: {
  events: ConversationEvent[];
  source: string;
  sessionId: string;
  onEventContentLoaded: (content: ConversationEventContentDto) => void;
  timelineRef: RefObject<HTMLDivElement | null>;
  onScroll: (event: UIEvent<HTMLDivElement>) => void;
}) {
  const { states: eventLoads, errors: eventErrors, run: runEventLoad } =
    useKeyedAsyncLoad<number>();
  const { states: thumbnailLoads, errors: thumbnailErrors, run: runThumbnailLoad } =
    useKeyedAsyncLoad<string>();
  const { states: imageLoads, errors: imageErrors, run: runImageLoad } =
    useKeyedAsyncLoad<string>();
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

  async function loadFullEvent(sequence: number) {
    await runEventLoad(
      sequence,
      () =>
        invoke<ConversationEventContentDto>("get_conversation_event_content", {
          source,
          sessionId,
          sequence,
        }),
      onEventContentLoaded,
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

  if (events.length === 0) {
    return (
      <EmptyState
        icon="chat"
        title="这条会话暂无事件"
        hint="当前会话没有可展示的语义事件。"
      />
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
                {event.text ? (
                  usesMarkdown ? (
                    <ConversationMarkdown markdown={event.text} />
                  ) : (
                    <pre className="conversation-event-text conversation-event-command">{event.text}</pre>
                  )
                ) : null}
                {isDeferred ? (
                  <div className="conversation-deferred" aria-live="polite">
                    <span>仅显示前部内容</span>
                    <Button
                      variant="text"
                      onClick={() => void loadFullEvent(event.sequence)}
                      disabled={eventLoads[event.sequence] === "loading"}
                    >
                      {eventLoads[event.sequence] === "loading" ? <Spinner size={12} /> : null}
                      加载全文
                    </Button>
                    {eventLoads[event.sequence] === "error" ? (
                      <span className="conversation-inline-error" role="alert">
                        {eventErrors[event.sequence]}
                      </span>
                    ) : null}
                  </div>
                ) : null}
                {showDetails ? (
                  <details className="conversation-event-details">
                    <summary>{event.kind === "unadapted" ? "查看原始事件" : "查看详细数据"}</summary>
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
                        cachedThumbnail?.signature === signature ? cachedThumbnail.dataUrl : null;
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
                                  thumbnailState === "error"
                                    ? undefined
                                    : "正在生成缩略图"
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
          );
        })}
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
  const [exportFormat, setExportFormat] = useState<"markdown" | "json" | null>(null);
  const [exportStatus, setExportStatus] = useState<string | null>(null);
  const [exportError, setExportError] = useState(false);
  const catalogGeneration = useRef(0);
  const detailGeneration = useRef(0);
  const detailRequestGate = useRef(
    createConversationRequestGate<ConversationDetailRequestIntent>(),
  );
  const mountedRef = useRef(true);
  const selectedRef = useRef<ConversationSessionRow | null>(null);
  const detailRef = useRef<ConversationDetailDto | null>(null);
  const observedDetailRevisionRef = useRef("");
  const timelineRef = useRef<HTMLDivElement>(null);
  const wasAtBottomRef = useRef(true);
  const pendingScrollRef = useRef(false);
  const savedTimelineScrollTopRef = useRef(0);
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
    observedDetailRevisionRef.current = result.revision;
    setDetail(result);
    setDetailFileAvailable(result.session.file_available);
    setDetailError(null);
    setPollError(null);
  }, []);

  const performDetailRequest = useCallback(
    async ({ session, generation }: ConversationDetailRequestIntent) => {
      try {
        const result = await invoke<ConversationDetailDto>("get_conversation_detail", {
          source: session.source,
          sessionId: session.session_id,
        });
        if (isDetailResponseCurrent(generation)) {
          replaceDetail(result, false);
        }
      } catch (error) {
        if (isDetailResponseCurrent(generation)) {
          setDetailError(humanStatus(error));
          onError?.(error);
        }
      } finally {
        if (isDetailResponseCurrent(generation)) {
          setDetailLoading(false);
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
        intent = detailRequestGate.current.release();
      }
    },
    [performDetailRequest],
  );

  useEffect(() => {
    const requestGate = detailRequestGate.current;
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      detailGeneration.current += 1;
      selectedRef.current = null;
      requestGate.clearPending();
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
      const acquired = !needsRequest || detailRequestGate.current.acquire();
      const isSameSelection =
        selectedRef.current?.source === session.source &&
        selectedRef.current.session_id === session.session_id;
      if (!acquired && isSameSelection) {
        return;
      }

      const generation = ++detailGeneration.current;
      selectedRef.current = session;
      setSelected(session);
      setDetailTab("events");
      detailRef.current = null;
      observedDetailRevisionRef.current = "";
      savedTimelineScrollTopRef.current = 0;
      setDetail(null);
      setDetailError(null);
      setPollError(null);
      setDetailFileAvailable(session.file_available);
      setExportFormat(null);
      setExportStatus(null);
      setExportError(false);
      unseenCountRef.current = 0;
      setUnseenCount(0);
      wasAtBottomRef.current = true;
      pendingScrollRef.current = false;

      if (!needsRequest) {
        detailRequestGate.current.clearPending();
        setDetailLoading(false);
        return;
      }
      setDetailLoading(true);
      const intent = { session, generation };
      if (acquired) {
        void drainDetailRequests(intent);
      } else {
        detailRequestGate.current.queueLatest(intent);
      }
    },
    [drainDetailRequests],
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
          knownRevision: observedDetailRevisionRef.current,
        });
        if (cancelled || !isDetailResponseCurrent(generation)) {
          return;
        }

        const revisionPollState = nextConversationRevisionPollState({
          revision: state.revision,
          changed: state.changed,
          fileAvailable: state.file_available,
        });
        observedDetailRevisionRef.current = revisionPollState.knownRevision;
        setDetailFileAvailable(state.file_available);
        setPollError(null);
        if (revisionPollState.shouldReload) {
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
        const pendingIntent = detailRequestGate.current.release();
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
    isDetailResponseCurrent,
    replaceDetail,
    selectedSessionId,
    selectedSource,
  ]);

  useLayoutEffect(() => {
    if (!detail || detailTab !== "events") {
      return;
    }
    const frame = window.requestAnimationFrame(() => {
      const timeline = timelineRef.current;
      if (timeline) {
        timeline.scrollTop = conversationTimelineScrollTarget({
          wasAtBottom: pendingScrollRef.current || wasAtBottomRef.current,
          savedScrollTop: savedTimelineScrollTopRef.current,
          scrollHeight: timeline.scrollHeight,
        });
        savedTimelineScrollTopRef.current = timeline.scrollTop;
        wasAtBottomRef.current = isNearConversationBottom(timeline);
      }
      pendingScrollRef.current = false;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [detail, detailTab]);

  function handleTimelineScroll(event: UIEvent<HTMLDivElement>) {
    savedTimelineScrollTopRef.current = event.currentTarget.scrollTop;
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
      savedTimelineScrollTopRef.current = timeline.scrollTop;
    } else {
      pendingScrollRef.current = true;
    }
    wasAtBottomRef.current = true;
    unseenCountRef.current = 0;
    setUnseenCount(0);
  }

  function handleDetailTabChange(nextTab: DetailTab) {
    if (detailTab === "events" && nextTab !== "events") {
      const timeline = timelineRef.current;
      if (timeline) {
        savedTimelineScrollTopRef.current = timeline.scrollTop;
        wasAtBottomRef.current = isNearConversationBottom(timeline);
      }
    }
    setDetailTab(nextTab);
  }

  function closeDetail() {
    detailGeneration.current += 1;
    selectedRef.current = null;
    detailRequestGate.current.clearPending();
    setSelected(null);
    setDetailTab("events");
    detailRef.current = null;
    observedDetailRevisionRef.current = "";
    setDetail(null);
    setDetailError(null);
    setPollError(null);
    setDetailFileAvailable(true);
    savedTimelineScrollTopRef.current = 0;
    unseenCountRef.current = 0;
    setUnseenCount(0);
    pendingScrollRef.current = false;
    setDetailLoading(false);
    setExportFormat(null);
    setExportStatus(null);
    setExportError(false);
  }

  async function exportConversation(format: "markdown" | "json") {
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

  function updateEventContent(content: ConversationEventContentDto) {
    setDetail((current) =>
      current
        ? {
            ...current,
            events: current.events.map((event) =>
              event.sequence === content.sequence
                ? {
                    ...event,
                    text: content.text,
                    details: content.details,
                    content_status: "complete",
                  }
                : event,
            ),
          }
        : current,
    );
  }

  if (selected) {
    const session = detail?.session ?? selected;
    return (
      <div className="conversation-detail-view">
        <section className="panel conversation-detail-head">
          <div className="conversation-detail-actions">
            <div className="conversation-detail-navigation">
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
            <div className="conversation-export-wrap">
              <div className="conversation-export-actions" aria-label="导出会话">
                <Icon name="download" size={14} />
                <Button
                  variant="text"
                  onClick={() => void exportConversation("markdown")}
                  disabled={exportFormat !== null || !detailFileAvailable || !detail}
                >
                  {exportFormat === "markdown" ? <Spinner size={12} /> : null}
                  Markdown
                </Button>
                <Button
                  variant="text"
                  onClick={() => void exportConversation("json")}
                  disabled={exportFormat !== null || !detailFileAvailable || !detail}
                >
                  {exportFormat === "json" ? <Spinner size={12} /> : null}
                  JSON
                </Button>
              </div>
              <span
                className={exportError ? "conversation-export-status is-error" : "conversation-export-status"}
                role={exportError ? "alert" : "status"}
                aria-live={exportError ? "assertive" : "polite"}
              >
                {exportStatus}
              </span>
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
              onChange={handleDetailTabChange}
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
                  key={`${session.source}:${session.session_id}`}
                  events={detail.events}
                  source={session.source}
                  sessionId={session.session_id}
                  onEventContentLoaded={updateEventContent}
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
