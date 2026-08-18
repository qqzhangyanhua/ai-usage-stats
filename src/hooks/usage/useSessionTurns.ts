import { invoke } from "@tauri-apps/api/core";
import { useCallback, useRef, useState } from "react";
import type { Filter, TurnRow } from "../../types";
import type { SelectedSession } from "./constants";

export function useSessionTurns(filter: Filter, reportError: (error: unknown) => void) {
  const turnsGeneration = useRef(0);
  const [turns, setTurns] = useState<TurnRow[]>([]);
  const [turnsLoading, setTurnsLoading] = useState(false);
  const [selectedSession, setSelectedSession] = useState<SelectedSession | null>(null);

  const loadSessionTurns = useCallback(
    async (session: SelectedSession, nextFilter = filter) => {
      const generation = ++turnsGeneration.current;
      setTurnsLoading(true);
      try {
        const rows = await invoke<TurnRow[]>("get_session_turns", {
          sessionId: session.id,
          source: session.source,
          filter: nextFilter,
        });
        if (generation === turnsGeneration.current) {
          setTurns(rows);
        }
      } finally {
        if (generation === turnsGeneration.current) {
          setTurnsLoading(false);
        }
      }
    },
    [filter],
  );

  const selectSession = useCallback(
    (session: SelectedSession) => {
      setSelectedSession(session);
      loadSessionTurns(session).catch(reportError);
    },
    [loadSessionTurns, reportError],
  );

  return {
    turns,
    turnsLoading,
    selectedSession,
    loadSessionTurns,
    selectSession,
  };
}
