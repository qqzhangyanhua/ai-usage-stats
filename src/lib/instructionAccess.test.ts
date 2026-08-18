import { describe, expect, it } from "vitest";
import type { GlobalInstructionFile } from "../types";
import { canEditInstruction, canOpenInstruction, showsLoadStatus } from "./instructionAccess";

function file(overrides: Partial<GlobalInstructionFile> = {}): GlobalInstructionFile {
  return {
    kind: "file",
    display_path: "~/.claude/CLAUDE.md",
    abs_path: "/tmp/.claude/CLAUDE.md",
    byte_size: 4,
    modified_at: null,
    load_status: "loaded",
    evidence: "verified",
    content: "ok\n",
    error: null,
    note: null,
    action: null,
    ...overrides,
  };
}

describe("canOpenInstruction", () => {
  it("allows disk-backed entries including directories", () => {
    expect(canOpenInstruction(file())).toBe(true);
    expect(
      canOpenInstruction(
        file({
          kind: "directory",
          display_path: "~/.claude/rules/",
          abs_path: "/tmp/.claude/rules",
        }),
      ),
    ).toBe(true);
  });

  it("hides the entry when the path is locally invisible", () => {
    expect(
      canOpenInstruction(
        file({
          display_path: "Cursor 账号级偏好",
          abs_path: "",
          load_status: "locally_invisible",
          action: "cursor_settings",
        }),
      ),
    ).toBe(false);
  });
});

describe("showsLoadStatus", () => {
  it("hides load status when the source has no global-instruction mechanism", () => {
    expect(
      showsLoadStatus(
        file({
          display_path: "无用户级全局指令机制",
          abs_path: "",
          load_status: "not_created",
          evidence: "no_mechanism",
        }),
      ),
    ).toBe(false);
    expect(showsLoadStatus(file({ load_status: "not_created" }))).toBe(true);
  });
});

describe("canEditInstruction", () => {
  it("does not offer the in-app editor for a directory", () => {
    expect(
      canEditInstruction(
        file({
          kind: "directory",
          display_path: "~/.claude/rules/",
          abs_path: "/tmp/.claude/rules",
        }),
      ),
    ).toBe(false);
  });
});
