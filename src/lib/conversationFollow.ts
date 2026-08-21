export type ConversationScrollMetrics = {
  scrollTop: number;
  clientHeight: number;
  scrollHeight: number;
};

export type ConversationRequestGate<T> = {
  acquire: () => boolean;
  queueLatest: (intent: T) => void;
  clearPending: () => void;
  release: () => T | null;
};

export function createConversationRequestGate<T = never>(): ConversationRequestGate<T> {
  let busy = false;
  let pending: T | null = null;
  return {
    acquire() {
      if (busy) {
        return false;
      }
      busy = true;
      return true;
    },
    queueLatest(intent) {
      pending = intent;
    },
    clearPending() {
      pending = null;
    },
    release() {
      if (pending !== null) {
        const intent = pending;
        pending = null;
        return intent;
      }
      busy = false;
      return null;
    },
  };
}

export function isConversationResponseCurrent({
  mounted,
  generation,
  currentGeneration,
}: {
  mounted: boolean;
  generation: number;
  currentGeneration: number;
}): boolean {
  return mounted && generation === currentGeneration;
}

export function isNearConversationBottom(
  { scrollTop, clientHeight, scrollHeight }: ConversationScrollMetrics,
  threshold = 40,
): boolean {
  return scrollHeight - scrollTop - clientHeight <= Math.max(0, threshold);
}

export function conversationTimelineScrollTarget({
  wasAtBottom,
  savedScrollTop,
  scrollHeight,
}: {
  wasAtBottom: boolean;
  savedScrollTop: number;
  scrollHeight: number;
}): number {
  return wasAtBottom ? Math.max(0, scrollHeight) : Math.max(0, savedScrollTop);
}

export type ConversationFollowInput = {
  previousCount: number;
  nextCount: number;
  wasAtBottom: boolean;
  unseenCount: number;
};

export type ConversationFollowState = {
  shouldScroll: boolean;
  unseenCount: number;
};

export function nextConversationFollowState({
  previousCount,
  nextCount,
  wasAtBottom,
  unseenCount,
}: ConversationFollowInput): ConversationFollowState {
  const normalizedPrevious = Math.max(0, previousCount);
  const normalizedNext = Math.max(0, nextCount);
  const normalizedUnseen = Math.min(Math.max(0, unseenCount), normalizedNext);
  const addedCount = Math.max(0, normalizedNext - normalizedPrevious);

  if (addedCount === 0) {
    return { shouldScroll: false, unseenCount: normalizedUnseen };
  }
  if (wasAtBottom) {
    return { shouldScroll: true, unseenCount: 0 };
  }
  return {
    shouldScroll: false,
    unseenCount: Math.min(normalizedNext, normalizedUnseen + addedCount),
  };
}
