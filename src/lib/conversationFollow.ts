export type ConversationScrollMetrics = {
  scrollTop: number;
  clientHeight: number;
  scrollHeight: number;
};

export type ConversationRequestGate = {
  acquire: () => boolean;
  release: () => void;
};

export function createConversationRequestGate(): ConversationRequestGate {
  let busy = false;
  return {
    acquire() {
      if (busy) {
        return false;
      }
      busy = true;
      return true;
    },
    release() {
      busy = false;
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
