import { describe, expect, it, beforeEach } from "vitest";
import type { CursorSessionDetailDto } from "../types";
import {
  clearCursorSessionDetailCache,
  getCachedCursorSessionDetail,
  setCachedCursorSessionDetail,
} from "./cursorSessionDetailCache";

function sampleDetail(sourceFile: string): CursorSessionDetailDto {
  return {
    session: {
      session_id: "abc",
      project: "/proj",
      turn_count: 1,
      success_count: 1,
      error_count: 0,
      aborted_count: 0,
      user_prompt_count: 1,
      subagent_count: 0,
      models: [],
      sources: [],
      tool_call_count: 0,
      first_seen_at: null,
      last_seen_at: null,
      files_touched: 0,
      source_file: sourceFile,
    },
    tools: [],
    read_paths: [],
    write_paths: [],
    hash_files: [],
    transcript_missing: false,
  };
}

describe("cursorSessionDetailCache", () => {
  beforeEach(() => {
    clearCursorSessionDetailCache();
  });

  it("stores and retrieves detail by source file", () => {
    const detail = sampleDetail("/tmp/transcript.jsonl");
    setCachedCursorSessionDetail(detail.session.source_file, detail);
    expect(getCachedCursorSessionDetail("/tmp/transcript.jsonl")).toEqual(detail);
  });

  it("clears all entries", () => {
    setCachedCursorSessionDetail("/a.jsonl", sampleDetail("/a.jsonl"));
    clearCursorSessionDetailCache();
    expect(getCachedCursorSessionDetail("/a.jsonl")).toBeUndefined();
  });
});
