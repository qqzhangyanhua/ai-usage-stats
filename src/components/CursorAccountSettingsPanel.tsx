import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { humanStatus } from "../lib/format";
import { Button } from "./ui/Button";
import { Field } from "./ui/Field";

/**
 * 设置页的 Cursor 账号用量入口：粘贴会话 token（进钥匙串）以及独立清空缓存。
 * 清空不删 token、不联网；下次刷新按全量拉。
 */
export function CursorAccountSettingsPanel() {
  const [hasToken, setHasToken] = useState(false);
  const [tokenDraft, setTokenDraft] = useState("");
  const [busy, setBusy] = useState<"idle" | "saving" | "clearing">("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    invoke<boolean>("has_cursor_session_token")
      .then((configured) => {
        if (alive) {
          setHasToken(configured);
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
      setHasToken(true);
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
    <section className="panel">
      <div className="panel-head">
        <div>
          <h2>Cursor 账号用量</h2>
          <p className="panel-note">
            会话 token 存 macOS 钥匙串，不写配置文件。清空只删这张独立缓存表，不删
            token，也不触发联网，更不动本机消耗记录。
          </p>
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
          placeholder={hasToken ? "已保存在钥匙串，可覆盖" : "从 cursor.com 复制会话 cookie"}
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
