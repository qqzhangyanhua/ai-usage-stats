import { useEffect, useState } from "react";
import { getSnapshotMeta, refreshSnapshot, resetSnapshot } from "../lib/litellm";
import type { PriceSnapshotMeta } from "../types";
import { Button } from "./ui/Button";

type Busy = "idle" | "refreshing" | "resetting";

/**
 * LiteLLM 价目快照面板：展示当前生效快照，并提供「可选刷新」与「恢复内置」。
 * 快照作为费用兜底——某模型既无来源自带费用、用户也没配单价时，用它把费用从「能算」变成「大体准」。
 */
export function LiteLlmSnapshotPanel({ onRefreshed }: { onRefreshed?: () => void }) {
  const [meta, setMeta] = useState<PriceSnapshotMeta | null>(null);
  const [busy, setBusy] = useState<Busy>("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    getSnapshotMeta()
      .then((value) => {
        if (alive) {
          setMeta(value);
        }
      })
      .catch((err: unknown) => {
        if (alive) {
          setError(err instanceof Error ? err.message : String(err));
        }
      });
    return () => {
      alive = false;
    };
  }, []);

  async function handleRefresh() {
    setBusy("refreshing");
    setMessage(null);
    setError(null);
    try {
      const next = await refreshSnapshot();
      setMeta(next);
      setMessage(`已更新为最新价目：${next.count} 个模型（${next.as_of}），费用估算已按新价目重算`);
      onRefreshed?.();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy("idle");
    }
  }

  async function handleReset() {
    setBusy("resetting");
    setMessage(null);
    setError(null);
    try {
      const next = await resetSnapshot();
      setMeta(next);
      setMessage("已恢复为内置价目快照");
      onRefreshed?.();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy("idle");
    }
  }

  return (
    <section className="panel" id="settings-litellm">
      <div className="panel-head">
        <div>
          <h2>LiteLLM 价目快照</h2>
          <p className="panel-note">
            内置一份社区维护（LiteLLM）的模型价目作为兜底：某模型没有来源自带费用、你也没在下方配置单价时，
            用它估算费用，让总花费开箱大体准。你的自定义单价始终优先，快照只补齐未配置的模型。
          </p>
        </div>
        <div className="row-actions">
          <Button variant="accent" disabled={busy !== "idle"} onClick={handleRefresh}>
            {busy === "refreshing" ? "刷新中…" : "联网刷新"}
          </Button>
          <Button
            disabled={busy !== "idle" || (meta?.bundled ?? true)}
            onClick={handleReset}
          >
            {busy === "resetting" ? "恢复中…" : "恢复内置"}
          </Button>
        </div>
      </div>

      <div className="snapshot-meta">
        {meta ? (
          <>
            <span className="snapshot-badge">{meta.bundled ? "内置默认" : "已联网刷新"}</span>
            <span>
              来源 <strong>{meta.source}</strong>
            </span>
            <span>
              模型数 <strong>{meta.count}</strong>
            </span>
            <span>
              价目日期 <strong>{meta.as_of || "—"}</strong>
            </span>
          </>
        ) : (
          <span className="panel-note">正在读取快照信息…</span>
        )}
      </div>

      {message ? (
        <p className="panel-note preset-message" role="status">
          {message}
        </p>
      ) : null}
      {error ? (
        <p className="panel-note snapshot-error" role="alert">
          {error}
        </p>
      ) : null}
    </section>
  );
}
