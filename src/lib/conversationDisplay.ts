import type { ConversationSessionRow } from "../types";
import { applicationLabel, projectLabel, relativeTime } from "./format";

const CAPABILITY_LABELS: Record<string, string> = {
  messages: "基础正文",
  events: "完整事件",
  usage: "用量明细",
};

export function capabilityLabel(capability: string): string {
  return CAPABILITY_LABELS[capability] ?? capability;
}

export function conversationApplicationLabel(source: string): string {
  return source === "cursor_agent" ? "Cursor / Cursor Agent" : applicationLabel(source);
}

export function conversationStatusLabel(status: string): string {
  return status === "experimental" ? "实验性" : status;
}

export function conversationFileUnavailableLabel(source: string): string {
  return source === "cursor_agent" ? "缺少 transcript" : "原文件已删除";
}

export function conversationSessionTime(
  session: Pick<ConversationSessionRow, "ended_at" | "started_at">,
): string {
  return session.ended_at || session.started_at;
}

export function conversationDetailSummary(session: ConversationSessionRow): string {
  const time = conversationSessionTime(session);
  const parts = [
    conversationApplicationLabel(session.source),
    projectLabel(session.project),
    session.model || "未标注",
  ];
  if (time) {
    parts.push(relativeTime(time));
  }
  return parts.join(" · ");
}
