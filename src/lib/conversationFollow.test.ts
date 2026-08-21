import { describe, expect, it } from "vitest";
import {
  createConversationRequestGate,
  isConversationResponseCurrent,
  isNearConversationBottom,
  nextConversationFollowState,
} from "./conversationFollow";

describe("createConversationRequestGate", () => {
  it("serializes requests until the active request releases the gate", () => {
    const gate = createConversationRequestGate();

    expect(gate.acquire()).toBe(true);
    expect(gate.acquire()).toBe(false);

    gate.release();

    expect(gate.acquire()).toBe(true);
  });
});

describe("isConversationResponseCurrent", () => {
  it("rejects responses after unmount even when the generation matches", () => {
    expect(
      isConversationResponseCurrent({ mounted: false, generation: 3, currentGeneration: 3 }),
    ).toBe(false);
  });

  it("rejects stale generations and accepts a mounted current response", () => {
    expect(
      isConversationResponseCurrent({ mounted: true, generation: 2, currentGeneration: 3 }),
    ).toBe(false);
    expect(
      isConversationResponseCurrent({ mounted: true, generation: 3, currentGeneration: 3 }),
    ).toBe(true);
  });
});

describe("isNearConversationBottom", () => {
  it("treats a viewport within the threshold as being at the bottom", () => {
    expect(
      isNearConversationBottom({ scrollTop: 559, clientHeight: 400, scrollHeight: 1000 }),
    ).toBe(false);
    expect(
      isNearConversationBottom({ scrollTop: 560, clientHeight: 400, scrollHeight: 1000 }),
    ).toBe(true);
  });

  it("supports a custom threshold and short content", () => {
    expect(
      isNearConversationBottom({ scrollTop: 481, clientHeight: 500, scrollHeight: 1000 }, 20),
    ).toBe(true);
    expect(isNearConversationBottom({ scrollTop: 0, clientHeight: 500, scrollHeight: 300 })).toBe(
      true,
    );
  });
});

describe("nextConversationFollowState", () => {
  it("follows new events when the reader was already at the bottom", () => {
    expect(
      nextConversationFollowState({
        previousCount: 4,
        nextCount: 7,
        wasAtBottom: true,
        unseenCount: 2,
      }),
    ).toEqual({ shouldScroll: true, unseenCount: 0 });
  });

  it("keeps the scroll position and accumulates new events away from the bottom", () => {
    expect(
      nextConversationFollowState({
        previousCount: 4,
        nextCount: 7,
        wasAtBottom: false,
        unseenCount: 2,
      }),
    ).toEqual({ shouldScroll: false, unseenCount: 5 });
  });

  it("does not create false additions when the event count shrinks or resets", () => {
    expect(
      nextConversationFollowState({
        previousCount: 7,
        nextCount: 4,
        wasAtBottom: false,
        unseenCount: 2,
      }),
    ).toEqual({ shouldScroll: false, unseenCount: 2 });
    expect(
      nextConversationFollowState({
        previousCount: 4,
        nextCount: 0,
        wasAtBottom: false,
        unseenCount: 2,
      }),
    ).toEqual({ shouldScroll: false, unseenCount: 0 });
  });

  it("clamps invalid counters without reporting additions", () => {
    expect(
      nextConversationFollowState({
        previousCount: -2,
        nextCount: -1,
        wasAtBottom: false,
        unseenCount: -3,
      }),
    ).toEqual({ shouldScroll: false, unseenCount: 0 });
  });
});
