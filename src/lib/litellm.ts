import { invoke } from "@tauri-apps/api/core";
import type { PriceSnapshotMeta } from "../types";

/**
 * LiteLLM 价目快照：内置一份随应用发布的默认价目，作为「用户单价 + 来源自带费用」之外的兜底，
 * 让费用开箱大体准；这里封装「读取当前快照元信息」与「可选联网刷新」。
 *
 * 刷新在 webview 侧发起 fetch（拉取体积较大的上游 JSON），再把原始文本交给 Rust 解析/落盘，
 * 避免给 Rust 引入 HTTP 依赖。
 */

export async function getSnapshotMeta(): Promise<PriceSnapshotMeta> {
  return invoke<PriceSnapshotMeta>("get_price_snapshot");
}

export async function resetSnapshot(): Promise<PriceSnapshotMeta> {
  return invoke<PriceSnapshotMeta>("reset_price_snapshot");
}

/** 联网刷新：拉取上游原始价目 → 交给 Rust 解析、落盘并热更新。 */
export async function refreshSnapshot(): Promise<PriceSnapshotMeta> {
  const url = await invoke<string>("get_price_snapshot_url");
  let response: Response;
  try {
    response = await fetch(url, { cache: "no-store" });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`拉取上游价目失败（请检查网络）：${detail}`, { cause: error });
  }
  if (!response.ok) {
    throw new Error(`拉取上游价目失败：HTTP ${response.status}`);
  }
  const raw = await response.text();
  return invoke<PriceSnapshotMeta>("refresh_price_snapshot", { raw });
}
