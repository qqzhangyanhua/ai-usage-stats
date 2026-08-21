import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { formatClock, humanStatus } from "../lib/format";
import { Button } from "./ui/Button";
import { Field } from "./ui/Field";

/**
 * 设置页的 Cursor 账号用量入口：优先自动读本机 Cursor 客户端的登录态，
 * 读不到才需要手动粘贴会话 token（进钥匙串）。独立清空缓存不删 token、不联网；
 * 下次刷新按全量拉。
 */
type CursorCredentialStatus = {
  source: "local" | "keychain" | "none";
  email: string | null;
  expires_at: string | null;
  local_expired: boolean;
};

function describeCredential(status: CursorCredentialStatus): string {
  if (status.source === "local") {
    const who = status.email ? `（${status.email}）` : "";
    const until = status.expires_at ? `，有效期至 ${formatClock(status.expires_at)}` : "";
    return `已自动读取本机 Cursor 客户端登录态${who}${until}，无需粘贴 cookie。`;
  }
  if (status.source === "keychain") {
    return status.local_expired
      ? "本机 Cursor 登录态已过期，当前用钥匙串里手动保存的 token；在 Cursor 里重新登录即可恢复自动读取。"
      : "当前用钥匙串里手动保存的 token。在本机 Cursor 客户端登录后可改为自动读取。";
  }
  return status.local_expired
    ? "本机 Cursor 登录态已过期，请在 Cursor 客户端重新登录，或在下方粘贴 WorkosCursorSessionToken。"
    : "尚未配置。在本机 Cursor 客户端登录即可自动读取，或在下方粘贴 WorkosCursorSessionToken。";
}

export function CursorAccountSettingsPanel() {
  const [status, setStatus] = useState<CursorCredentialStatus | null>(null);
  const [tokenDraft, setTokenDraft] = useState("");
  const [busy, setBusy] = useState<"idle" | "saving" | "clearing">("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    invoke<CursorCredentialStatus>("get_cursor_credential_status")
      .then((next) => {
        if (alive) {
          setStatus(next);
        }
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
    setBusy("saving");
    setMessage(null);
    setError(null);
    try {
      await invoke("save_cursor_session_token", { token: value });
      setStatus(await invoke<CursorCredentialStatus>("get_cursor_credential_status"));
      setTokenDraft("");
      setMessage("已保存到钥匙串，可到 Cursor 页点刷新拉取账号用量");
    } catch (err: unknown) {
      setError(humanStatus(err));
    } finally {
      setBusy("idle");
    }
  }

  async function handleClearCache() {
    setBusy("clearing");
    setMessage(null);
    setError(null);
    try {
      await invoke("clear_cursor_account_usage");
      setMessage("已清空 Cursor 账号用量缓存，本机消耗记录未改动；下次刷新将重新拉全量");
    } catch (err: unknown) {
      setError(humanStatus(err));
    } finally {
      setBusy("idle");
    }
  }

  return (
    <section className="panel" id="settings-cursor-account">
      <div className="panel-head">
        <div>
          <h2>Cursor 账号用量</h2>
          <p className="panel-note">
            优先读本机 Cursor 客户端的登录态（只读，不写 Cursor 任何文件）；读不到才需要手动粘贴，
            手动值存 macOS 钥匙串，不写配置文件。清空只删这张独立缓存表，不删
            token，也不触发联网，更不动本机消耗记录。
          </p>
          {status ? (
            <p className="panel-note" role="status">
              {describeCredential(status)}
            </p>
          ) : null}
        </div>
        <Button variant="danger" disabled={busy !== "idle"} onClick={() => void handleClearCache()}>
          {busy === "clearing" ? "清空中…" : "清空账号用量缓存"}
        </Button>
      </div>
      <div className="token-row">
        <Field
          label="WorkosCursorSessionToken"
          type="password"
          autoComplete="off"
          placeholder={
            status?.source === "local"
              ? "已自动读取本机登录态，一般不用填"
              : status?.source === "keychain"
                ? "已保存在钥匙串，可覆盖"
                : "从 cursor.com 复制会话 cookie"
          }
          value={tokenDraft}
          onChange={(event) => setTokenDraft(event.target.value)}
        />
        <Button
          variant="accent"
          disabled={busy !== "idle" || !tokenDraft.trim()}
          onClick={() => void handleSaveToken()}
        >
          {busy === "saving" ? "保存中…" : "保存到钥匙串"}
        </Button>
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
