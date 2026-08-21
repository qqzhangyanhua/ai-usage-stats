import type { ConversationSessionRow } from "../types";

export type ConversationDetailTab = "events" | "usage";

export type ConversationNavigationFrame = {
  session: ConversationSessionRow;
  tab: ConversationDetailTab;
  expanded_relationship_ids: string[];
  scroll_top: number;
  entered_from_relationship_id: string | null;
};

export type ConversationNavigationState = {
  frames: ConversationNavigationFrame[];
  focus_relationship_id: string | null;
};

export type ConversationNavigationAction =
  | { type: "open_root"; session: ConversationSessionRow }
  | {
      type: "enter_child";
      session: ConversationSessionRow;
      relationship_id: string;
      parent_scroll_top: number;
    }
  | { type: "set_tab"; tab: ConversationDetailTab }
  | { type: "toggle_child"; relationship_id: string }
  | { type: "back" }
  | { type: "close" }
  | { type: "focus_restored" };

export const initialConversationNavigationState: ConversationNavigationState = {
  frames: [],
  focus_relationship_id: null,
};

function newFrame(
  session: ConversationSessionRow,
  enteredFromRelationshipId: string | null,
): ConversationNavigationFrame {
  return {
    session,
    tab: "events",
    expanded_relationship_ids: [],
    scroll_top: 0,
    entered_from_relationship_id: enteredFromRelationshipId,
  };
}

function sameSession(left: ConversationSessionRow, right: ConversationSessionRow): boolean {
  return left.source === right.source && left.session_id === right.session_id;
}

function updateCurrentFrame(
  state: ConversationNavigationState,
  update: (frame: ConversationNavigationFrame) => ConversationNavigationFrame,
): ConversationNavigationState {
  if (state.frames.length === 0) return state;
  const frames = state.frames.slice();
  frames[frames.length - 1] = update(frames[frames.length - 1]);
  return { ...state, frames };
}

export function transitionConversationNavigation(
  state: ConversationNavigationState,
  action: ConversationNavigationAction,
): ConversationNavigationState {
  switch (action.type) {
    case "open_root":
      return { frames: [newFrame(action.session, null)], focus_relationship_id: null };
    case "enter_child": {
      if (
        state.frames.length === 0 ||
        state.frames.some((frame) => sameSession(frame.session, action.session))
      ) {
        return state;
      }
      const parentState = updateCurrentFrame(state, (frame) => ({
        ...frame,
        scroll_top: action.parent_scroll_top,
      }));
      return {
        frames: [
          ...parentState.frames,
          newFrame(action.session, action.relationship_id),
        ],
        focus_relationship_id: null,
      };
    }
    case "set_tab":
      return updateCurrentFrame(state, (frame) => ({ ...frame, tab: action.tab }));
    case "toggle_child":
      return updateCurrentFrame(state, (frame) => {
        const expanded = new Set(frame.expanded_relationship_ids);
        if (expanded.has(action.relationship_id)) expanded.delete(action.relationship_id);
        else expanded.add(action.relationship_id);
        return { ...frame, expanded_relationship_ids: [...expanded].sort() };
      });
    case "back": {
      if (state.frames.length <= 1) return initialConversationNavigationState;
      const leaving = state.frames[state.frames.length - 1];
      return {
        frames: state.frames.slice(0, -1),
        focus_relationship_id: leaving.entered_from_relationship_id,
      };
    }
    case "close":
      return initialConversationNavigationState;
    case "focus_restored":
      return state.focus_relationship_id === null
        ? state
        : { ...state, focus_relationship_id: null };
  }
}

export function currentConversationFrame(
  state: ConversationNavigationState,
): ConversationNavigationFrame | null {
  return state.frames.at(-1) ?? null;
}
