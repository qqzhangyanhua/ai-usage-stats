export type ConversationScrollMetrics = {
  scrollTop: number;
  clientHeight: number;
  scrollHeight: number;
};

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
