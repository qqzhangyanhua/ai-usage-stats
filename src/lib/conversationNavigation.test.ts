import { describe, expect, it } from "vitest";
import type { ConversationSessionRow } from "../types";
import {
  currentConversationFrame,
  initialConversationNavigationState,
  shouldRequestConversationDetail,
  transitionConversationNavigation,
} from "./conversationNavigation";

function session(session_id: string): ConversationSessionRow {
  return {
    source: "codex",
    session_id,
    title: session_id,
    project: "/workspace/project",
    model: "gpt-test",
    started_at: "2026-08-21T00:00:00Z",
    ended_at: "2026-08-21T00:01:00Z",
    source_file: `${session_id}.jsonl`,
    source_files: [`${session_id}.jsonl`],
    capabilities: ["messages", "events", "usage"],
    support_status: "experimental",
    file_available: true,
    total_tokens: 0,
    cost: null,
    unpriced: false,
  };
}

describe("conversation navigation", () => {
  it("requests status-only detail for unavailable Cursor sessions", () => {
    const cursor = { ...session("cursor"), source: "cursor_agent", file_available: false };
    const missingCodex = { ...session("codex"), file_available: false };

    expect(shouldRequestConversationDetail(cursor)).toBe(true);
    expect(shouldRequestConversationDetail(missingCodex)).toBe(false);
    expect(shouldRequestConversationDetail(session("available"))).toBe(true);
  });

  it("restores the parent tab, expansion, scroll, and relationship focus after child detail", () => {
    let state = transitionConversationNavigation(initialConversationNavigationState, {
      type: "open_root",
      session: session("parent"),
    });
    state = transitionConversationNavigation(state, { type: "set_tab", tab: "usage" });
    state = transitionConversationNavigation(state, {
      type: "toggle_child",
      relationship_id: "rel-child",
    });
    state = transitionConversationNavigation(state, {
      type: "enter_child",
      session: session("child"),
      relationship_id: "rel-child",
      parent_scroll_top: 480,
    });

    expect(currentConversationFrame(state)).toMatchObject({
      session: { session_id: "child" },
      tab: "events",
      entered_from_relationship_id: "rel-child",
    });

    state = transitionConversationNavigation(state, { type: "back" });
    expect(currentConversationFrame(state)).toMatchObject({
      session: { session_id: "parent" },
      tab: "usage",
      expanded_relationship_ids: ["rel-child"],
      scroll_top: 480,
    });
    expect(state.focus_relationship_id).toBe("rel-child");
  });

  it("pops one level and rejects a cyclic session entry", () => {
    let state = transitionConversationNavigation(initialConversationNavigationState, {
      type: "open_root",
      session: session("parent"),
    });
    state = transitionConversationNavigation(state, {
      type: "enter_child",
      session: session("child"),
      relationship_id: "parent-child",
      parent_scroll_top: 0,
    });
    state = transitionConversationNavigation(state, {
      type: "enter_child",
      session: session("grandchild"),
      relationship_id: "child-grandchild",
      parent_scroll_top: 120,
    });
    const unchanged = transitionConversationNavigation(state, {
      type: "enter_child",
      session: session("parent"),
      relationship_id: "cycle",
      parent_scroll_top: 0,
    });

    expect(unchanged).toBe(state);
    state = transitionConversationNavigation(state, { type: "back" });
    expect(currentConversationFrame(state)?.session.session_id).toBe("child");
    expect(state.frames).toHaveLength(2);
  });
});
