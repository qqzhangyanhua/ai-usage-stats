import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { formatClock, formatTokens, humanStatus } from "../lib/format";
import type { CursorAccountUsageDto } from "../types";
import { EmptyState } from "./EmptyState";
import { KpiCard } from "./Kpi";
import { Button } from "./ui/Button";
import { Field } from "./ui/Field";

function emptyUsage(): CursorAccountUsageDto {
  return {
    as_of: null,
    event_count: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_read_tokens: 0,
    cache_creation_tokens: 0,
    total_tokens: 0,
  };
}

export function CursorAccountUsagePanel() {
  const [usage, setUsage] = useState<CursorAccountUsageDto | null>(null);
  const [hasToken, setHasToken] = useState(false);
  const [tokenDraft, setTokenDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    Promise.all([
      invoke<CursorAccountUsageDto>("get_cursor_account_usage"),
      invoke<boolean>("has_cursor_session_token"),
    ])
      .then(([next, configured]) => {
        if (!alive) {
          return;
        }
        setUsage(next);
        setHasToken(configured);
      })
      .catch((err: unknown) => {
        if (alive) {
          setError(humanStatus(err));
        }
      });
    return () => {
      alive = false;
    };
  }, []);

  async function handleSaveToken() {
    const value = tokenDraft.trim();
    if (!value) {
      setError("请先粘贴 WorkosCursorSessionToken");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await invoke("save_cursor_session_token", { token: value });
      setHasToken(true);
      setTokenDraft("");
    } catch (err: unknown) {
      setError(humanStatus(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleRefresh() {
    setBusy(true);
    setError(null);
    try {
      const token = tokenDraft.trim() || null;
      const next = await invoke<CursorAccountUsageDto>("refresh_cursor_account_usage", {
        token,
      });
      if (token) {
        setHasToken(true);
        setTokenDraft("");
      }
      setUsage(next);
    } catch (err: unknown) {
      setError(humanStatus(err));
    } finally {
      setBusy(false);
    }
  }

  const data = usage ?? emptyUsage();
  const asOf = formatClock(data.as_of);
  const showEmpty = data.event_count === 0 && data.total_tokens === 0;

  return (
    <div className="stack">
      <section className="panel partition">
        <div className="panel-head">
          <div>
            <h2>Cursor 账号用量</h2>
            <p className="note">
              云端账号 / 含全部设备 / 全时段 / 仅 token 无费用 · 最后刷新于 {asOf}
            </p>
          </div>
          <Button variant="accent" disabled={busy} onClick={() => void handleRefresh()}>
            {busy ? "刷新中…" : "刷新"}
          </Button>
        </div>
        <div className="token-row">
          <Field
            label="WorkosCursorSessionToken"
            type="password"
            autoComplete="off"
            placeholder={hasToken ? "已保存在钥匙串，可覆盖" : "从 cursor.com 复制会话 cookie"}
            value={tokenDraft}
            onChange={(event) => setTokenDraft(event.target.value)}
          />
          <Button disabled={busy || !tokenDraft.trim()} onClick={() => void handleSaveToken()}>
            保存到钥匙串
          </Button>
        </div>
        {error ? (
          <p className="panel-note snapshot-error" role="alert">
            {error}
          </p>
        ) : null}
      </section>

      {showEmpty ? (
        <div className="panel partition">
          <EmptyState
            icon="cursor"
            title="暂无 Cursor 账号用量"
            hint="粘贴 WorkosCursorSessionToken 后点刷新。该数据是账号级云端用量，不会并入本机 token 总量。"
          />
        </div>
      ) : (
        <section className="kpi-row">
          <KpiCard icon="trend" tone="purple" label="总量" value={formatTokens(data.total_tokens)} />
          <KpiCard icon="sessions" tone="cyan" label="输入" value={formatTokens(data.input_tokens)} />
          <KpiCard
            icon="model"
            tone="orange"
            label="输出"
            value={formatTokens(data.output_tokens)}
          />
          <KpiCard
            icon="daily"
            tone="blue"
            label="缓存读 / 写"
            value={`${formatTokens(data.cache_read_tokens)} / ${formatTokens(data.cache_creation_tokens)}`}
          />
        </section>
      )}
    </div>
  );
}
