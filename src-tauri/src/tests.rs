use crate::adapters::cursor::{
    parse_cursor_commits, summarize_code_volume, with_cost_roi, CursorCommitRow,
};
use crate::adapters::cursor_account;
use crate::adapters::opencode::{parse_opencode_messages, OpencodeMessage};
use crate::adapters::{
    claude, codex, copilot, cursor_agent, dsh, factory, gemini, grok, kimi, pi, qwen,
};
use crate::aggregate;
use crate::backup;
use crate::billing_window;
use crate::budget;
use crate::cost::derive_cost;
use crate::domain::{
    BudgetConfig, CostSource, Filter, PriceEntry, PriceOrigin, PriceTable, SessionQuery, Source,
    UsageRecord,
};
use crate::ingest;
use crate::query;
use crate::store;
use chrono::{Datelike, Local, NaiveTime, TimeZone, Timelike, Utc};
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(path).expect("fixture")
}

fn rec(
    occurred_at: &str,
    source: Source,
    model: &str,
    provider: &str,
    project: &str,
    session_id: &str,
    total: i64,
) -> UsageRecord {
    UsageRecord {
        occurred_at: occurred_at.to_string(),
        source,
        model: model.to_string(),
        provider: provider.to_string(),
        project: project.to_string(),
        session_id: session_id.to_string(),
        source_file: format!("/{session_id}.jsonl"),
        input_tokens: total,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: total,
        native_cost: None,
    }
}

#[test]
fn codex_adapter_counts_last_token_usage_not_cumulative() {
    let records = codex::parse_codex_jsonl(
        &fixture("codex.jsonl"),
        "/Users/zhangyanhua/.codex/sessions/rollout.jsonl",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].source, Source::Codex);
    assert_eq!(records[0].model, "gpt-5.1-codex");
    assert_eq!(records[0].provider, "codex_local_access");
    assert_eq!(
        records[0].project,
        "/Users/zhangyanhua/AI/chord-creator-studio"
    );
    assert_eq!(
        records[0].session_id,
        "019a9618-5abf-7892-be63-df90ece3d676"
    );
    assert_eq!(records[0].input_tokens, 8904);
    assert_eq!(records[0].cache_read_tokens, 1024);
    assert_eq!(records[0].output_tokens, 592);
    assert_eq!(records[0].total_tokens, 9496);
    assert_eq!(records[1].input_tokens, 9509);
    assert_eq!(records[1].output_tokens, 108);
    assert_eq!(records[1].reasoning_tokens, 64);
    assert_eq!(records[1].total_tokens, 9617);
    let summed: i64 = records.iter().map(|r| r.total_tokens).sum();
    assert_eq!(summed, 19113);
    assert_ne!(summed, 9496 + 19113);
}

#[test]
fn codex_adapter_falls_back_to_total_token_usage_delta() {
    let records = codex::parse_codex_jsonl(
        &fixture("codex-total-only.jsonl"),
        "/Users/zhangyanhua/.codex/sessions/rollout.jsonl",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].input_tokens, 100);
    assert_eq!(records[0].cache_read_tokens, 50);
    assert_eq!(records[0].output_tokens, 10);
    assert_eq!(records[1].input_tokens, 50);
    assert_eq!(records[1].cache_read_tokens, 25);
    assert_eq!(records[1].output_tokens, 5);
    let summed: i64 = records.iter().map(|r| r.input_tokens).sum();
    assert_eq!(summed, 150);
}

#[test]
fn claude_adapter_maps_usage_and_project_dir() {
    let records = claude::parse_claude_jsonl(
        &fixture("claude.jsonl"),
        "/Users/zhangyanhua/.claude/projects/-Users-zhangyanhua-AI-TradingAgents-CN/04868551-34c3-4588-b984-6ae9a5d95f8a.jsonl",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].source, Source::Claude);
    assert_eq!(records[0].model, "claude-sonnet-5");
    assert_eq!(
        records[0].session_id,
        "04868551-34c3-4588-b984-6ae9a5d95f8a"
    );
    assert_eq!(records[0].project, "/Users/zhangyanhua/AI/TradingAgents-CN");
    assert_eq!(records[0].input_tokens, 0);
    assert_eq!(records[0].output_tokens, 62);
    assert_eq!(records[0].cache_read_tokens, 0);
    assert_eq!(records[0].cache_creation_tokens, 56332);
    assert_eq!(records[0].reasoning_tokens, 0);
    assert_eq!(records[0].total_tokens, 56394);
    assert_eq!(records[1].input_tokens, 120);
    assert_eq!(records[1].output_tokens, 40);
    assert_eq!(records[1].cache_read_tokens, 56332);
    assert_eq!(records[1].cache_creation_tokens, 0);
    assert_eq!(records[1].total_tokens, 56492);
    assert!((records[0].native_cost.unwrap() - 0.0123).abs() < 1e-9);
    assert!((records[1].native_cost.unwrap() - 0.0081).abs() < 1e-9);
}

#[test]
fn claude_adapter_dedups_message_id_and_skips_zero_usage() {
    let records = claude::parse_claude_jsonl(
        &fixture("claude-dedup.jsonl"),
        "/Users/zhangyanhua/.claude/projects/-Users-zhangyanhua-AI-cli/s-claude.jsonl",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].input_tokens, 2);
    assert_eq!(records[0].output_tokens, 80);
    assert_eq!(records[0].cache_read_tokens, 48719);
    assert_eq!(records[0].cache_creation_tokens, 2061);
    assert!((records[0].native_cost.unwrap() - 0.05).abs() < 1e-9);
    assert_eq!(records[1].input_tokens, 10);
    assert_eq!(records[1].output_tokens, 4);
    assert!(records[1].native_cost.is_none());
}

#[test]
fn pi_adapter_uses_native_cost() {
    let records = pi::parse_pi_jsonl(
        &fixture("pi.jsonl"),
        "/Users/zhangyanhua/.pi/agent/sessions/--Users-zhangyanhua-workCode-ruoyi-ui-vue3--/s.jsonl",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].source, Source::Pi);
    assert_eq!(records[0].model, "gpt-5.5");
    assert_eq!(records[0].provider, "subapi");
    assert_eq!(
        records[0].project,
        "/Users/zhangyanhua/workCode/ruoyi-ui-vue3"
    );
    assert_eq!(
        records[0].session_id,
        "019f5abc-b360-79e4-bd7d-9a794da8cfc5"
    );
    assert_eq!(records[0].input_tokens, 12658);
    assert_eq!(records[0].output_tokens, 35);
    assert_eq!(records[0].cache_read_tokens, 0);
    assert_eq!(records[0].cache_creation_tokens, 0);
    assert_eq!(records[0].reasoning_tokens, 12);
    assert_eq!(records[0].total_tokens, 12693);
    assert_eq!(records[0].native_cost, Some(0.06434));
    assert_eq!(records[1].input_tokens, 517);
    assert_eq!(records[1].output_tokens, 41);
    assert_eq!(records[1].cache_read_tokens, 12288);
    assert_eq!(records[1].cache_creation_tokens, 0);
    assert_eq!(records[1].reasoning_tokens, 13);
    assert_eq!(records[1].total_tokens, 12846);
    assert!((records[1].native_cost.unwrap() - 0.009959).abs() < 1e-9);
}

#[test]
fn pi_adapter_skips_zero_token_assistant_messages() {
    // fixture 里追加了一条 usage 四分项全 0 的 assistant 消息（a3），
    // 与其它 adapter（claude/codex/gemini/opencode）保持一致：不计入会话/费用统计。
    let records = pi::parse_pi_jsonl(
        &fixture("pi.jsonl"),
        "/Users/zhangyanhua/.pi/agent/sessions/--Users-zhangyanhua-workCode-ruoyi-ui-vue3--/s.jsonl",
    );
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r.total_tokens > 0));
}

#[test]
fn opencode_adapter_skips_user_and_keeps_native_cost() {
    let raw = fixture("opencode-messages.json");
    let values: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
    let rows: Vec<OpencodeMessage> = values
        .into_iter()
        .map(|v| OpencodeMessage {
            session_id: v["session_id"].as_str().unwrap().to_string(),
            source_file: "opencode.db".to_string(),
            data: v["data"].clone(),
        })
        .collect();
    let records = parse_opencode_messages(&rows);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].source, Source::Opencode);
    assert_eq!(records[0].model, "gemini-claude-sonnet-4-5-thinking");
    assert_eq!(records[0].provider, "anthropic");
    assert_eq!(
        records[0].project,
        "/Users/zhangyanhua/workCode/project_front"
    );
    assert_eq!(records[0].session_id, "ses_4064c35bcffeKnRpPdbo4Ege2l");
    assert_eq!(records[0].input_tokens, 20882);
    assert_eq!(records[0].output_tokens, 138);
    assert_eq!(records[0].cache_read_tokens, 100);
    assert_eq!(records[0].cache_creation_tokens, 20);
    assert_eq!(records[0].reasoning_tokens, 0);
    assert_eq!(records[0].total_tokens, 21140);
    assert_eq!(records[0].native_cost, Some(0.42));
}

#[test]
fn opencode_adapter_ignores_zero_native_cost() {
    let rows = [OpencodeMessage {
        session_id: "s1".to_string(),
        source_file: "opencode.db".to_string(),
        data: serde_json::json!({
            "role": "assistant",
            "modelID": "mimo-v2.5-pro",
            "time": { "created": 1, "completed": 2 },
            "tokens": { "input": 1000, "output": 200 },
            "cost": 0.0
        }),
    }];
    let records = parse_opencode_messages(&rows);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].input_tokens, 1000);
    assert_eq!(records[0].output_tokens, 200);
    assert_eq!(records[0].native_cost, None);
}

#[test]
fn kimi_adapter_keeps_last_status_update_per_turn() {
    let records = kimi::parse_kimi_wire(
        &fixture("kimi-wire.jsonl"),
        "/Users/zhangyanhua/.kimi/sessions/hash/bd1ab6fc-768d-4cff-b4c4-221a583c3af8/wire.jsonl",
        "/Users/zhangyanhua/workCode/app-storage",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].source, Source::Kimi);
    assert_eq!(
        records[0].session_id,
        "bd1ab6fc-768d-4cff-b4c4-221a583c3af8"
    );
    assert_eq!(
        records[0].project,
        "/Users/zhangyanhua/workCode/app-storage"
    );
    assert_eq!(records[0].model, "");
    assert_eq!(records[0].input_tokens, 3000);
    assert_eq!(records[0].output_tokens, 200);
    assert_eq!(records[0].cache_read_tokens, 4352);
    assert_eq!(records[0].cache_creation_tokens, 10);
    assert_eq!(records[0].reasoning_tokens, 0);
    assert_eq!(records[0].total_tokens, 7562);
    assert_ne!(records[0].input_tokens, 2547);
    assert_ne!(records[0].output_tokens, 142);
    assert_eq!(records[1].input_tokens, 330);
    assert_eq!(records[1].output_tokens, 339);
    assert_eq!(records[1].cache_read_tokens, 6656);
    assert_eq!(records[1].cache_creation_tokens, 0);
    assert_eq!(records[1].total_tokens, 7325);
}

#[test]
fn dsh_adapter_reads_final_assistant_turn_not_chunks() {
    let records = dsh::parse_dsh_jsonl(
        &fixture("dsh.jsonl"),
        "/Users/zhangyanhua/.dsh/sessions/--Users-zhangyanhua-AI-pi--/session.jsonl.zstd",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].source, Source::Dsh);
    assert_eq!(records[0].model, "deepseek-v4-flash");
    assert_eq!(records[0].provider, "deepseek-official");
    assert_eq!(records[0].project, "/Users/zhangyanhua/AI/pi");
    assert_eq!(
        records[0].session_id,
        "session-f1cbbe01-e379-4152-8d13-46440f595d2d"
    );
    assert_eq!(records[0].input_tokens, 13672);
    assert_eq!(records[0].output_tokens, 442);
    assert_eq!(records[0].cache_read_tokens, 0);
    assert_eq!(records[0].cache_creation_tokens, 0);
    assert_eq!(records[0].reasoning_tokens, 321);
    assert_eq!(records[0].total_tokens, 14435);
    assert_ne!(records[0].input_tokens, 1);
    assert_eq!(records[1].input_tokens, 1603);
    assert_eq!(records[1].output_tokens, 430);
    assert_eq!(records[1].cache_read_tokens, 14080);
    assert_eq!(records[1].reasoning_tokens, 281);
    assert_eq!(records[1].total_tokens, 16394);
}

#[test]
fn dsh_adapter_reads_compressed_session_as_usage_records() {
    let raw = fixture("dsh.jsonl");
    let compressed = zstd::encode_all(raw.as_bytes(), 0).unwrap();
    let records = dsh::parse_dsh_zstd(&compressed, "session.jsonl.zstd").unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].input_tokens, 13672);
    assert_eq!(records[0].total_tokens, 14435);
    assert_eq!(records[1].cache_read_tokens, 14080);
}

#[test]
fn gemini_adapter_maps_chat_tokens() {
    let records = gemini::parse_gemini_session(
        &fixture("gemini-session.json"),
        "/Users/zhangyanhua/.gemini/tmp/ruoyi-ui-vue3/chats/session-2026-03-07.json",
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].source, Source::Gemini);
    assert_eq!(records[0].model, "gemini-3-flash-preview");
    assert_eq!(
        records[0].session_id,
        "2392a2f0-142a-407e-a08f-8f37781ba76c"
    );
    assert_eq!(records[0].project, "ruoyi-ui-vue3");
    assert_eq!(records[0].input_tokens, 13354);
    assert_eq!(records[0].output_tokens, 662);
    assert_eq!(records[0].cache_read_tokens, 0);
    assert_eq!(records[0].cache_creation_tokens, 0);
    assert_eq!(records[0].reasoning_tokens, 285);
    assert_eq!(records[0].total_tokens, 14301);
}

#[test]
fn grok_adapter_decodes_project_and_dedups_prompt() {
    let records = grok::parse_grok_updates(
        &fixture("grok-updates.jsonl"),
        "/Users/zhangyanhua/.grok/sessions/%2FUsers%2Fzhangyanhua%2FAI%2FTradingAgents-CN/019fd235/updates.jsonl",
        "grok-4.5",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].source, Source::Grok);
    assert_eq!(records[0].project, "/Users/zhangyanhua/AI/TradingAgents-CN");
    assert_eq!(records[0].session_id, "019fd235");
    assert_eq!(records[0].model, "grok-4.5");
    assert_eq!(records[0].input_tokens, 0);
    assert_eq!(records[0].output_tokens, 0);
    assert_eq!(records[0].cache_read_tokens, 0);
    assert_eq!(records[0].cache_creation_tokens, 0);
    assert_eq!(records[0].reasoning_tokens, 0);
    assert_eq!(records[0].total_tokens, 26857);
    assert_ne!(records[0].total_tokens, 15681);
    assert_eq!(records[1].total_tokens, 71351);
}

#[test]
fn grok_adapter_reads_turn_completed_usage_not_context_total() {
    let records = grok::parse_grok_updates(
        &fixture("grok-turn-completed.jsonl"),
        "/Users/zhangyanhua/.grok/sessions/%2FUsers%2Fzhangyanhua%2FAI%2FTradingAgents-CN/019fd235/updates.jsonl",
        "grok-4.5",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].source, Source::Grok);
    assert_eq!(records[0].project, "/Users/zhangyanhua/AI/TradingAgents-CN");
    assert_eq!(records[0].session_id, "019fd235");
    assert_eq!(records[0].model, "grok-4.6-build");
    assert_eq!(records[0].input_tokens, 447430);
    assert_eq!(records[0].output_tokens, 4742);
    assert_eq!(records[0].cache_read_tokens, 410112);
    assert_eq!(records[0].cache_creation_tokens, 0);
    assert_eq!(records[0].reasoning_tokens, 3567);
    assert_eq!(records[0].total_tokens, 452172);
    assert_ne!(records[0].total_tokens, 15681);
    assert!((records[0].native_cost.unwrap() - 0.308144).abs() < 1e-9);
    assert_eq!(records[1].input_tokens, 100);
    assert_eq!(records[1].output_tokens, 10);
    assert_eq!(records[1].cache_read_tokens, 5);
    assert_eq!(records[1].reasoning_tokens, 3);
    assert_eq!(records[1].total_tokens, 110);
    assert!((records[1].native_cost.unwrap() - 0.1).abs() < 1e-9);
}

#[test]
fn qwen_adapter_returns_empty_when_no_tokens() {
    let records = qwen::parse_qwen_session(&fixture("qwen-logs.json"), "logs.json");
    assert!(records.is_empty());
}

#[test]
fn factory_adapter_maps_session_token_usage() {
    let records = factory::parse_factory_settings(
        &fixture("factory.settings.json"),
        "/Users/zhangyanhua/.factory/sessions/-Users-zhangyanhua-AI-cli/9ab2ca7b-bd30-495b-9434-07892ee0e5e6.settings.json",
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].source, Source::Factory);
    assert_eq!(records[0].provider, "anthropic");
    assert_eq!(records[0].model, "");
    assert_eq!(records[0].project, "/Users/zhangyanhua/AI/cli");
    assert_eq!(
        records[0].session_id,
        "9ab2ca7b-bd30-495b-9434-07892ee0e5e6"
    );
    assert_eq!(records[0].input_tokens, 3);
    assert_eq!(records[0].output_tokens, 1022);
    assert_eq!(records[0].cache_creation_tokens, 8125);
    assert_eq!(records[0].cache_read_tokens, 11084);
    assert_eq!(records[0].reasoning_tokens, 0);
    assert_eq!(records[0].total_tokens, 20234);
}

#[test]
fn cursor_agent_adapter_maps_result_usage_per_turn() {
    let records = cursor_agent::parse_cursor_agent_jsonl(
        &fixture("cursor-agent-stream.jsonl"),
        "/Users/dev/.cursor-agent-usage/3ce011d4-33d1-41d0-a16c-f6dc206c47f1.jsonl",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].source, Source::CursorAgent);
    assert_eq!(records[0].model, "Cursor Grok 4.6 High Fast");
    assert_eq!(records[0].provider, "");
    assert_eq!(records[0].project, "/Users/dev/project");
    assert_eq!(
        records[0].session_id,
        "3ce011d4-33d1-41d0-a16c-f6dc206c47f1"
    );
    assert_eq!(records[0].occurred_at, "2026-08-17T05:31:13.226190+00:00");
    assert_eq!(records[0].input_tokens, 18851);
    assert_eq!(records[0].output_tokens, 35);
    assert_eq!(records[0].cache_read_tokens, 0);
    assert_eq!(records[0].cache_creation_tokens, 0);
    assert_eq!(records[0].reasoning_tokens, 0);
    assert_eq!(records[0].total_tokens, 18886);
    assert!(records[0].native_cost.is_none());
    // 第二轮：cacheWriteTokens 映射到 cache_creation，total 为各口径之和。
    assert_eq!(records[1].cache_creation_tokens, 400);
    assert_eq!(records[1].total_tokens, 1000);
}

#[test]
fn copilot_adapter_only_uses_the_last_shutdown_snapshot_per_session() {
    let records = copilot::parse_copilot_jsonl(
        &fixture("copilot-events.jsonl"),
        "/Users/dev/.copilot/session-state/c0ffee11-2222-4333-8444-555566667777/events.jsonl",
    );
    // 文件里有两次 session.shutdown（会话续接两次）；只应采信最后一次的累计用量，
    // 否则会把第一次 shutdown 的 gpt-5.4 用量重复计入。
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].source, Source::Copilot);
    assert_eq!(records[0].model, "claude-sonnet-4.5");
    assert_eq!(records[0].provider, "");
    assert_eq!(records[0].project, "/Users/dev/ai-usage-stats");
    assert_eq!(
        records[0].session_id,
        "c0ffee11-2222-4333-8444-555566667777"
    );
    assert_eq!(records[0].occurred_at, "2026-08-10T15:12:30.500Z");
    assert_eq!(records[0].input_tokens, 21583);
    assert_eq!(records[0].output_tokens, 1064);
    assert_eq!(records[0].cache_read_tokens, 21187);
    assert_eq!(records[0].cache_creation_tokens, 0);
    assert_eq!(records[0].total_tokens, 21583 + 1064 + 21187);

    assert_eq!(records[1].model, "gpt-5.4");
    assert_eq!(records[1].input_tokens, 244120);
    assert_eq!(records[1].output_tokens, 2383);
    assert_eq!(records[1].cache_read_tokens, 202112);
}

#[test]
fn copilot_adapter_falls_back_to_parent_dir_name_when_session_id_is_missing() {
    let content = r#"{"type":"session.shutdown","timestamp":"2026-08-11T00:00:00.000Z","data":{"modelMetrics":{"gpt-5.4":{"usage":{"inputTokens":10,"outputTokens":5,"cacheReadTokens":0,"cacheWriteTokens":0}}}}}"#;
    let records = copilot::parse_copilot_jsonl(
        content,
        "/Users/dev/.copilot/session-state/no-start-event/events.jsonl",
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].session_id, "no-start-event");
    assert_eq!(records[0].project, "");
}

#[test]
fn source_maps_to_user_facing_application_names() {
    assert_eq!(Source::Claude.application_name(), "Claude Code");
    assert_eq!(Source::Codex.application_name(), "Codex");
    assert_eq!(Source::Factory.application_name(), "Droid");
    assert_eq!(Source::Opencode.application_name(), "OpenCode");
    assert_eq!(Source::Dsh.application_name(), "DeepSeek Harness");
    assert_eq!(Source::CursorAgent.application_name(), "Cursor Agent");
    assert_eq!(Source::Copilot.application_name(), "GitHub Copilot CLI");
}

#[test]
fn application_breakdown_ranks_user_facing_apps() {
    let records = seed_records();
    let rows = aggregate::by_name(
        &records,
        &Filter::default(),
        &PriceTable::default(),
        |record| record.source.application_name().to_string(),
    );

    assert_eq!(rows[0].name, "Claude Code");
    assert_eq!(rows[0].total_tokens, 300);
    assert_eq!(rows[1].name, "Codex");
    assert_eq!(rows[1].total_tokens, 100);
    assert_eq!(rows[2].name, "Pi");
    assert_eq!(rows[2].total_tokens, 50);
}

#[test]
fn application_analytics_builds_trend_matrix_and_efficiency_metrics() {
    let mut codex_day_one = rec(
        "2026-08-01T10:00:00Z",
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj/a",
        "codex-session",
        100,
    );
    codex_day_one.input_tokens = 80;
    codex_day_one.cache_read_tokens = 20;
    codex_day_one.reasoning_tokens = 10;

    let mut codex_day_two = rec(
        "2026-08-02T10:00:00Z",
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj/a",
        "codex-session",
        50,
    );
    codex_day_two.input_tokens = 40;
    codex_day_two.cache_read_tokens = 10;
    codex_day_two.reasoning_tokens = 5;

    let mut claude_project_a = rec(
        "2026-08-01T11:00:00Z",
        Source::Claude,
        "claude-sonnet-5",
        "anthropic",
        "/proj/a",
        "claude-a",
        200,
    );
    claude_project_a.input_tokens = 100;
    claude_project_a.cache_read_tokens = 100;
    claude_project_a.reasoning_tokens = 20;

    let mut claude_project_b = rec(
        "2026-08-02T11:00:00Z",
        Source::Claude,
        "claude-sonnet-5",
        "anthropic",
        "/proj/b",
        "claude-b",
        100,
    );
    claude_project_b.input_tokens = 0;

    let records = vec![
        codex_day_one,
        codex_day_two,
        claude_project_a,
        claude_project_b,
    ];
    let analytics = aggregate::application_analytics(&records, &Filter::default(), "day");

    assert_eq!(analytics.summary.total_tokens, 450);
    assert_eq!(analytics.summary.session_count, 3);
    assert_eq!(analytics.summary.average_session_tokens, Some(150.0));
    assert!((analytics.summary.cache_hit_rate.unwrap() - 130.0 / 350.0).abs() < 1e-9);
    assert!((analytics.summary.reasoning_share.unwrap() - 35.0 / 450.0).abs() < 1e-9);

    assert_eq!(analytics.by_application.len(), 2);
    assert_eq!(analytics.by_application[0].application, "Claude Code");
    assert_eq!(analytics.by_application[0].metrics.total_tokens, 300);
    assert_eq!(analytics.by_application[0].metrics.session_count, 2);
    assert_eq!(
        analytics.by_application[0].metrics.average_session_tokens,
        Some(150.0)
    );
    assert_eq!(
        analytics.by_application[0].metrics.cache_hit_rate,
        Some(0.5)
    );
    assert!(
        (analytics.by_application[0].metrics.reasoning_share.unwrap() - 1.0 / 15.0).abs() < 1e-9
    );
    assert_eq!(analytics.by_application[1].source, "codex");
    assert_eq!(
        analytics.by_application[1].metrics.cache_hit_rate,
        Some(0.2)
    );

    assert_eq!(analytics.trend.len(), 2);
    assert_eq!(analytics.trend[0].bucket, "2026-08-01");
    assert_eq!(analytics.trend[0].total_tokens, 300);
    assert_eq!(analytics.trend[0].values["codex"], 100);
    assert_eq!(analytics.trend[0].values["claude"], 200);
    assert_eq!(analytics.trend[1].total_tokens, 150);

    assert_eq!(analytics.projects.len(), 2);
    assert_eq!(analytics.projects[0].project, "/proj/a");
    assert_eq!(analytics.projects[0].total_tokens, 350);
    assert_eq!(analytics.projects[0].values["codex"], 150);
    assert_eq!(analytics.projects[0].values["claude"], 200);
    assert_eq!(analytics.projects[1].project, "/proj/b");

    let filtered = aggregate::application_analytics(
        &records,
        &Filter {
            projects: vec!["/proj/b".into()],
            ..Filter::default()
        },
        "month",
    );
    assert_eq!(filtered.summary.total_tokens, 100);
    assert_eq!(filtered.by_application.len(), 1);
    assert_eq!(filtered.by_application[0].application, "Claude Code");
    assert_eq!(filtered.trend[0].bucket, "2026-08");
    assert_eq!(filtered.projects.len(), 1);
}

#[test]
fn application_efficiency_returns_none_when_ratio_denominators_are_zero() {
    let records = vec![rec(
        "2026-08-01T10:00:00Z",
        Source::Factory,
        "",
        "anthropic",
        "",
        "droid-session",
        0,
    )];
    let analytics = aggregate::application_analytics(&records, &Filter::default(), "day");

    assert_eq!(analytics.summary.cache_hit_rate, None);
    assert_eq!(analytics.summary.reasoning_share, None);
    assert_eq!(analytics.summary.average_session_tokens, Some(0.0));
    assert_eq!(analytics.projects[0].project, "（未标注）");
}

#[test]
fn factory_adapter_root_settings_have_empty_project() {
    let records = factory::parse_factory_settings(
        &fixture("factory.settings.json"),
        "/Users/zhangyanhua/.factory/sessions/9ab2ca7b-bd30-495b-9434-07892ee0e5e6.settings.json",
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].project, "");
    assert_eq!(
        records[0].session_id,
        "9ab2ca7b-bd30-495b-9434-07892ee0e5e6"
    );
}

#[test]
fn cursor_code_volume_stays_outside_usage_records() {
    let commits = parse_cursor_commits(&[CursorCommitRow {
        commit_hash: "abc".into(),
        branch: "main".into(),
        scored_at_ms: 1_771_411_050_440,
        lines_added: 156,
        composer_lines_added: 32,
        human_lines_added: 0,
        ai_percentage: Some(100.0),
    }]);
    let summary = summarize_code_volume(&commits);
    assert_eq!(summary.commit_count, 1);
    assert_eq!(summary.lines_added, 156);
    assert_eq!(summary.composer_lines_added, 32);
    assert!((summary.ai_percentage.unwrap() - 20.51282051282051).abs() < 1e-9);
    assert_ne!(summary.ai_percentage.unwrap(), 100.0);

    let empty = summarize_code_volume(&[]);
    assert_eq!(empty.commit_count, 0);
    assert_eq!(empty.lines_added, 0);
    assert_eq!(empty.ai_percentage, None);

    let fallback = summarize_code_volume(&parse_cursor_commits(&[
        CursorCommitRow {
            commit_hash: "a".into(),
            branch: "main".into(),
            scored_at_ms: 1,
            lines_added: 0,
            composer_lines_added: 0,
            human_lines_added: 0,
            ai_percentage: Some(40.0),
        },
        CursorCommitRow {
            commit_hash: "b".into(),
            branch: "main".into(),
            scored_at_ms: 2,
            lines_added: 0,
            composer_lines_added: 0,
            human_lines_added: 0,
            ai_percentage: Some(60.0),
        },
    ]));
    assert_eq!(fallback.lines_added, 0);
    assert!((fallback.ai_percentage.unwrap() - 50.0).abs() < 1e-9);

    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &seed_records()).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 450);
    assert_eq!(stored.len(), 3);
}

#[test]
fn with_cost_roi_derives_cost_per_thousand_ai_lines() {
    let summary = summarize_code_volume(&parse_cursor_commits(&[CursorCommitRow {
        commit_hash: "abc".into(),
        branch: "main".into(),
        scored_at_ms: 1,
        lines_added: 4000,
        composer_lines_added: 2000,
        human_lines_added: 2000,
        ai_percentage: None,
    }]));

    let priced = with_cost_roi(summary.clone(), Some(30.0), false);
    assert_eq!(priced.total_cost, Some(30.0));
    assert!(!priced.cost_unpriced);
    // 2000 行 AI 代码花了 $30，即每千行 $15。
    assert!((priced.cost_per_thousand_ai_lines.unwrap() - 15.0).abs() < 1e-9);

    // 未配置任何单价时 cost 为 None，ROI 也应为 None，而不是被当成 0 处理。
    let unpriced = with_cost_roi(summary.clone(), None, true);
    assert_eq!(unpriced.cost_per_thousand_ai_lines, None);
    assert!(unpriced.cost_unpriced);

    // 没有任何 AI 生成行时分母为 0，即使有费用也不应该算出 ROI。
    let no_lines = summarize_code_volume(&[]);
    let no_lines_priced = with_cost_roi(no_lines, Some(10.0), false);
    assert_eq!(no_lines_priced.cost_per_thousand_ai_lines, None);
}

#[test]
fn load_code_volume_reads_sqlite_without_writing_usage() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    assert_eq!(ingest::load_code_volume(home).unwrap().commit_count, 0);
    assert_eq!(ingest::load_code_volume(home).unwrap().ai_percentage, None);

    let db_path = home.join(".cursor/ai-tracking/ai-code-tracking.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let src = rusqlite::Connection::open(&db_path).unwrap();
    src.execute_batch(
        r#"
        CREATE TABLE scored_commits (
            commitHash TEXT,
            branchName TEXT,
            scoredAt INTEGER,
            linesAdded INTEGER,
            composerLinesAdded INTEGER,
            humanLinesAdded INTEGER,
            v2AiPercentage TEXT
        );
        INSERT INTO scored_commits VALUES
            ('abc', 'main', 1771411050440, 156, 32, 0, '100'),
            ('skip', 'main', 1771411050441, NULL, NULL, NULL, NULL);
        "#,
    )
    .unwrap();
    drop(src);

    let conn = store::open_memory().unwrap();
    let report = ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(report.records_written, 0);
    assert!(store::load_all(&conn).unwrap().is_empty());

    let volume = ingest::load_code_volume(home).unwrap();
    assert_eq!(volume.commit_count, 1);
    assert_eq!(volume.lines_added, 156);
    assert_eq!(volume.composer_lines_added, 32);
    assert!((volume.ai_percentage.unwrap() - 20.51282051282051).abs() < 1e-9);
}

fn seed_records() -> Vec<UsageRecord> {
    vec![
        rec(
            "2026-08-01T10:00:00Z",
            Source::Codex,
            "gpt-5.1-codex",
            "official",
            "/proj/a",
            "s1",
            100,
        ),
        rec(
            "2026-08-02T10:00:00Z",
            Source::Claude,
            "claude-sonnet-5",
            "anthropic",
            "/proj/a",
            "s2",
            300,
        ),
        rec(
            "2026-08-08T10:00:00Z",
            Source::Pi,
            "gpt-5.5",
            "subapi",
            "/proj/b",
            "s3",
            50,
        ),
    ]
}

#[test]
fn overview_from_codex_fixture_uses_last_token_usage_totals() {
    let records = codex::parse_codex_jsonl(
        &fixture("codex.jsonl"),
        "/Users/zhangyanhua/.codex/sessions/rollout.jsonl",
    );
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 19113);
    assert_eq!(dto.input_tokens, 18413);
    assert_eq!(dto.output_tokens, 700);
    assert_eq!(dto.cache_read_tokens, 2048);
    assert_eq!(dto.cache_creation_tokens, 0);
    assert_eq!(dto.reasoning_tokens, 64);
    assert_eq!(dto.session_count, 1);
    assert_ne!(dto.total_tokens, 9496 + 19113);
}

#[test]
fn overview_from_claude_fixture_sums_per_record_token_dimensions() {
    let records = claude::parse_claude_jsonl(
        &fixture("claude.jsonl"),
        "/Users/zhangyanhua/.claude/projects/-Users-zhangyanhua-AI-TradingAgents-CN/04868551-34c3-4588-b984-6ae9a5d95f8a.jsonl",
    );
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 112886);
    assert_eq!(dto.input_tokens, 120);
    assert_eq!(dto.output_tokens, 102);
    assert_eq!(dto.cache_read_tokens, 56332);
    assert_eq!(dto.cache_creation_tokens, 56332);
    assert_eq!(dto.reasoning_tokens, 0);
    assert_eq!(dto.session_count, 1);
    assert!((dto.cost.unwrap() - 0.0204).abs() < 1e-9);
    assert!(!dto.unpriced);
}

#[test]
fn overview_from_pi_fixture_uses_native_cost() {
    let records = pi::parse_pi_jsonl(
        &fixture("pi.jsonl"),
        "/Users/zhangyanhua/.pi/agent/sessions/--Users-zhangyanhua-workCode-ruoyi-ui-vue3--/s.jsonl",
    );
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 25539);
    assert_eq!(dto.input_tokens, 13175);
    assert_eq!(dto.output_tokens, 76);
    assert_eq!(dto.cache_read_tokens, 12288);
    assert_eq!(dto.cache_creation_tokens, 0);
    assert_eq!(dto.reasoning_tokens, 25);
    assert_eq!(dto.session_count, 1);
    assert!((dto.cost.unwrap() - 0.074299).abs() < 1e-9);
    assert!(!dto.unpriced);
}

#[test]
fn overview_from_opencode_fixture_uses_native_cost() {
    let raw = fixture("opencode-messages.json");
    let values: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
    let rows: Vec<OpencodeMessage> = values
        .into_iter()
        .map(|v| OpencodeMessage {
            session_id: v["session_id"].as_str().unwrap().to_string(),
            source_file: "opencode.db".to_string(),
            data: v["data"].clone(),
        })
        .collect();
    let records = parse_opencode_messages(&rows);
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 21140);
    assert_eq!(dto.input_tokens, 20882);
    assert_eq!(dto.output_tokens, 138);
    assert_eq!(dto.cache_read_tokens, 100);
    assert_eq!(dto.cache_creation_tokens, 20);
    assert_eq!(dto.reasoning_tokens, 0);
    assert_eq!(dto.session_count, 1);
    assert_eq!(dto.cost, Some(0.42));
    assert!(!dto.unpriced);
}

#[test]
fn overview_from_kimi_fixture_uses_last_status_update_totals() {
    let records = kimi::parse_kimi_wire(
        &fixture("kimi-wire.jsonl"),
        "/Users/zhangyanhua/.kimi/sessions/hash/bd1ab6fc-768d-4cff-b4c4-221a583c3af8/wire.jsonl",
        "/Users/zhangyanhua/workCode/app-storage",
    );
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 14887);
    assert_eq!(dto.input_tokens, 3330);
    assert_eq!(dto.output_tokens, 539);
    assert_eq!(dto.cache_read_tokens, 11008);
    assert_eq!(dto.cache_creation_tokens, 10);
    assert_eq!(dto.reasoning_tokens, 0);
    assert_eq!(dto.session_count, 1);
    assert_ne!(dto.total_tokens, 2547 + 142 + 3000 + 200 + 330 + 339);
}

#[test]
fn overview_from_dsh_fixture_uses_final_assistant_totals() {
    let records = dsh::parse_dsh_jsonl(
        &fixture("dsh.jsonl"),
        "/Users/zhangyanhua/.dsh/sessions/--Users-zhangyanhua-AI-pi--/session.jsonl.zstd",
    );
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 30829);
    assert_eq!(dto.input_tokens, 15275);
    assert_eq!(dto.output_tokens, 872);
    assert_eq!(dto.cache_read_tokens, 14080);
    assert_eq!(dto.cache_creation_tokens, 0);
    assert_eq!(dto.reasoning_tokens, 602);
    assert_eq!(dto.session_count, 1);
    assert_ne!(dto.total_tokens, 30829 + 4);
}

#[test]
fn overview_from_gemini_fixture_sums_per_record_token_dimensions() {
    let records = gemini::parse_gemini_session(
        &fixture("gemini-session.json"),
        "/Users/zhangyanhua/.gemini/tmp/ruoyi-ui-vue3/chats/session-2026-03-07.json",
    );
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 14301);
    assert_eq!(dto.input_tokens, 13354);
    assert_eq!(dto.output_tokens, 662);
    assert_eq!(dto.cache_read_tokens, 0);
    assert_eq!(dto.cache_creation_tokens, 0);
    assert_eq!(dto.reasoning_tokens, 285);
    assert_eq!(dto.session_count, 1);
}

#[test]
fn overview_from_grok_fixture_uses_last_total_per_prompt() {
    let records = grok::parse_grok_updates(
        &fixture("grok-updates.jsonl"),
        "/Users/zhangyanhua/.grok/sessions/%2FUsers%2Fzhangyanhua%2FAI%2FTradingAgents-CN/019fd235/updates.jsonl",
        "grok-4.5",
    );
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 98208);
    assert_eq!(dto.input_tokens, 0);
    assert_eq!(dto.output_tokens, 0);
    assert_eq!(dto.cache_read_tokens, 0);
    assert_eq!(dto.cache_creation_tokens, 0);
    assert_eq!(dto.reasoning_tokens, 0);
    assert_eq!(dto.session_count, 1);
    assert_ne!(dto.total_tokens, 15681 + 26857 + 71351);
}

#[test]
fn overview_from_grok_turn_completed_uses_usage_not_context_total() {
    let records = grok::parse_grok_updates(
        &fixture("grok-turn-completed.jsonl"),
        "/Users/zhangyanhua/.grok/sessions/%2FUsers%2Fzhangyanhua%2FAI%2FTradingAgents-CN/019fd235/updates.jsonl",
        "grok-4.5",
    );
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 452282);
    assert_eq!(dto.input_tokens, 447530);
    assert_eq!(dto.output_tokens, 4752);
    assert_eq!(dto.cache_read_tokens, 410117);
    assert_eq!(dto.reasoning_tokens, 3570);
    assert_eq!(dto.session_count, 1);
    assert!((dto.cost.unwrap() - 0.408144).abs() < 1e-9);
    assert!(!dto.unpriced);
}

#[test]
fn overview_from_qwen_fixture_contributes_no_tokens() {
    let records = qwen::parse_qwen_session(&fixture("qwen-logs.json"), "logs.json");
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 0);
    assert_eq!(dto.input_tokens, 0);
    assert_eq!(dto.output_tokens, 0);
    assert_eq!(dto.cache_read_tokens, 0);
    assert_eq!(dto.cache_creation_tokens, 0);
    assert_eq!(dto.reasoning_tokens, 0);
    assert_eq!(dto.session_count, 0);
}

#[test]
fn overview_from_copilot_fixture_uses_last_shutdown_snapshot() {
    let records = copilot::parse_copilot_jsonl(
        &fixture("copilot-events.jsonl"),
        "/Users/dev/.copilot/session-state/c0ffee11-2222-4333-8444-555566667777/events.jsonl",
    );
    assert_eq!(records.len(), 2);
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());

    assert_eq!(dto.input_tokens, 21583 + 244120);
    assert_eq!(dto.output_tokens, 1064 + 2383);
    assert_eq!(dto.cache_read_tokens, 21187 + 202112);
    assert_eq!(dto.cache_creation_tokens, 0);
    assert_eq!(dto.reasoning_tokens, 0);
    assert_eq!(dto.session_count, 1);
    assert_eq!(
        dto.total_tokens,
        records
            .iter()
            .map(|record| record.total_tokens)
            .sum::<i64>()
    );
    assert_eq!(
        dto.total_tokens,
        (21583 + 1064 + 21187) + records[1].total_tokens
    );
}

#[test]
fn overview_from_factory_fixture_uses_session_token_usage() {
    let records = factory::parse_factory_settings(
        &fixture("factory.settings.json"),
        "/Users/zhangyanhua/.factory/sessions/-Users-zhangyanhua-AI-cli/9ab2ca7b-bd30-495b-9434-07892ee0e5e6.settings.json",
    );
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&stored, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 20234);
    assert_eq!(dto.input_tokens, 3);
    assert_eq!(dto.output_tokens, 1022);
    assert_eq!(dto.cache_read_tokens, 11084);
    assert_eq!(dto.cache_creation_tokens, 8125);
    assert_eq!(dto.reasoning_tokens, 0);
    assert_eq!(dto.session_count, 1);
}

#[test]
fn overview_sums_seeded_sqlite_records() {
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &seed_records()).unwrap();
    let records = store::load_all(&conn).unwrap();
    let dto = aggregate::overview(&records, &Filter::default(), &PriceTable::default());
    assert_eq!(dto.total_tokens, 450);
    assert_eq!(dto.input_tokens, 450);
    assert_eq!(dto.session_count, 3);
}

#[test]
fn filters_restrict_overview_to_matching_subset() {
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &seed_records()).unwrap();
    let records = store::load_all(&conn).unwrap();
    let prices = PriceTable::default();

    let all = aggregate::overview(&records, &Filter::default(), &prices);
    assert_eq!(all.total_tokens, 450);
    assert_eq!(all.session_count, 3);

    let from_aug2 = Filter {
        from: Some("2026-08-02T00:00:00Z".into()),
        ..Filter::default()
    };
    let dto = aggregate::overview(&records, &from_aug2, &prices);
    assert_eq!(dto.total_tokens, 350);
    assert_eq!(dto.session_count, 2);

    let until = Filter {
        to: Some("2026-08-02T00:00:00Z".into()),
        ..Filter::default()
    };
    assert_eq!(
        aggregate::overview(&records, &until, &prices).total_tokens,
        100
    );

    let by_source = Filter {
        sources: vec!["codex".into()],
        ..Filter::default()
    };
    assert_eq!(
        aggregate::overview(&records, &by_source, &prices).total_tokens,
        100
    );

    let by_model = Filter {
        models: vec!["gpt-5.5".into()],
        ..Filter::default()
    };
    assert_eq!(
        aggregate::overview(&records, &by_model, &prices).total_tokens,
        50
    );

    let by_project = Filter {
        projects: vec!["/proj/a".into()],
        ..Filter::default()
    };
    assert_eq!(
        aggregate::overview(&records, &by_project, &prices).total_tokens,
        400
    );

    let intersect = Filter {
        from: Some("2026-08-02T00:00:00Z".into()),
        projects: vec!["/proj/a".into()],
        ..Filter::default()
    };
    assert_eq!(
        aggregate::overview(&records, &intersect, &prices).total_tokens,
        300
    );
}

#[test]
fn filters_apply_across_trend_breakdown_and_sessions() {
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &seed_records()).unwrap();
    let records = store::load_all(&conn).unwrap();
    let prices = PriceTable::default();
    let by_project = Filter {
        projects: vec!["/proj/a".into()],
        ..Filter::default()
    };

    let days = aggregate::trend(&records, &by_project, &prices, "day");
    assert_eq!(days.len(), 2);
    assert_eq!(days[0].bucket, "2026-08-01");
    assert_eq!(days[0].total_tokens, 100);
    assert_eq!(days[1].bucket, "2026-08-02");
    assert_eq!(days[1].total_tokens, 300);

    let by_source = aggregate::by_name(&records, &by_project, &prices, |r| {
        r.source.as_str().to_string()
    });
    assert_eq!(by_source.len(), 2);
    assert_eq!(by_source[0].name, "claude");
    assert_eq!(by_source[0].total_tokens, 300);
    assert_eq!(by_source[1].name, "codex");
    assert_eq!(by_source[1].total_tokens, 100);

    let top = aggregate::top_sessions(&records, &by_project, &prices, 10);
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].session_id, "s2");
    assert_eq!(top[1].session_id, "s1");
}

#[test]
fn filter_options_list_distinct_sources_models_projects() {
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &seed_records()).unwrap();
    let records = store::load_all(&conn).unwrap();
    let options = aggregate::filter_options(&records);
    assert_eq!(options.sources, vec!["claude", "codex", "pi"]);
    assert_eq!(
        options.models,
        vec!["claude-sonnet-5", "gpt-5.1-codex", "gpt-5.5"]
    );
    assert_eq!(options.projects, vec!["/proj/a", "/proj/b"]);
    assert_eq!(options.providers, vec!["anthropic", "official", "subapi"]);
}

#[test]
fn trend_buckets_by_day_and_week() {
    let mut records = seed_records();
    records.push(rec(
        "2026-08-01T11:00:00Z",
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj/a",
        "s1",
        20,
    ));
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let prices = PriceTable::default();

    let days = aggregate::trend(&stored, &Filter::default(), &prices, "day");
    assert_eq!(days.len(), 3);
    assert_eq!(days[0].bucket, "2026-08-01");
    assert_eq!(days[0].total_tokens, 120);
    assert_eq!(days[0].input_tokens, 120);
    assert_eq!(days[0].output_tokens, 0);
    assert_eq!(days[1].bucket, "2026-08-02");
    assert_eq!(days[1].total_tokens, 300);
    assert_eq!(days[2].bucket, "2026-08-08");
    assert_eq!(days[2].total_tokens, 50);

    let months = aggregate::trend(&stored, &Filter::default(), &prices, "month");
    assert_eq!(months.len(), 1);
    assert_eq!(months[0].bucket, "2026-08");
    assert_eq!(months[0].total_tokens, 470);

    let hours = aggregate::trend(&stored, &Filter::default(), &prices, "hour");
    assert!(hours.iter().any(|point| point.bucket == "2026-08-01T11"));
    assert_eq!(
        hours
            .iter()
            .filter(|point| point.bucket == "2026-08-01T11")
            .map(|point| point.total_tokens)
            .sum::<i64>(),
        20
    );

    let weeks = aggregate::trend(&stored, &Filter::default(), &prices, "week");
    assert_eq!(weeks.len(), 2);
    assert_eq!(weeks[0].bucket, "2026-W31");
    assert_eq!(weeks[0].total_tokens, 420);
    assert_eq!(weeks[1].bucket, "2026-W32");
    assert_eq!(weeks[1].total_tokens, 50);

    let from_aug2 = Filter {
        from: Some("2026-08-02T00:00:00Z".into()),
        ..Filter::default()
    };
    let filtered_days = aggregate::trend(&stored, &from_aug2, &prices, "day");
    assert_eq!(filtered_days.len(), 2);
    assert_eq!(filtered_days[0].bucket, "2026-08-02");
    assert_eq!(filtered_days[0].total_tokens, 300);
    let filtered_weeks = aggregate::trend(&stored, &from_aug2, &prices, "week");
    assert_eq!(filtered_weeks.len(), 2);
    assert_eq!(filtered_weeks[0].bucket, "2026-W31");
    assert_eq!(filtered_weeks[0].total_tokens, 300);
    assert_eq!(filtered_weeks[1].bucket, "2026-W32");
    assert_eq!(filtered_weeks[1].total_tokens, 50);
}

#[test]
fn breakdowns_rank_source_model_provider_and_project() {
    let records = seed_records();
    let by_source = aggregate::by_name(&records, &Filter::default(), &PriceTable::default(), |r| {
        r.source.as_str().to_string()
    });
    assert_eq!(by_source[0].name, "claude");
    assert_eq!(by_source[0].total_tokens, 300);
    assert!((by_source[0].share - 300.0 / 450.0).abs() < 1e-9);

    let by_model = aggregate::by_name(&records, &Filter::default(), &PriceTable::default(), |r| {
        r.model.clone()
    });
    assert_eq!(by_model[0].name, "claude-sonnet-5");

    let by_provider =
        aggregate::by_name(&records, &Filter::default(), &PriceTable::default(), |r| {
            r.provider.clone()
        });
    assert_eq!(by_provider[0].name, "anthropic");

    let by_project =
        aggregate::by_name(&records, &Filter::default(), &PriceTable::default(), |r| {
            r.project.clone()
        });
    assert_eq!(by_project[0].name, "/proj/a");
    assert_eq!(by_project[0].total_tokens, 400);
}

#[test]
fn breakdown_by_source_ranks_share_and_follows_filter() {
    let mut records = seed_records();
    records.push(rec(
        "2026-08-01T11:00:00Z",
        Source::Claude,
        "claude-sonnet-5",
        "anthropic",
        "/proj/a",
        "s2",
        50,
    ));
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let prices = PriceTable::default();

    let rows = aggregate::by_name(&stored, &Filter::default(), &prices, |r| {
        r.source.as_str().to_string()
    });
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].name, "claude");
    assert_eq!(rows[0].total_tokens, 350);
    assert!((rows[0].share - 350.0 / 500.0).abs() < 1e-9);
    assert_eq!(rows[1].name, "codex");
    assert_eq!(rows[1].total_tokens, 100);
    assert!((rows[1].share - 100.0 / 500.0).abs() < 1e-9);
    assert_eq!(rows[2].name, "pi");
    assert_eq!(rows[2].total_tokens, 50);
    assert!((rows[2].share - 50.0 / 500.0).abs() < 1e-9);

    let from_aug2 = Filter {
        from: Some("2026-08-02T00:00:00Z".into()),
        ..Filter::default()
    };
    let filtered = aggregate::by_name(&stored, &from_aug2, &prices, |r| {
        r.source.as_str().to_string()
    });
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].name, "claude");
    assert_eq!(filtered[0].total_tokens, 300);
    assert!((filtered[0].share - 300.0 / 350.0).abs() < 1e-9);
    assert_eq!(filtered[1].name, "pi");
    assert_eq!(filtered[1].total_tokens, 50);
    assert!((filtered[1].share - 50.0 / 350.0).abs() < 1e-9);
}

#[test]
fn breakdown_by_model_ranks_across_sources_and_follows_filter() {
    let mut records = seed_records();
    records.push(rec(
        "2026-08-01T11:00:00Z",
        Source::Codex,
        "gpt-5.5",
        "official",
        "/proj/a",
        "s1",
        80,
    ));
    records.push(rec(
        "2026-08-08T12:00:00Z",
        Source::Factory,
        "",
        "anthropic",
        "/proj/b",
        "s4",
        20,
    ));
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let prices = PriceTable::default();

    let rows = aggregate::by_name(&stored, &Filter::default(), &prices, |r| r.model.clone());
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].name, "claude-sonnet-5");
    assert_eq!(rows[0].total_tokens, 300);
    assert!((rows[0].share - 300.0 / 550.0).abs() < 1e-9);
    assert_eq!(rows[1].name, "gpt-5.5");
    assert_eq!(rows[1].total_tokens, 130);
    assert!((rows[1].share - 130.0 / 550.0).abs() < 1e-9);
    assert_eq!(rows[2].name, "gpt-5.1-codex");
    assert_eq!(rows[2].total_tokens, 100);
    assert_eq!(rows[3].name, "（未标注）");
    assert_eq!(rows[3].total_tokens, 20);

    let from_aug2 = Filter {
        from: Some("2026-08-02T00:00:00Z".into()),
        ..Filter::default()
    };
    let filtered = aggregate::by_name(&stored, &from_aug2, &prices, |r| r.model.clone());
    assert_eq!(filtered.len(), 3);
    assert_eq!(filtered[0].name, "claude-sonnet-5");
    assert_eq!(filtered[0].total_tokens, 300);
    assert!((filtered[0].share - 300.0 / 370.0).abs() < 1e-9);
    assert_eq!(filtered[1].name, "gpt-5.5");
    assert_eq!(filtered[1].total_tokens, 50);
    assert!((filtered[1].share - 50.0 / 370.0).abs() < 1e-9);
    assert_eq!(filtered[2].name, "（未标注）");
    assert_eq!(filtered[2].total_tokens, 20);
}

#[test]
fn breakdown_by_provider_ranks_and_follows_filter() {
    let mut records = seed_records();
    records.push(rec(
        "2026-08-01T11:00:00Z",
        Source::Factory,
        "claude-sonnet-5",
        "anthropic",
        "/proj/a",
        "s4",
        40,
    ));
    records.push(rec(
        "2026-08-08T12:00:00Z",
        Source::Pi,
        "gpt-5.5",
        "siliconflow",
        "/proj/b",
        "s3",
        70,
    ));
    records.push(rec(
        "2026-08-08T13:00:00Z",
        Source::Kimi,
        "",
        "",
        "/proj/b",
        "s5",
        20,
    ));
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let prices = PriceTable::default();

    let rows = aggregate::by_name(&stored, &Filter::default(), &prices, |r| r.provider.clone());
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].name, "anthropic");
    assert_eq!(rows[0].total_tokens, 340);
    assert!((rows[0].share - 340.0 / 580.0).abs() < 1e-9);
    assert_eq!(rows[1].name, "official");
    assert_eq!(rows[1].total_tokens, 100);
    assert_eq!(rows[2].name, "siliconflow");
    assert_eq!(rows[2].total_tokens, 70);
    assert_eq!(rows[3].name, "subapi");
    assert_eq!(rows[3].total_tokens, 50);
    assert_eq!(rows[4].name, "（未标注）");
    assert_eq!(rows[4].total_tokens, 20);

    let from_aug2 = Filter {
        from: Some("2026-08-02T00:00:00Z".into()),
        ..Filter::default()
    };
    let filtered = aggregate::by_name(&stored, &from_aug2, &prices, |r| r.provider.clone());
    assert_eq!(filtered.len(), 4);
    assert_eq!(filtered[0].name, "anthropic");
    assert_eq!(filtered[0].total_tokens, 300);
    assert!((filtered[0].share - 300.0 / 440.0).abs() < 1e-9);
    assert_eq!(filtered[1].name, "siliconflow");
    assert_eq!(filtered[1].total_tokens, 70);
    assert_eq!(filtered[2].name, "subapi");
    assert_eq!(filtered[2].total_tokens, 50);
    assert_eq!(filtered[3].name, "（未标注）");
    assert_eq!(filtered[3].total_tokens, 20);

    let by_official = Filter {
        providers: vec!["official".into()],
        ..Filter::default()
    };
    let official_only = aggregate::by_name(&stored, &by_official, &prices, |r| r.provider.clone());
    assert_eq!(official_only.len(), 1);
    assert_eq!(official_only[0].name, "official");
    assert_eq!(official_only[0].total_tokens, 100);
}

#[test]
fn breakdown_by_project_ranks_and_follows_filter() {
    let mut records = seed_records();
    records.push(rec(
        "2026-08-01T11:00:00Z",
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj/c",
        "s6",
        80,
    ));
    records.push(rec(
        "2026-08-08T12:00:00Z",
        Source::Factory,
        "",
        "anthropic",
        "",
        "s7",
        20,
    ));
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let prices = PriceTable::default();

    let rows = aggregate::by_name(&stored, &Filter::default(), &prices, |r| r.project.clone());
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].name, "/proj/a");
    assert_eq!(rows[0].total_tokens, 400);
    assert!((rows[0].share - 400.0 / 550.0).abs() < 1e-9);
    assert_eq!(rows[1].name, "/proj/c");
    assert_eq!(rows[1].total_tokens, 80);
    assert_eq!(rows[2].name, "/proj/b");
    assert_eq!(rows[2].total_tokens, 50);
    assert_eq!(rows[3].name, "（未标注）");
    assert_eq!(rows[3].total_tokens, 20);

    let from_aug2 = Filter {
        from: Some("2026-08-02T00:00:00Z".into()),
        ..Filter::default()
    };
    let filtered = aggregate::by_name(&stored, &from_aug2, &prices, |r| r.project.clone());
    assert_eq!(filtered.len(), 3);
    assert_eq!(filtered[0].name, "/proj/a");
    assert_eq!(filtered[0].total_tokens, 300);
    assert!((filtered[0].share - 300.0 / 370.0).abs() < 1e-9);
    assert_eq!(filtered[1].name, "/proj/b");
    assert_eq!(filtered[1].total_tokens, 50);
    assert_eq!(filtered[2].name, "（未标注）");
    assert_eq!(filtered[2].total_tokens, 20);
}

#[test]
fn top_sessions_and_turns_preserve_source_file() {
    let mut records = seed_records();
    records.push(rec(
        "2026-08-01T11:00:00Z",
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj/a",
        "s1",
        20,
    ));
    records.push(rec(
        "2026-08-01T12:00:00Z",
        Source::Claude,
        "claude-sonnet-5",
        "anthropic",
        "/proj/a",
        "s1",
        99,
    ));
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let prices = PriceTable::default();

    let top = aggregate::top_sessions(&stored, &Filter::default(), &prices, 2);
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].session_id, "s2");
    assert_eq!(top[0].source, "claude");
    assert_eq!(top[0].project, "/proj/a");
    assert_eq!(top[0].total_tokens, 300);
    assert_eq!(top[0].started_at, "2026-08-02T10:00:00Z");
    assert_eq!(top[0].ended_at, "2026-08-02T10:00:00Z");
    assert_eq!(top[0].source_file, "/s2.jsonl");
    assert_eq!(top[1].session_id, "s1");
    assert_eq!(top[1].source, "codex");
    assert_eq!(top[1].total_tokens, 120);
    assert_eq!(top[1].started_at, "2026-08-01T10:00:00Z");
    assert_eq!(top[1].ended_at, "2026-08-01T11:00:00Z");
    assert_eq!(top[1].source_file, "/s1.jsonl");

    let all = aggregate::top_sessions(&stored, &Filter::default(), &prices, 10);
    assert_eq!(all.len(), 4);
    assert_eq!(all[2].session_id, "s1");
    assert_eq!(all[2].source, "claude");
    assert_eq!(all[2].total_tokens, 99);

    let from_aug2 = Filter {
        from: Some("2026-08-02T00:00:00Z".into()),
        ..Filter::default()
    };
    let filtered_top = aggregate::top_sessions(&stored, &from_aug2, &prices, 10);
    assert_eq!(filtered_top.len(), 2);
    assert_eq!(filtered_top[0].session_id, "s2");
    assert_eq!(filtered_top[0].total_tokens, 300);
    assert_eq!(filtered_top[1].session_id, "s3");
    assert_eq!(filtered_top[1].total_tokens, 50);

    let turns = aggregate::session_turns(&stored, "s1", Some("codex"), &Filter::default(), &prices);
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].occurred_at, "2026-08-01T10:00:00Z");
    assert_eq!(turns[0].model, "gpt-5.1-codex");
    assert_eq!(turns[0].total_tokens, 100);
    assert_eq!(turns[0].source_file, "/s1.jsonl");
    assert_eq!(turns[1].occurred_at, "2026-08-01T11:00:00Z");
    assert_eq!(turns[1].total_tokens, 20);

    let same_id_all_sources =
        aggregate::session_turns(&stored, "s1", None, &Filter::default(), &prices);
    assert_eq!(same_id_all_sources.len(), 3);
    let same_id_other_source =
        aggregate::session_turns(&stored, "s1", Some("codex"), &Filter::default(), &prices);
    assert_eq!(same_id_other_source.len(), 2);

    let recent = Filter {
        from: Some("2026-08-01T10:30:00Z".into()),
        ..Filter::default()
    };
    let filtered = aggregate::session_turns(&stored, "s1", Some("codex"), &recent, &prices);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].total_tokens, 20);
    assert_eq!(filtered[0].source_file, "/s1.jsonl");
}

#[test]
fn sessions_page_supports_search_sort_and_pagination() {
    let mut records = seed_records();
    records.push(rec(
        "2026-08-01T11:00:00Z",
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj/c",
        "s6",
        80,
    ));
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let prices = PriceTable::default();

    // 默认排序：按 total_tokens 降序，分页返回第一页。
    let page1 = query::sessions_page(
        &conn,
        &prices,
        &SessionQuery {
            page: Some(1),
            page_size: Some(2),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page1.total, 4);
    assert_eq!(page1.rows.len(), 2);
    assert_eq!(page1.rows[0].session_id, "s2");
    assert_eq!(page1.rows[0].total_tokens, 300);
    assert_eq!(page1.rows[1].session_id, "s1");
    assert_eq!(page1.total_tokens, 300 + 100 + 80 + 50);
    assert!(page1.rows[0].cost.is_none());
    assert!(!page1.rows[0].unpriced);

    let page2 = query::sessions_page(
        &conn,
        &prices,
        &SessionQuery {
            page: Some(2),
            page_size: Some(2),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page2.rows.len(), 2);
    assert_eq!(page2.rows[0].session_id, "s6");
    assert_eq!(page2.rows[1].session_id, "s3");

    // 超出页码时仍返回汇总，避免 KPI 被清空。
    let empty_page = query::sessions_page(
        &conn,
        &prices,
        &SessionQuery {
            page: Some(99),
            page_size: Some(10),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(empty_page.total, 4);
    assert_eq!(empty_page.total_tokens, 300 + 100 + 80 + 50);
    assert!(empty_page.rows.is_empty());
    assert!(empty_page.last_ended.is_some());

    // 升序排序按 session_id。
    let asc_by_session = query::sessions_page(
        &conn,
        &prices,
        &SessionQuery {
            sort_by: Some("session".into()),
            sort_dir: Some("asc".into()),
            page: Some(1),
            page_size: Some(10),
            ..Default::default()
        },
    )
    .unwrap();
    let ids: Vec<&str> = asc_by_session
        .rows
        .iter()
        .map(|r| r.session_id.as_str())
        .collect();
    assert_eq!(ids, vec!["s1", "s2", "s3", "s6"]);

    // 搜索：只命中项目名包含 "proj/c" 的会话。
    let searched = query::sessions_page(
        &conn,
        &prices,
        &SessionQuery {
            search: Some("proj/c".into()),
            page: Some(1),
            page_size: Some(10),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(searched.total, 1);
    assert_eq!(searched.rows[0].session_id, "s6");

    // 搜索无匹配时返回空结果而非报错。
    let no_match = query::sessions_page(
        &conn,
        &prices,
        &SessionQuery {
            search: Some("不存在的关键字".into()),
            page: Some(1),
            page_size: Some(10),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(no_match.total, 0);
    assert!(no_match.rows.is_empty());
    assert_eq!(no_match.last_ended, None);
}

#[test]
fn sessions_page_computes_cost_only_when_requested() {
    let mut priced = rec(
        "2026-08-01T10:00:00Z",
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj/a",
        "s1",
        0,
    );
    priced.input_tokens = 1000;
    priced.total_tokens = 1000;
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &[priced]).unwrap();
    let table = PriceTable {
        prices: vec![PriceEntry {
            model: "gpt-5.1-codex".into(),
            provider: Some("official".into()),
            input: 0.001,
            output: 0.0,
            cache_read: 0.0,
            cache_creation: 0.0,
            origin: PriceOrigin::User,
        }],
    };

    let listed = query::sessions_page(&conn, &table, &SessionQuery::default()).unwrap();
    assert_eq!(listed.rows.len(), 1);
    assert_eq!(listed.rows[0].cost, None);
    assert!(!listed.rows[0].unpriced);

    let exported = query::sessions_page(
        &conn,
        &table,
        &SessionQuery {
            include_cost: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(exported.rows[0].cost, Some(1.0));
    assert!(!exported.rows[0].unpriced);
}

#[test]
fn cost_prefers_native_and_marks_unpriced() {
    let priced = UsageRecord {
        native_cost: None,
        ..rec(
            "2026-08-01T10:00:00Z",
            Source::Codex,
            "gpt-5.1-codex",
            "official",
            "/proj/a",
            "s1",
            0,
        )
    };
    let mut priced = priced;
    priced.input_tokens = 1000;
    priced.output_tokens = 500;
    priced.cache_read_tokens = 200;
    priced.cache_creation_tokens = 100;
    let table = PriceTable {
        prices: vec![PriceEntry {
            model: "gpt-5.1-codex".into(),
            provider: Some("official".into()),
            input: 0.001,
            output: 0.002,
            cache_read: 0.0005,
            cache_creation: 0.003,
            origin: PriceOrigin::User,
        }],
    };
    let derived = derive_cost(&priced, &table);
    assert_eq!(derived.amount, Some(1.0 + 1.0 + 0.1 + 0.3));
    assert!(!derived.unpriced);

    let native = UsageRecord {
        native_cost: Some(9.9),
        ..priced.clone()
    };
    let derived = derive_cost(&native, &table);
    assert_eq!(derived.amount, Some(9.9));
    assert!(derived.source_native);

    let missing = rec(
        "2026-08-01T10:00:00Z",
        Source::Claude,
        "claude-sonnet-5",
        "anthropic",
        "/proj/a",
        "s2",
        10,
    );
    let derived = derive_cost(&missing, &table);
    assert_eq!(derived.amount, None);
    assert!(derived.unpriced);

    priced.reasoning_tokens = 999;
    let derived = derive_cost(&priced, &table);
    assert_eq!(derived.amount, Some(2.4));

    let mut by_provider = rec(
        "2026-08-01T10:00:00Z",
        Source::Pi,
        "gpt-5.5",
        "subapi",
        "/proj/b",
        "s3",
        0,
    );
    by_provider.input_tokens = 100;
    let mixed = PriceTable {
        prices: vec![
            PriceEntry {
                model: "gpt-5.5".into(),
                provider: Some("subapi".into()),
                input: 0.02,
                output: 0.0,
                cache_read: 0.0,
                cache_creation: 0.0,
                origin: PriceOrigin::User,
            },
            PriceEntry {
                model: "gpt-5.5".into(),
                provider: None,
                input: 0.01,
                output: 0.0,
                cache_read: 0.0,
                cache_creation: 0.0,
                origin: PriceOrigin::User,
            },
        ],
    };
    assert_eq!(derive_cost(&by_provider, &mixed).amount, Some(2.0));
    by_provider.provider = "siliconflow".into();
    assert_eq!(derive_cost(&by_provider, &mixed).amount, Some(1.0));
    by_provider.model = "unknown-model".into();
    let unknown = derive_cost(&by_provider, &mixed);
    assert_eq!(unknown.amount, None);
    assert!(unknown.unpriced);
}

#[test]
fn cost_matches_model_and_provider_case_insensitively() {
    // 来源上报或用户价目表里的大小写不一致（如 "GPT-4o" vs "gpt-4o"）时仍应命中同一模型单价。
    let mut record = rec(
        "2026-08-01T10:00:00Z",
        Source::Codex,
        "GPT-4o",
        "OpenAI",
        "/proj/a",
        "s1",
        0,
    );
    record.input_tokens = 100;
    let table = PriceTable {
        prices: vec![PriceEntry {
            model: "gpt-4o".into(),
            provider: Some("openai".into()),
            input: 0.01,
            output: 0.0,
            cache_read: 0.0,
            cache_creation: 0.0,
            origin: PriceOrigin::User,
        }],
    };
    let derived = derive_cost(&record, &table);
    assert_eq!(derived.amount, Some(1.0));
    assert!(!derived.unpriced);

    // provider 兜底档（价目表条目 provider 为 None）同样大小写不敏感。
    let table_bare = PriceTable {
        prices: vec![PriceEntry {
            model: "gpt-4o".into(),
            provider: None,
            input: 0.02,
            output: 0.0,
            cache_read: 0.0,
            cache_creation: 0.0,
            origin: PriceOrigin::User,
        }],
    };
    let derived_bare = derive_cost(&record, &table_bare);
    assert_eq!(derived_bare.amount, Some(2.0));
}

#[test]
fn overview_and_turns_use_price_table_and_flag_unpriced() {
    let mut priced = rec(
        "2026-08-01T10:00:00Z",
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj/a",
        "s1",
        0,
    );
    priced.input_tokens = 1000;
    priced.total_tokens = 1000;
    let unpriced = rec(
        "2026-08-02T10:00:00Z",
        Source::Claude,
        "claude-sonnet-5",
        "anthropic",
        "/proj/a",
        "s2",
        10,
    );
    let mut native = rec(
        "2026-08-08T10:00:00Z",
        Source::Pi,
        "gpt-5.5",
        "subapi",
        "/proj/b",
        "s3",
        50,
    );
    native.native_cost = Some(0.5);
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &[priced, unpriced, native]).unwrap();
    let stored = store::load_all(&conn).unwrap();
    let table = PriceTable {
        prices: vec![PriceEntry {
            model: "gpt-5.1-codex".into(),
            provider: Some("official".into()),
            input: 0.001,
            output: 0.0,
            cache_read: 0.0,
            cache_creation: 0.0,
            origin: PriceOrigin::User,
        }],
    };

    let dto = aggregate::overview(&stored, &Filter::default(), &table);
    assert_eq!(dto.cost, Some(1.5));
    assert!(dto.unpriced);

    let priced_turns =
        aggregate::session_turns(&stored, "s1", Some("codex"), &Filter::default(), &table);
    assert_eq!(priced_turns[0].cost, Some(1.0));
    assert_eq!(priced_turns[0].cost_source, CostSource::User);
    assert_eq!(priced_turns[0].cost_note.as_deref(), Some("用户单价"));
    let unpriced_turns =
        aggregate::session_turns(&stored, "s2", Some("claude"), &Filter::default(), &table);
    assert_eq!(unpriced_turns[0].cost, None);
    assert!(unpriced_turns[0].unpriced);
    assert_eq!(unpriced_turns[0].cost_source, CostSource::None);
    assert_eq!(unpriced_turns[0].cost_note.as_deref(), Some("单价未配置"));
    let native_turns =
        aggregate::session_turns(&stored, "s3", Some("pi"), &Filter::default(), &table);
    assert_eq!(native_turns[0].cost, Some(0.5));
    assert_eq!(native_turns[0].cost_source, CostSource::Native);
    assert_eq!(native_turns[0].cost_note.as_deref(), Some("来源自带"));

    let by_source = aggregate::by_name(&stored, &Filter::default(), &table, |r| {
        r.source.as_str().to_string()
    });
    assert_eq!(by_source[0].name, "codex");
    assert_eq!(by_source[0].cost, Some(1.0));
    assert!(!by_source[0].unpriced);
    assert_eq!(by_source[1].name, "pi");
    assert_eq!(by_source[1].cost, Some(0.5));
    assert_eq!(by_source[2].name, "claude");
    assert_eq!(by_source[2].cost, None);
    assert!(by_source[2].unpriced);
}

#[test]
fn opening_legacy_cache_adds_trusted_ingest_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite");
    let legacy = rusqlite::Connection::open(&path).unwrap();
    legacy
        .execute_batch(
            r#"
            CREATE TABLE usage_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                occurred_at TEXT NOT NULL,
                source TEXT NOT NULL,
                model TEXT NOT NULL,
                provider TEXT NOT NULL,
                project TEXT NOT NULL,
                session_id TEXT NOT NULL,
                source_file TEXT NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                cache_read_tokens INTEGER NOT NULL,
                cache_creation_tokens INTEGER NOT NULL,
                reasoning_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                native_cost REAL
            );
            CREATE TABLE ingested_files (
                path TEXT PRIMARY KEY,
                mtime_ms INTEGER NOT NULL,
                size INTEGER NOT NULL
            );
            INSERT INTO usage_records (
                occurred_at, source, model, provider, project, session_id, source_file,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                reasoning_tokens, total_tokens, native_cost
            ) VALUES ('2026-01-01T00:00:00Z', 'codex', '', '', '', 's', '/one.jsonl', 1, 0, 0, 0, 0, 1, NULL);
            INSERT INTO ingested_files(path, mtime_ms, size) VALUES('/one.jsonl', 1, 1);
            "#,
        )
        .unwrap();
    drop(legacy);

    let conn = store::open_db(path.to_string_lossy().as_ref()).unwrap();
    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(ingested_files)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let source: String = conn
        .query_row(
            "SELECT source FROM ingested_files WHERE path = '/one.jsonl'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(columns.contains(&"fingerprint".to_string()));
    assert!(columns.contains(&"adapter_version".to_string()));
    assert_eq!(source, "codex");
}

#[test]
fn usage_records_source_file_operations_use_an_index() {
    let conn = store::open_memory().unwrap();
    for sql in [
        "EXPLAIN QUERY PLAN SELECT COUNT(*) FROM usage_records WHERE source_file = ?1",
        "EXPLAIN QUERY PLAN DELETE FROM usage_records WHERE source_file = ?1",
    ] {
        let plan: Vec<String> = conn
            .prepare(sql)
            .unwrap()
            .query_map(["/one.jsonl"], |row| row.get(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(
            plan.iter().any(|detail| {
                detail.contains("USING")
                    && detail.contains("INDEX")
                    && detail.contains("source_file")
            }),
            "source_file operation must use an index, query plan: {plan:?}"
        );
    }
}

#[test]
fn reconcile_source_lookup_uses_an_index() {
    let conn = store::open_memory().unwrap();
    let plan: Vec<String> = conn
        .prepare("EXPLAIN QUERY PLAN SELECT path FROM ingested_files WHERE source = ?1")
        .unwrap()
        .query_map(["codex"], |row| row.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        plan.iter()
            .any(|detail| detail.contains("USING") && detail.contains("INDEX")),
        "ingested_files(source) lookup must use an index, query plan: {plan:?}"
    );
}

#[test]
fn source_and_occurred_at_filter_uses_composite_index() {
    let conn = store::open_memory().unwrap();
    let plan: Vec<String> = conn
        .prepare(
            "EXPLAIN QUERY PLAN SELECT COUNT(*) FROM usage_records \
             WHERE source = ?1 AND occurred_at >= ?2",
        )
        .unwrap()
        .query_map(["codex", "2026-01-01"], |row| row.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        plan.iter().any(|detail| {
            detail.contains("USING") && detail.contains("INDEX") && detail.contains("source")
        }),
        "combined source+occurred_at filter must use an index, query plan: {plan:?}"
    );
}

#[test]
fn open_db_enables_wal_and_normal_synchronous() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("usage.sqlite");
    let conn = store::open_db(path.to_str().unwrap()).unwrap();
    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode.to_lowercase(), "wal");
    let synchronous: i64 = conn
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .unwrap();
    assert_eq!(synchronous, 1, "synchronous should be NORMAL (1)");
}

#[test]
fn ingest_skips_unchanged_file_on_second_pass() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let session_dir = home.join(".codex/sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(session_dir.join("one.jsonl"), fixture("codex.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    let first = ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(first.files_parsed, 1);
    assert_eq!(first.files_skipped, 0);
    assert_eq!(first.records_written, 2);
    let second = ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(second.files_parsed, 0);
    assert_eq!(second.files_skipped, 1);
    assert_eq!(second.records_written, 0);
    let records = store::load_all(&conn).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records.iter().map(|r| r.total_tokens).sum::<i64>(), 19113);
}

#[test]
fn ingest_rewrites_changed_file_without_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let session_dir = home.join(".codex/sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    let path = session_dir.join("one.jsonl");
    std::fs::write(&path, fixture("codex.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();

    let mut changed = fixture("codex.jsonl");
    changed.push('\n');
    std::fs::write(&path, changed).unwrap();
    let second = ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(second.files_parsed, 1);
    assert_eq!(second.files_skipped, 0);
    assert_eq!(second.records_written, 2);

    let records = store::load_all(&conn).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records.iter().map(|r| r.total_tokens).sum::<i64>(), 19113);
}

#[test]
fn ingest_keeps_last_good_records_when_changed_jsonl_has_a_bad_line() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let session_dir = home.join(".codex/sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    let path = session_dir.join("one.jsonl");
    std::fs::write(&path, fixture("codex.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();

    let broken = format!("{}\n{{not-json", fixture("codex.jsonl"));
    std::fs::write(&path, broken).unwrap();
    let report = ingest::ingest_all(&conn, home).unwrap();

    assert_eq!(report.files_failed, 1);
    assert_eq!(report.files_parsed, 0);
    assert!(report.partial_success);
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].source, "codex");
    let records = store::load_all(&conn).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records.iter().map(|r| r.total_tokens).sum::<i64>(), 19113);
}

#[test]
fn ingest_keeps_last_good_records_when_valid_jsonl_loses_usage_events() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let session_dir = home.join(".codex/sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    let path = session_dir.join("one.jsonl");
    let original = fixture("codex.jsonl");
    std::fs::write(&path, &original).unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();

    let partial = original.lines().take(4).collect::<Vec<_>>().join("\n");
    std::fs::write(&path, partial).unwrap();
    let report = ingest::ingest_all(&conn, home).unwrap();

    assert_eq!(report.files_failed, 1);
    let records = store::load_all(&conn).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(
        records
            .iter()
            .map(|record| record.total_tokens)
            .sum::<i64>(),
        19113
    );
}

#[test]
fn ingest_keeps_last_good_records_when_changed_file_has_no_usage_records() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let session_dir = home.join(".codex/sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    let path = session_dir.join("one.jsonl");
    std::fs::write(&path, fixture("codex.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();

    std::fs::write(&path, "{}\n").unwrap();
    let report = ingest::ingest_all(&conn, home).unwrap();

    assert_eq!(report.files_failed, 1);
    assert_eq!(store::load_all(&conn).unwrap().len(), 2);
}

#[test]
fn source_with_a_failed_file_defers_deleted_file_reconciliation() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let session_dir = home.join(".codex/sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    let first = session_dir.join("one.jsonl");
    let second = session_dir.join("two.jsonl");
    std::fs::write(&first, fixture("codex.jsonl")).unwrap();
    std::fs::write(&second, fixture("codex.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(store::load_all(&conn).unwrap().len(), 4);

    std::fs::write(&first, "{not-json").unwrap();
    std::fs::remove_file(second).unwrap();
    let report = ingest::ingest_all(&conn, home).unwrap();

    assert_eq!(report.files_failed, 1);
    assert_eq!(report.records_removed, 0);
    assert_eq!(store::load_all(&conn).unwrap().len(), 4);
}

#[test]
fn ingest_archives_records_after_a_source_file_is_deleted() {
    // ADR 0004：源文件消失（工具自身清理/轮转）不再物理删除历史记录，只归档；
    // 归档记录仍然计入统计，直到用户显式清理。
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let session_dir = home.join(".codex/sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    let path = session_dir.join("one.jsonl");
    std::fs::write(&path, fixture("codex.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();

    std::fs::remove_file(path).unwrap();
    let report = ingest::ingest_all(&conn, home).unwrap();

    assert_eq!(report.records_removed, 0);
    assert_eq!(report.records_archived, 2);
    let records = store::load_all(&conn).unwrap();
    assert_eq!(records.len(), 2, "archived records still count in totals");
    assert_eq!(records.iter().map(|r| r.total_tokens).sum::<i64>(), 19113);

    // 幂等：再摄取一次不会重复归档同一批记录。
    let second = ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(second.records_archived, 0);
    assert_eq!(store::load_all(&conn).unwrap().len(), 2);

    let diagnostics = ingest::source_diagnostics(&conn, home).unwrap();
    let codex = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.source == "codex")
        .unwrap();
    assert_eq!(codex.archived_record_count, 2);
    assert_eq!(codex.record_count, 2);
}

#[test]
fn ingest_replaces_archived_records_when_the_same_path_reappears() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let session_dir = home.join(".codex/sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    let path = session_dir.join("one.jsonl");
    std::fs::write(&path, fixture("codex.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();

    std::fs::remove_file(&path).unwrap();
    ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(store::load_all(&conn).unwrap().len(), 2);

    // 文件在同一路径重新出现（比如从备份恢复），不应和归档快照重复计数。
    std::fs::write(&path, fixture("codex.jsonl")).unwrap();
    let report = ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(report.files_parsed, 1);
    let records = store::load_all(&conn).unwrap();
    assert_eq!(
        records.len(),
        2,
        "reappearing file replaces its archived snapshot"
    );
    assert!(records.iter().all(|r| r.total_tokens > 0));
}

#[test]
fn purge_archived_permanently_deletes_only_archived_records() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let codex_dir = home.join(".codex/sessions");
    let claude_dir = home.join(".claude/projects/project");
    std::fs::create_dir_all(&codex_dir).unwrap();
    std::fs::create_dir_all(&claude_dir).unwrap();
    let codex_path = codex_dir.join("one.jsonl");
    std::fs::write(&codex_path, fixture("codex.jsonl")).unwrap();
    std::fs::write(claude_dir.join("one.jsonl"), fixture("claude.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();

    std::fs::remove_file(&codex_path).unwrap();
    ingest::ingest_all(&conn, home).unwrap();

    // 按来源清理：只删 codex 的归档记录，claude 的活跃记录不受影响。
    let removed = store::purge_archived(&conn, Some(Source::Codex)).unwrap();
    assert_eq!(removed, 2);
    let records = store::load_all(&conn).unwrap();
    assert!(records.iter().all(|r| r.source == Source::Claude));

    let removed_again = store::purge_archived(&conn, Some(Source::Codex)).unwrap();
    assert_eq!(removed_again, 0);

    let removed_all = store::purge_archived(&conn, None).unwrap();
    assert_eq!(removed_all, 0, "claude records were never archived");
}

#[test]
fn kimi_sidecar_change_invalidates_unchanged_session_file() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let session_id = "bd1ab6fc-768d-4cff-b4c4-221a583c3af8";
    let wire = home.join(format!(".kimi/sessions/hash/{session_id}/wire.jsonl"));
    std::fs::create_dir_all(wire.parent().unwrap()).unwrap();
    std::fs::write(&wire, fixture("kimi-wire.jsonl")).unwrap();
    std::fs::write(
        home.join(".kimi/kimi.json"),
        format!(r#"{{"work_dirs":[{{"last_session_id":"{session_id}","path":"/project/one"}}]}}"#),
    )
    .unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();
    assert!(store::load_all(&conn)
        .unwrap()
        .iter()
        .all(|record| record.project == "/project/one"));

    std::fs::write(
        home.join(".kimi/kimi.json"),
        format!(r#"{{"work_dirs":[{{"last_session_id":"{session_id}","path":"/project/two"}}]}}"#),
    )
    .unwrap();
    let report = ingest::ingest_all(&conn, home).unwrap();

    assert_eq!(report.files_parsed, 1);
    assert!(store::load_all(&conn)
        .unwrap()
        .iter()
        .all(|record| record.project == "/project/two"));
}

#[test]
fn invalid_kimi_sidecar_keeps_last_good_project_mapping() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let session_id = "bd1ab6fc-768d-4cff-b4c4-221a583c3af8";
    let wire = home.join(format!(".kimi/sessions/hash/{session_id}/wire.jsonl"));
    let sidecar = home.join(".kimi/kimi.json");
    std::fs::create_dir_all(wire.parent().unwrap()).unwrap();
    std::fs::write(&wire, fixture("kimi-wire.jsonl")).unwrap();
    std::fs::write(
        &sidecar,
        format!(r#"{{"work_dirs":[{{"last_session_id":"{session_id}","path":"/project/good"}}]}}"#),
    )
    .unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();

    std::fs::write(&sidecar, "{not-json").unwrap();
    let report = ingest::ingest_all(&conn, home).unwrap();
    let records = store::load_all(&conn).unwrap();

    assert_eq!(report.files_failed, 1);
    assert!(records
        .iter()
        .all(|record| record.project == "/project/good"));
}

#[test]
fn rebuilding_one_source_keeps_other_sources_and_reparses_target() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let codex_dir = home.join(".codex/sessions");
    let claude_dir = home.join(".claude/projects/project");
    std::fs::create_dir_all(&codex_dir).unwrap();
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(codex_dir.join("one.jsonl"), fixture("codex.jsonl")).unwrap();
    std::fs::write(claude_dir.join("one.jsonl"), fixture("claude.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();

    let report = ingest::rebuild_cache(&conn, home, Some(Source::Codex)).unwrap();
    let records = store::load_all(&conn).unwrap();

    assert_eq!(report.files_parsed, 1);
    assert!(records.iter().any(|record| record.source == Source::Codex));
    assert!(records.iter().any(|record| record.source == Source::Claude));
}

#[test]
fn rebuild_keeps_last_good_records_when_target_file_is_broken() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let codex_dir = home.join(".codex/sessions");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let path = codex_dir.join("one.jsonl");
    std::fs::write(&path, fixture("codex.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();
    std::fs::write(&path, "{not-json").unwrap();

    let report = ingest::rebuild_cache(&conn, home, Some(Source::Codex)).unwrap();
    let records = store::load_all(&conn).unwrap();

    assert_eq!(report.files_failed, 1);
    assert_eq!(records.len(), 2);
    assert_eq!(
        records
            .iter()
            .map(|record| record.total_tokens)
            .sum::<i64>(),
        19113
    );
}

#[test]
fn rebuilding_all_removes_unknown_source_records() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let conn = store::open_memory().unwrap();
    conn.execute_batch(
        r#"
        INSERT INTO usage_records (
            occurred_at, source, model, provider, project, session_id, source_file,
            input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
            reasoning_tokens, total_tokens, native_cost
        ) VALUES ('2026-01-01T00:00:00Z', 'future-source', '', '', '', 's', '/future', 1, 0, 0, 0, 0, 1, NULL);
        INSERT INTO ingested_files(path, mtime_ms, size, source, fingerprint, adapter_version)
        VALUES('/future', 1, 1, 'future-source', '', 1);
        "#,
    )
    .unwrap();
    assert!(store::load_all(&conn).is_err());

    let report = ingest::rebuild_cache(&conn, home, None).unwrap();

    assert_eq!(report.records_removed, 1);
    assert!(store::load_all(&conn).unwrap().is_empty());
}

#[test]
fn remove_unknown_sources_keeps_every_registered_source() {
    let conn = store::open_memory().unwrap();
    for source in Source::ALL {
        conn.execute(
            r#"
            INSERT INTO usage_records (
                occurred_at, source, model, provider, project, session_id, source_file,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                reasoning_tokens, total_tokens, native_cost
            ) VALUES ('2026-01-01T00:00:00Z', ?1, '', '', '', ?1, ?2, 1, 0, 0, 0, 0, 1, NULL)
            "#,
            rusqlite::params![source.as_str(), format!("/{}.jsonl", source.as_str())],
        )
        .unwrap();
        conn.execute(
            r#"
            INSERT INTO ingested_files(path, mtime_ms, size, source, fingerprint, adapter_version)
            VALUES(?1, 1, 1, ?2, '', 1)
            "#,
            rusqlite::params![format!("/{}.jsonl", source.as_str()), source.as_str()],
        )
        .unwrap();
    }

    let removed = store::remove_unknown_sources(&conn).unwrap();
    assert_eq!(removed, 0);

    for source in Source::ALL {
        let (cached_files, record_count, _, _) = store::source_cache_stats(&conn, source).unwrap();
        assert_eq!(
            cached_files,
            1,
            "{} cached files were wiped",
            source.as_str()
        );
        assert_eq!(record_count, 1, "{} records were wiped", source.as_str());
    }
}

#[test]
fn source_diagnostics_explain_detection_cache_and_usage_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let codex_dir = home.join(".codex/sessions");
    std::fs::create_dir_all(&codex_dir).unwrap();
    std::fs::write(codex_dir.join("one.jsonl"), fixture("codex.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, home).unwrap();

    let diagnostics = ingest::source_diagnostics(&conn, home).unwrap();
    let codex = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.source == "codex")
        .unwrap();
    let qwen = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.source == "qwen")
        .unwrap();

    assert!(codex.detected);
    assert_eq!(codex.cached_files, 1);
    assert_eq!(codex.record_count, 2);
    assert_eq!(codex.total_tokens, 19113);
    assert_eq!(codex.coverage, "轮级 Token");
    assert!(!qwen.detected);
    assert_eq!(qwen.coverage, "本地无 Token");
}

#[test]
fn source_scan_dirs_default_to_home_relative_paths() {
    let home = std::path::Path::new("/home/example");
    let overrides = ingest::PathOverrides::new();

    assert_eq!(
        ingest::source_scan_dirs_with(&overrides, home, Source::Codex),
        vec![home.join(".codex/sessions")],
    );
    // Claude Code 有的安装方式写到 XDG 目录而不是 ~/.claude，默认两个都扫。
    assert_eq!(
        ingest::source_scan_dirs_with(&overrides, home, Source::Claude),
        vec![
            home.join(".claude/projects"),
            home.join(".config/claude/projects"),
        ],
    );
    assert_eq!(
        ingest::source_scan_dirs_with(&overrides, home, Source::Copilot),
        vec![home.join(".copilot/session-state")],
    );
}

#[test]
fn source_scan_dirs_env_override_replaces_defaults_with_same_leaf_join_rule() {
    let home = std::path::Path::new("/home/example");
    let overrides = ingest::PathOverrides::from([
        ("CODEX_HOME", vec![PathBuf::from("/custom/codex")]),
        (
            "CLAUDE_CONFIG_DIR",
            vec![
                PathBuf::from("/custom/claude-a"),
                PathBuf::from("/custom/claude-b"),
            ],
        ),
    ]);

    assert_eq!(
        ingest::source_scan_dirs_with(&overrides, home, Source::Codex),
        vec![PathBuf::from("/custom/codex/sessions")],
    );
    // 覆盖后不再回退到默认的 XDG 双路径，只扫用户显式给出的目录。
    assert_eq!(
        ingest::source_scan_dirs_with(&overrides, home, Source::Claude),
        vec![
            PathBuf::from("/custom/claude-a/projects"),
            PathBuf::from("/custom/claude-b/projects"),
        ],
    );
    // 未覆盖的 Source 仍然用默认路径。
    assert_eq!(
        ingest::source_scan_dirs_with(&overrides, home, Source::Grok),
        vec![home.join(".grok/sessions")],
    );
}

#[test]
fn ingest_scans_multiple_overridden_directories_for_one_source() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();

    // 默认路径 home/.codex/sessions 放一份数据，用来验证覆盖后它不会再被扫到。
    let default_sessions = home.join(".codex/sessions");
    std::fs::create_dir_all(&default_sessions).unwrap();
    std::fs::write(
        default_sessions.join("ignored.jsonl"),
        fixture("codex.jsonl"),
    )
    .unwrap();

    // CODEX_HOME 覆盖为两个自定义根目录（逗号分隔多个），两个都要按同样的 /sessions
    // 规则拼接、都要被扫到。
    let root_a = home.join("codex-root-a");
    let root_b = home.join("codex-root-b");
    std::fs::create_dir_all(root_a.join("sessions")).unwrap();
    std::fs::create_dir_all(root_b.join("sessions")).unwrap();
    std::fs::write(root_a.join("sessions/a.jsonl"), fixture("codex.jsonl")).unwrap();
    std::fs::write(root_b.join("sessions/b.jsonl"), fixture("codex.jsonl")).unwrap();

    let overrides =
        ingest::PathOverrides::from([("CODEX_HOME", vec![root_a.clone(), root_b.clone()])]);

    let conn = store::open_memory().unwrap();
    let report = ingest::ingest_all_with_overrides(&conn, home, &overrides).unwrap();

    assert_eq!(report.files_parsed, 2);
    let records = store::load_all(&conn).unwrap();
    assert_eq!(records.len(), 4);
    assert!(records.iter().all(|r| r.source == Source::Codex));
    assert!(records
        .iter()
        .all(|r| !r.source_file.contains("ignored.jsonl")));

    // 删掉其中一个根目录下的文件，reconcile 应该只处理那一份，另一份不受影响——
    // 说明多目录是合并到同一次对账里的，而不是互相独立、互不感知。
    // 按 ADR 0004，消失的文件只归档、不物理删除，归档记录仍计入统计。
    std::fs::remove_file(root_a.join("sessions/a.jsonl")).unwrap();
    let second = ingest::ingest_all_with_overrides(&conn, home, &overrides).unwrap();
    assert_eq!(second.records_removed, 0);
    assert_eq!(second.records_archived, 2);
    assert_eq!(
        store::load_all(&conn).unwrap().len(),
        4,
        "archived records still count in totals"
    );

    // 被归档的正好是 root_a 那一份：显式清理归档记录后，剩下的应当只有 root_b 的记录。
    assert_eq!(
        store::purge_archived(&conn, Some(Source::Codex)).unwrap(),
        2
    );
    let remaining = store::load_all(&conn).unwrap();
    assert_eq!(remaining.len(), 2);
    assert!(remaining
        .iter()
        .all(|r| r.source_file.contains("codex-root-b")));
}

fn rollup_sum(
    records: &[UsageRecord],
    filter: &Filter,
    prices: &PriceTable,
    selector: impl Fn(&UsageRecord) -> String,
) -> i64 {
    aggregate::by_name(records, filter, prices, selector)
        .iter()
        .map(|row| row.total_tokens)
        .sum()
}

fn assert_rollups_match_overview(records: &[UsageRecord], filter: &Filter) {
    let prices = PriceTable::default();
    let overview = aggregate::overview(records, filter, &prices);
    assert_eq!(
        overview.total_tokens,
        rollup_sum(records, filter, &prices, |r| r.source.as_str().to_string())
    );
    assert_eq!(
        overview.total_tokens,
        rollup_sum(records, filter, &prices, |r| r.model.clone())
    );
    assert_eq!(
        overview.total_tokens,
        rollup_sum(records, filter, &prices, |r| r.provider.clone())
    );
    assert_eq!(
        overview.total_tokens,
        rollup_sum(records, filter, &prices, |r| r.project.clone())
    );
    let session_total: i64 = aggregate::top_sessions(records, filter, &prices, usize::MAX)
        .iter()
        .map(|row| row.total_tokens)
        .sum();
    assert_eq!(overview.total_tokens, session_total);
}

#[test]
fn overview_matches_source_model_project_and_session_rollups() {
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &seed_records()).unwrap();
    let records = store::load_all(&conn).unwrap();
    assert_rollups_match_overview(&records, &Filter::default());

    let from_aug2 = Filter {
        from: Some("2026-08-02T00:00:00Z".into()),
        ..Filter::default()
    };
    let filtered = aggregate::overview(&records, &from_aug2, &PriceTable::default());
    assert_eq!(filtered.total_tokens, 350);
    assert_rollups_match_overview(&records, &from_aug2);
}

fn write_all_source_fixtures(home: &std::path::Path) {
    let paths: [(&str, &str); 8] = [
        (".codex/sessions/one.jsonl", "codex.jsonl"),
        (
            ".claude/projects/-Users-zhangyanhua-AI-TradingAgents-CN/04868551-34c3-4588-b984-6ae9a5d95f8a.jsonl",
            "claude.jsonl",
        ),
        (
            ".pi/agent/sessions/--Users-zhangyanhua-workCode-ruoyi-ui-vue3--/s.jsonl",
            "pi.jsonl",
        ),
        (
            ".kimi/sessions/hash/bd1ab6fc-768d-4cff-b4c4-221a583c3af8/wire.jsonl",
            "kimi-wire.jsonl",
        ),
        (
            ".gemini/tmp/ruoyi-ui-vue3/chats/session-2026-03-07.json",
            "gemini-session.json",
        ),
        (
            ".grok/sessions/%2FUsers%2Fzhangyanhua%2FAI%2FTradingAgents-CN/019fd235/updates.jsonl",
            "grok-updates.jsonl",
        ),
        (".qwen/tmp/hash/logs.json", "qwen-logs.json"),
        (
            ".copilot/session-state/c0ffee11-2222-4333-8444-555566667777/events.jsonl",
            "copilot-events.jsonl",
        ),
    ];
    for (rel, name) in paths {
        let path = home.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, fixture(name)).unwrap();
    }
    let factory = home.join(
        ".factory/sessions/-Users-zhangyanhua-AI-cli/9ab2ca7b-bd30-495b-9434-07892ee0e5e6.settings.json",
    );
    std::fs::create_dir_all(factory.parent().unwrap()).unwrap();
    std::fs::write(&factory, fixture("factory.settings.json")).unwrap();
    let dsh = home.join(".dsh/sessions/--Users-zhangyanhua-AI-pi--/session.jsonl.zstd");
    std::fs::create_dir_all(dsh.parent().unwrap()).unwrap();
    let compressed = zstd::encode_all(fixture("dsh.jsonl").as_bytes(), 0).unwrap();
    std::fs::write(&dsh, compressed).unwrap();
}

#[test]
fn ingest_all_fixtures_is_stable_on_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    write_all_source_fixtures(home);
    let conn = store::open_memory().unwrap();
    let first = ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(first.files_parsed, 10);
    assert_eq!(first.records_written, 16);
    let stored = store::load_all(&conn).unwrap();
    assert_eq!(stored.len(), 16);
    assert_eq!(stored.iter().map(|r| r.total_tokens).sum::<i64>(), 828446);
    assert_rollups_match_overview(&stored, &Filter::default());

    let second = ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(second.files_parsed, 0);
    assert_eq!(second.files_skipped, 10);
    assert_eq!(second.records_written, 0);
    let again = store::load_all(&conn).unwrap();
    assert_eq!(again.len(), 16);
    assert_eq!(again.iter().map(|r| r.total_tokens).sum::<i64>(), 828446);
}

#[test]
fn scan_is_stale_detects_new_changed_and_deleted_source_files() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let sessions = home.join(".codex/sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let first = sessions.join("one.jsonl");
    std::fs::write(&first, fixture("codex.jsonl")).unwrap();

    let conn = store::open_memory().unwrap();
    assert!(
        ingest::scan_is_stale(&conn, home).unwrap(),
        "empty cache should be stale when source files exist"
    );

    ingest::ingest_all(&conn, home).unwrap();
    assert!(!ingest::scan_is_stale(&conn, home).unwrap());

    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    let file = std::fs::File::options().write(true).open(&first).unwrap();
    file.set_modified(later).unwrap();
    drop(file);
    assert!(
        ingest::scan_is_stale(&conn, home).unwrap(),
        "mtime change should be stale"
    );

    ingest::ingest_all(&conn, home).unwrap();
    assert!(!ingest::scan_is_stale(&conn, home).unwrap());

    std::fs::write(sessions.join("two.jsonl"), fixture("codex.jsonl")).unwrap();
    assert!(
        ingest::scan_is_stale(&conn, home).unwrap(),
        "new file should be stale"
    );

    ingest::ingest_all(&conn, home).unwrap();
    assert!(!ingest::scan_is_stale(&conn, home).unwrap());

    std::fs::remove_file(sessions.join("two.jsonl")).unwrap();
    assert!(
        ingest::scan_is_stale(&conn, home).unwrap(),
        "deleted cached file should be stale"
    );
}

#[ignore]
#[test]
fn ingest_real_home_rollups_match_overview() {
    let conn = store::open_memory().unwrap();
    ingest::ingest_all(&conn, &ingest::default_home()).unwrap();
    let records = store::load_all(&conn).unwrap();
    assert_rollups_match_overview(&records, &Filter::default());
}

fn window_rec(occurred_at: &str, source: Source, session_id: &str, total: i64) -> UsageRecord {
    let mut record = rec(
        occurred_at,
        source,
        "claude-sonnet-5",
        "anthropic",
        "/proj/a",
        session_id,
        total,
    );
    record.native_cost = Some(total as f64 / 1000.0);
    record
}

#[test]
fn billing_window_keeps_activity_within_five_hours() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let records = vec![
        window_rec("2026-08-17T08:10:00Z", Source::Claude, "s1", 100),
        window_rec("2026-08-17T09:10:00Z", Source::Claude, "s1", 50),
    ];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert_eq!(dto.current.len(), 1);
    assert!(dto.recent.is_empty());
    let window = &dto.current[0];
    assert_eq!(window.source, "claude");
    assert_eq!(window.start, "2026-08-17T08:00:00Z");
    assert_eq!(window.end, "2026-08-17T13:00:00Z");
    assert_eq!(window.total_tokens, 150);
    assert_eq!(window.session_count, 1);
    assert_eq!(window.remaining_minutes, Some(60));
    let burn = window.burn.as_ref().expect("应有燃烧速率");
    assert!((burn.tokens_per_minute - 2.5).abs() < 1e-9);
    let projection = window.projection.as_ref().expect("应有预测");
    assert_eq!(projection.total_tokens, 300);
}

#[test]
fn billing_window_opens_after_five_hour_gap() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let records = vec![
        window_rec("2026-08-17T02:00:00Z", Source::Claude, "s1", 80),
        window_rec("2026-08-17T08:00:00Z", Source::Claude, "s1", 40),
    ];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert_eq!(dto.current.len(), 1);
    assert_eq!(dto.recent.len(), 1);
    assert_eq!(dto.recent[0].start, "2026-08-17T02:00:00Z");
    assert_eq!(dto.recent[0].end, "2026-08-17T07:00:00Z");
    assert!(!dto.recent[0].is_active);
    assert_eq!(dto.current[0].start, "2026-08-17T08:00:00Z");
    assert_eq!(dto.current[0].total_tokens, 40);
}

#[test]
fn billing_window_floors_start_to_utc_hour() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let records = vec![window_rec("2026-08-17T08:37:12Z", Source::Claude, "s1", 10)];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert_eq!(dto.current[0].start, "2026-08-17T08:00:00Z");
    assert_eq!(dto.current[0].end, "2026-08-17T13:00:00Z");
    assert!(dto.current[0].burn.is_none());
    assert!(dto.current[0].projection.is_none());
}

#[test]
fn billing_window_expires_after_end_or_idle() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let expired = vec![window_rec("2026-08-17T06:00:00Z", Source::Claude, "s1", 20)];
    let dto = billing_window::summarize(&expired, &PriceTable::default(), now);
    assert!(dto.current.is_empty());
    assert_eq!(dto.recent.len(), 1);
    assert!(!dto.recent[0].is_active);
    assert_eq!(dto.recent[0].remaining_minutes, None);
}

#[test]
fn billing_windows_do_not_mix_sources() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let records = vec![
        window_rec("2026-08-17T10:00:00Z", Source::Claude, "c1", 30),
        window_rec("2026-08-17T10:05:00Z", Source::Codex, "x1", 90),
    ];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert_eq!(dto.current.len(), 2);
    let claude = dto
        .current
        .iter()
        .find(|window| window.source == "claude")
        .expect("claude");
    let codex = dto
        .current
        .iter()
        .find(|window| window.source == "codex")
        .expect("codex");
    assert_eq!(claude.total_tokens, 30);
    assert_eq!(codex.total_tokens, 90);
}

#[test]
fn weekly_window_sums_last_seven_days_per_source() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let records = vec![
        window_rec("2026-08-11T12:00:00Z", Source::Claude, "s1", 100),
        window_rec("2026-08-16T09:00:00Z", Source::Claude, "s1", 50),
        // 8 天前，超出 7 天滚动窗口，不应计入。
        window_rec("2026-08-09T12:00:00Z", Source::Claude, "s2", 999),
        window_rec("2026-08-15T00:00:00Z", Source::Codex, "x1", 70),
    ];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert_eq!(dto.weekly_window_days, 7);
    assert_eq!(dto.weekly.len(), 2);

    let claude = dto
        .weekly
        .iter()
        .find(|window| window.source == "claude")
        .expect("claude weekly window");
    assert_eq!(claude.total_tokens, 150);
    assert_eq!(claude.session_count, 1);
    assert_eq!(claude.end, "2026-08-17T12:00:00Z");
    assert_eq!(claude.start, "2026-08-10T12:00:00Z");
    assert!((claude.daily_average_tokens - 150.0 / 7.0).abs() < 1e-9);
    let claude_cost = claude.cost.expect("claude weekly cost");
    assert!((claude_cost - 0.15).abs() < 1e-9);
    let claude_daily_cost = claude.daily_average_cost.expect("claude daily cost");
    assert!((claude_daily_cost - claude_cost / 7.0).abs() < 1e-9);

    let codex = dto
        .weekly
        .iter()
        .find(|window| window.source == "codex")
        .expect("codex weekly window");
    assert_eq!(codex.total_tokens, 70);

    // 按 total_tokens 降序排列。
    assert_eq!(dto.weekly[0].source, "claude");
}

#[test]
fn weekly_window_excludes_activity_older_than_seven_days_but_within_the_lookback() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    // 10 天前：仍落在 14 天摄取回看窗内，但超出 7 天滚动窗口，不应计入 weekly。
    let records = vec![window_rec("2026-08-07T12:00:00Z", Source::Claude, "s1", 40)];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert!(dto.weekly.is_empty());
    // 仍应出现在 recent（5 小时窗）里，证明记录本身被正常摄取，只是不满足 weekly 的时间范围。
    assert_eq!(dto.recent.len(), 1);
}

fn assert_opt_f64_eq(a: Option<f64>, b: Option<f64>) {
    match (a, b) {
        (Some(x), Some(y)) => assert!((x - y).abs() < 1e-9, "金额不一致：{x} vs {y}"),
        (None, None) => {}
        (x, y) => panic!("金额 Option 不一致：{x:?} vs {y:?}"),
    }
}

fn diverse_prices() -> PriceTable {
    PriceTable {
        prices: vec![
            PriceEntry {
                model: "gpt-5.1-codex".into(),
                provider: Some("official".into()),
                input: 0.001,
                output: 0.002,
                cache_read: 0.0005,
                cache_creation: 0.003,
                origin: PriceOrigin::User,
            },
            PriceEntry {
                model: "claude-sonnet-5".into(),
                provider: None,
                input: 0.003,
                output: 0.015,
                cache_read: 0.001,
                cache_creation: 0.0,
                origin: PriceOrigin::User,
            },
            PriceEntry {
                model: "gpt-5.5".into(),
                provider: Some("subapi".into()),
                input: 0.02,
                output: 0.0,
                cache_read: 0.0,
                cache_creation: 0.0,
                origin: PriceOrigin::User,
            },
            PriceEntry {
                model: "gpt-5.5".into(),
                provider: None,
                input: 0.01,
                output: 0.0,
                cache_read: 0.0,
                cache_creation: 0.0,
                origin: PriceOrigin::User,
            },
        ],
    }
}

/// 覆盖：多来源、精确/兜底 provider 价格、native_cost、空项目、跨来源同名会话。
fn diverse_records() -> Vec<UsageRecord> {
    let mut r1 = rec(
        "2026-08-01T10:00:00Z",
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj/a",
        "s1",
        100,
    );
    r1.input_tokens = 80;
    r1.output_tokens = 10;
    r1.cache_read_tokens = 5;
    r1.cache_creation_tokens = 2;
    r1.reasoning_tokens = 3;

    let mut r2 = rec(
        "2026-08-02T10:00:00Z",
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj/a",
        "s1",
        50,
    );
    r2.input_tokens = 40;
    r2.output_tokens = 5;
    r2.cache_read_tokens = 3;
    r2.cache_creation_tokens = 1;
    r2.reasoning_tokens = 1;
    r2.native_cost = Some(1.5);

    let mut r3 = rec(
        "2026-08-01T11:00:00Z",
        Source::Claude,
        "claude-sonnet-5",
        "anthropic",
        "/proj/a",
        "s2",
        200,
    );
    r3.input_tokens = 150;
    r3.output_tokens = 50;

    let mut r4 = rec(
        "2026-08-08T10:00:00Z",
        Source::Pi,
        "gpt-5.5",
        "subapi",
        "/proj/b",
        "s3",
        300,
    );
    r4.input_tokens = 100;
    r4.output_tokens = 200;
    r4.native_cost = Some(0.25);

    let mut r5 = rec(
        "2026-08-09T10:00:00Z",
        Source::Pi,
        "gpt-5.5",
        "siliconflow",
        "/proj/b",
        "s4",
        60,
    );
    r5.input_tokens = 60;

    let mut r6 = rec(
        "2026-08-10T10:00:00Z",
        Source::Codex,
        "unknown-model",
        "official",
        "",
        "s5",
        30,
    );
    r6.input_tokens = 30;

    vec![r1, r2, r3, r4, r5, r6]
}

#[test]
fn sql_queries_match_in_memory_aggregates() {
    let conn = store::open_memory().unwrap();
    let records = diverse_records();
    store::insert_records(&conn, &records).unwrap();
    let prices = diverse_prices();

    // overview
    let sql_ov = query::overview(&conn, &Filter::default(), &prices).unwrap();
    let mem_ov = aggregate::overview(&records, &Filter::default(), &prices);
    assert_eq!(sql_ov.total_tokens, mem_ov.total_tokens);
    assert_eq!(sql_ov.input_tokens, mem_ov.input_tokens);
    assert_eq!(sql_ov.output_tokens, mem_ov.output_tokens);
    assert_eq!(sql_ov.cache_read_tokens, mem_ov.cache_read_tokens);
    assert_eq!(sql_ov.cache_creation_tokens, mem_ov.cache_creation_tokens);
    assert_eq!(sql_ov.reasoning_tokens, mem_ov.reasoning_tokens);
    assert_eq!(sql_ov.session_count, mem_ov.session_count);
    assert_eq!(sql_ov.unpriced, mem_ov.unpriced);
    assert_opt_f64_eq(sql_ov.cost, mem_ov.cost);

    // trend 四种粒度
    for grain in ["hour", "day", "week", "month"] {
        let sql_tr = query::trend(&conn, &Filter::default(), &prices, grain).unwrap();
        let mem_tr = aggregate::trend(&records, &Filter::default(), &prices, grain);
        assert_eq!(sql_tr, mem_tr, "trend grain={grain} 不一致");
    }

    // breakdown 五个维度
    for dim in ["application", "source", "model", "provider", "project"] {
        let sql_bd = query::breakdown(&conn, &Filter::default(), &prices, dim).unwrap();
        let mem_bd = aggregate::by_name(&records, &Filter::default(), &prices, |r| match dim {
            "application" => r.source.application_name().to_string(),
            "source" => r.source.as_str().to_string(),
            "model" => r.model.clone(),
            "provider" => r.provider.clone(),
            "project" => r.project.clone(),
            _ => unreachable!(),
        });
        assert_eq!(sql_bd.len(), mem_bd.len(), "breakdown dim={dim} 行数不一致");
        for (s, m) in sql_bd.iter().zip(mem_bd.iter()) {
            assert_eq!(s.name, m.name);
            assert_eq!(s.total_tokens, m.total_tokens);
            assert!((s.share - m.share).abs() < 1e-9);
            assert_eq!(s.unpriced, m.unpriced);
            assert_opt_f64_eq(s.cost, m.cost);
        }
    }

    // application_analytics（DTO 整体相等）
    let sql_aa = query::application_analytics(&conn, &Filter::default(), "day").unwrap();
    let mem_aa = aggregate::application_analytics(&records, &Filter::default(), "day");
    assert_eq!(sql_aa, mem_aa);

    // top_sessions
    let sql_top = query::top_sessions(&conn, &Filter::default(), &prices, 10).unwrap();
    let mem_top = aggregate::top_sessions(&records, &Filter::default(), &prices, 10);
    assert_eq!(sql_top, mem_top);

    // session_turns（含 source 过滤与无 source）
    for source in [Some("codex"), None] {
        let sql_turns =
            query::session_turns(&conn, "s1", source, &Filter::default(), &prices).unwrap();
        let mem_turns =
            aggregate::session_turns(&records, "s1", source, &Filter::default(), &prices);
        assert_eq!(
            sql_turns, mem_turns,
            "session_turns source={source:?} 不一致"
        );
    }

    // filter_options
    let sql_fo = query::filter_options(&conn).unwrap();
    let mem_fo = aggregate::filter_options(&records);
    assert_eq!(sql_fo.sources, mem_fo.sources);
    assert_eq!(sql_fo.models, mem_fo.models);
    assert_eq!(sql_fo.projects, mem_fo.projects);
    assert_eq!(sql_fo.providers, mem_fo.providers);

    // billing_windows（忽略日期筛选，按来源切 5h 窗）
    let window_now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let sql_bw = query::billing_windows(&conn, &Filter::default(), &prices, window_now).unwrap();
    let mem_bw = aggregate::billing_windows(&records, &Filter::default(), &prices, window_now);
    assert_eq!(sql_bw, mem_bw);
    let dated = Filter {
        from: Some("2026-08-09T00:00:00Z".into()),
        to: Some("2026-08-09T23:59:59Z".into()),
        ..Filter::default()
    };
    let sql_bw_dated = query::billing_windows(&conn, &dated, &prices, window_now).unwrap();
    let mem_bw_dated = aggregate::billing_windows(&records, &dated, &prices, window_now);
    assert_eq!(sql_bw_dated, mem_bw_dated);
    assert_eq!(sql_bw_dated, sql_bw);

    // 过滤条件的 overview 对照（覆盖 WHERE 子句）
    let filters = [
        Filter {
            from: Some("2026-08-02T00:00:00Z".into()),
            ..Filter::default()
        },
        Filter {
            to: Some("2026-08-02T00:00:00Z".into()),
            ..Filter::default()
        },
        Filter {
            projects: vec!["/proj/b".into()],
            ..Filter::default()
        },
        Filter {
            models: vec!["gpt-5.5".into()],
            ..Filter::default()
        },
        Filter {
            sources: vec!["codex".into()],
            ..Filter::default()
        },
        Filter {
            providers: vec!["official".into()],
            ..Filter::default()
        },
    ];
    for f in &filters {
        let sql_ov = query::overview(&conn, f, &prices).unwrap();
        let mem_ov = aggregate::overview(&records, f, &prices);
        assert_eq!(sql_ov.total_tokens, mem_ov.total_tokens, "filter={f:?}");
        assert_eq!(sql_ov.session_count, mem_ov.session_count);
        assert_eq!(sql_ov.unpriced, mem_ov.unpriced);
        assert_opt_f64_eq(sql_ov.cost, mem_ov.cost);
    }
}

fn local_noon_iso(date: chrono::NaiveDate) -> String {
    let noon = date.and_hms_opt(12, 0, 0).expect("noon");
    noon.and_local_timezone(Local)
        .earliest()
        .or_else(|| noon.and_local_timezone(Local).latest())
        .expect("local noon")
        .with_timezone(&Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[test]
fn local_day_filter_uses_local_midnight_and_end_as_utc_z() {
    let now = Local
        .with_ymd_and_hms(2026, 8, 17, 19, 22, 30)
        .single()
        .expect("fixed local time");
    let filter = crate::tray::local_day_filter(now);
    let from = filter.from.expect("from");
    let to = filter.to.expect("to");
    assert!(from.ends_with('Z'), "{from}");
    assert!(to.ends_with('Z'), "{to}");

    let from_local = chrono::DateTime::parse_from_rfc3339(&from)
        .unwrap()
        .with_timezone(&Local);
    let to_local = chrono::DateTime::parse_from_rfc3339(&to)
        .unwrap()
        .with_timezone(&Local);
    assert_eq!(from_local.date_naive(), now.date_naive());
    assert_eq!(
        from_local.time(),
        NaiveTime::from_hms_milli_opt(0, 0, 0, 0).unwrap()
    );
    assert_eq!(to_local.date_naive(), now.date_naive());
    assert_eq!(
        to_local.time(),
        NaiveTime::from_hms_milli_opt(23, 59, 59, 999).unwrap()
    );
}

#[test]
fn tray_format_title_marks_unpriced() {
    assert_eq!(crate::tray::format_title(Some(1.23), false), "$1.23");
    assert_eq!(crate::tray::format_title(Some(1.23), true), "$1.23*");
    assert_eq!(crate::tray::format_title(None, false), "$0.00");
    assert_eq!(crate::tray::format_title(None, true), "—");
}

#[test]
fn today_filter_overview_matches_in_memory_and_excludes_other_days() {
    let now = Local::now();
    let filter = crate::tray::local_day_filter(now);
    let mut today = rec(
        &local_noon_iso(now.date_naive()),
        Source::Claude,
        "claude",
        "anthropic",
        "/p",
        "s-today",
        100,
    );
    today.native_cost = Some(1.5);
    let mut yesterday = rec(
        &local_noon_iso(now.date_naive() - chrono::Days::new(1)),
        Source::Codex,
        "gpt",
        "official",
        "/p",
        "s-yday",
        200,
    );
    yesterday.native_cost = Some(9.0);

    let records = vec![today, yesterday];
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();
    let prices = PriceTable::default();

    let sql = query::overview(&conn, &filter, &prices).unwrap();
    let mem = aggregate::overview(&records, &filter, &prices);
    assert_eq!(sql.total_tokens, 100);
    assert_eq!(mem.total_tokens, 100);
    assert_eq!(sql.cost, Some(1.5));
    assert_eq!(mem.cost, Some(1.5));
    assert!(!sql.unpriced);
    assert!(!mem.unpriced);
}

// ---------- LiteLLM 价目快照 ----------

const LITELLM_RAW_SAMPLE: &str = r#"{
    "sample_spec": {"note": "占位，应被跳过"},
    "gpt-4o": {
        "litellm_provider": "openai",
        "mode": "chat",
        "input_cost_per_token": 2.5e-06,
        "output_cost_per_token": 1e-05,
        "cache_read_input_token_cost": 1.25e-06
    },
    "anthropic/claude-3-5-sonnet": {
        "litellm_provider": "anthropic",
        "mode": "chat",
        "input_cost_per_token": 3e-06,
        "output_cost_per_token": 1.5e-05,
        "cache_creation_input_token_cost": 3.75e-06
    },
    "claude-3-5-sonnet": {
        "litellm_provider": "anthropic",
        "mode": "chat",
        "input_cost_per_token": 3e-06,
        "output_cost_per_token": 1.5e-05
    },
    "text-embedding-3-small": {
        "litellm_provider": "openai",
        "mode": "embedding",
        "input_cost_per_token": 2e-08
    },
    "free-local-model": {
        "litellm_provider": "ollama",
        "mode": "chat",
        "input_cost_per_token": 0,
        "output_cost_per_token": 0
    }
}"#;

#[test]
fn litellm_snapshot_normalizes_upstream_and_skips_noise() {
    let snapshot = crate::litellm::parse_litellm_raw(LITELLM_RAW_SAMPLE, "2026-08-17")
        .expect("parse litellm sample");
    assert_eq!(snapshot.as_of, "2026-08-17");
    assert_eq!(snapshot.source, "litellm");

    let by_model: std::collections::HashMap<&str, &PriceEntry> = snapshot
        .entries
        .iter()
        .map(|e| (e.model.as_str(), e))
        .collect();

    // sample_spec、embedding 模式、纯零价条目都应被跳过。
    assert!(!by_model.contains_key("sample_spec"));
    assert!(!by_model.contains_key("text-embedding-3-small"));
    assert!(!by_model.contains_key("free-local-model"));

    // 归一后 provider 一律为空，充当按模型兜底。
    let gpt = by_model.get("gpt-4o").expect("gpt-4o present");
    assert_eq!(gpt.provider, None);
    assert_eq!(gpt.input, 2.5e-06);
    assert_eq!(gpt.output, 1e-05);
    assert_eq!(gpt.cache_read, 1.25e-06);

    // 同一模型同时有裸键与带前缀键时，只保留裸键那条（无 cache_creation）。
    let claude = by_model.get("claude-3-5-sonnet").expect("claude present");
    assert_eq!(claude.provider, None);
    assert_eq!(claude.cache_creation, 0.0);
    // 去重后每个模型只有一条。
    assert_eq!(
        snapshot
            .entries
            .iter()
            .filter(|e| e.model == "claude-3-5-sonnet")
            .count(),
        1
    );
}

#[test]
fn litellm_merge_lets_user_prices_win_and_fills_the_rest() {
    let snapshot = crate::litellm::parse_litellm_raw(LITELLM_RAW_SAMPLE, "2026-08-17").unwrap();
    let user = PriceTable {
        prices: vec![PriceEntry {
            model: "gpt-4o".into(),
            provider: None,
            input: 9.9,
            output: 9.9,
            cache_read: 0.0,
            cache_creation: 0.0,
            origin: PriceOrigin::User,
        }],
    };
    let merged = crate::litellm::merge(&user, &snapshot);

    // 用户配置过的 gpt-4o 不被快照覆盖，只保留用户那条。
    let gpt: Vec<&PriceEntry> = merged
        .prices
        .iter()
        .filter(|e| e.model == "gpt-4o")
        .collect();
    assert_eq!(gpt.len(), 1);
    assert_eq!(gpt[0].input, 9.9);
    assert_eq!(gpt[0].origin, PriceOrigin::User);
    // 用户没配的模型由快照补齐，并打上 snapshot 来源。
    let claude = merged
        .prices
        .iter()
        .find(|e| e.model == "claude-3-5-sonnet")
        .expect("snapshot fills missing model");
    assert_eq!(claude.origin, PriceOrigin::Snapshot);
}

#[test]
fn litellm_snapshot_fills_cost_for_models_without_native_or_user_price() {
    let snapshot = crate::litellm::parse_litellm_raw(LITELLM_RAW_SAMPLE, "2026-08-17").unwrap();
    // 空的用户单价表：完全依赖快照兜底。
    let effective = crate::litellm::merge(&PriceTable::default(), &snapshot);

    // Codex 类记录：无 native_cost、provider 为空，模型名与快照一致。
    let mut record = rec(
        "2026-08-01T10:00:00Z",
        Source::Codex,
        "gpt-4o",
        "",
        "/proj/a",
        "s1",
        0,
    );
    record.input_tokens = 1_000_000;
    record.output_tokens = 1_000_000;

    let derived = derive_cost(&record, &effective);
    assert!(!derived.unpriced, "快照应把该模型标记为已定价");
    assert!(!derived.source_native, "快照兜底不是来源自带费用");
    assert_eq!(derived.cost_source, CostSource::Snapshot);
    assert_eq!(derived.amount, Some(2.5 + 10.0));

    // 有来源自带费用时优先 native。
    let native = UsageRecord {
        native_cost: Some(4.2),
        ..record.clone()
    };
    let native_derived = derive_cost(&native, &effective);
    assert_eq!(native_derived.amount, Some(4.2));
    assert_eq!(native_derived.cost_source, CostSource::Native);

    // 快照没有的模型仍然是未定价。
    let unknown = rec(
        "2026-08-01T10:00:00Z",
        Source::Codex,
        "totally-unknown-model",
        "",
        "/proj/a",
        "s2",
        100,
    );
    let unknown_derived = derive_cost(&unknown, &effective);
    assert!(unknown_derived.unpriced);
    assert_eq!(unknown_derived.cost_source, CostSource::None);
}

#[test]
fn cost_source_labels_native_user_snapshot_and_none_on_sql_and_memory() {
    let snapshot = crate::litellm::parse_litellm_raw(LITELLM_RAW_SAMPLE, "2026-08-17").unwrap();
    let user = PriceTable {
        prices: vec![PriceEntry {
            model: "user-only-model".into(),
            provider: None,
            input: 0.001,
            output: 0.0,
            cache_read: 0.0,
            cache_creation: 0.0,
            origin: PriceOrigin::User,
        }],
    };
    let prices = crate::litellm::merge(&user, &snapshot);

    let mut native = rec(
        "2026-08-01T10:00:00Z",
        Source::Codex,
        "gpt-4o",
        "",
        "/proj/a",
        "s-native",
        0,
    );
    native.native_cost = Some(1.25);
    native.input_tokens = 10;

    let mut user_priced = rec(
        "2026-08-01T10:01:00Z",
        Source::Codex,
        "user-only-model",
        "",
        "/proj/a",
        "s-user",
        0,
    );
    user_priced.input_tokens = 1000;

    let mut snapshot_priced = rec(
        "2026-08-01T10:02:00Z",
        Source::Codex,
        "gpt-4o",
        "",
        "/proj/a",
        "s-snapshot",
        0,
    );
    snapshot_priced.input_tokens = 1_000_000;

    let unpriced = rec(
        "2026-08-01T10:03:00Z",
        Source::Codex,
        "totally-unknown-model",
        "",
        "/proj/a",
        "s-none",
        0,
    );

    let records = vec![
        native.clone(),
        user_priced.clone(),
        snapshot_priced.clone(),
        unpriced.clone(),
    ];
    let conn = store::open_memory().unwrap();
    store::insert_records(&conn, &records).unwrap();

    let cases = [
        ("s-native", CostSource::Native, "来源自带", Some(1.25)),
        ("s-user", CostSource::User, "用户单价", Some(1.0)),
        (
            "s-snapshot",
            CostSource::Snapshot,
            "LiteLLM 快照",
            Some(2.5),
        ),
        ("s-none", CostSource::None, "单价未配置", None),
    ];
    for (session_id, source, note, cost) in cases {
        let mem = aggregate::session_turns(
            &records,
            session_id,
            Some("codex"),
            &Filter::default(),
            &prices,
        );
        let sql = query::session_turns(
            &conn,
            session_id,
            Some("codex"),
            &Filter::default(),
            &prices,
        )
        .unwrap();
        assert_eq!(mem, sql, "session_turns cost_source 不一致：{session_id}");
        assert_eq!(mem[0].cost_source, source);
        assert_eq!(mem[0].cost_note.as_deref(), Some(note));
        assert_eq!(mem[0].cost, cost);
    }
}

#[test]
fn price_entry_origin_defaults_to_user_for_legacy_json() {
    let table: PriceTable = serde_json::from_str(
        r#"{"prices":[{"model":"gpt-4o","provider":null,"input":1.0,"output":2.0,"cache_read":0.0,"cache_creation":0.0}]}"#,
    )
    .unwrap();
    assert_eq!(table.prices[0].origin, PriceOrigin::User);
    let encoded = serde_json::to_string(&table).unwrap();
    assert!(
        !encoded.contains("origin"),
        "用户单价序列化不应写出默认 origin：{encoded}"
    );
}

#[test]
fn bundled_litellm_snapshot_is_valid_and_covers_common_models() {
    let bundled = crate::litellm::bundled_snapshot();
    assert!(
        bundled.entries.len() > 200,
        "内置快照应包含大量模型，实际 {}",
        bundled.entries.len()
    );
    assert_eq!(bundled.source, "litellm");
    let models: std::collections::HashSet<&str> =
        bundled.entries.iter().map(|e| e.model.as_str()).collect();
    for expected in ["gpt-4o", "claude-3-5-sonnet-20241022", "gemini-2.5-pro"] {
        assert!(models.contains(expected), "内置快照缺少常见模型 {expected}");
    }
    // 所有条目都应有非零单价（生成阶段已过滤零价）。
    assert!(bundled
        .entries
        .iter()
        .all(|e| e.input > 0.0 || e.output > 0.0));
}

#[test]
fn local_month_filter_starts_at_first_of_month_local_midnight() {
    let now = Local
        .with_ymd_and_hms(2026, 8, 17, 19, 22, 30)
        .single()
        .expect("fixed local time");
    let filter = budget::local_month_filter(now);
    let from = filter.from.expect("from");
    let to = filter.to.expect("to");
    assert!(from.ends_with('Z'), "{from}");
    assert!(to.ends_with('Z'), "{to}");

    let from_local = chrono::DateTime::parse_from_rfc3339(&from)
        .unwrap()
        .with_timezone(&Local);
    assert_eq!(
        from_local.date_naive(),
        chrono::NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()
    );
    assert_eq!(
        from_local.time(),
        NaiveTime::from_hms_milli_opt(0, 0, 0, 0).unwrap()
    );

    let to_local = chrono::DateTime::parse_from_rfc3339(&to)
        .unwrap()
        .with_timezone(&Local);
    assert_eq!(to_local.date_naive(), now.date_naive());
    assert_eq!(
        to_local.time().num_seconds_from_midnight(),
        19 * 3600 + 22 * 60 + 30
    );
}

#[test]
fn budget_status_scopes_cost_to_the_current_calendar_month() {
    let now = Local::now();
    let conn = store::open_memory().unwrap();

    let mut this_month = rec(
        &now.with_timezone(&Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        Source::Claude,
        "claude-sonnet-5",
        "anthropic",
        "/proj",
        "s1",
        1000,
    );
    this_month.native_cost = Some(30.0);

    // 40 天前无论如何都落在上个自然月之前，不应计入本月预算。
    let mut last_month = rec(
        &(now - chrono::Duration::days(40))
            .with_timezone(&Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj",
        "s2",
        2000,
    );
    last_month.native_cost = Some(999.0);

    store::insert_records(&conn, &[this_month, last_month]).unwrap();

    let config = BudgetConfig {
        monthly_usd: Some(100.0),
    };
    let dto = budget::status(&conn, &PriceTable::default(), &config, now).unwrap();

    assert_eq!(dto.month, now.format("%Y-%m").to_string());
    assert_eq!(dto.days_elapsed, now.day() as i64);
    assert!(dto.days_in_month >= 28 && dto.days_in_month <= 31);
    assert!((dto.month_to_date_cost - 30.0).abs() < 1e-9);
    assert!(!dto.unpriced);
    assert_eq!(dto.monthly_budget, Some(100.0));
    assert!((dto.percent_used.unwrap() - 30.0).abs() < 1e-9);
    // 预测费用按日均线性外推到月末，应不小于已产生的费用。
    assert!(dto.projected_month_cost.unwrap() >= dto.month_to_date_cost);
    assert_eq!(dto.thresholds, vec![50, 80, 100]);
}

#[test]
fn budget_status_without_a_configured_budget_has_no_percentages() {
    let now = Local::now();
    let conn = store::open_memory().unwrap();
    let config = BudgetConfig { monthly_usd: None };
    let dto = budget::status(&conn, &PriceTable::default(), &config, now).unwrap();
    assert_eq!(dto.monthly_budget, None);
    assert_eq!(dto.percent_used, None);
    assert_eq!(dto.percent_projected, None);
}

#[test]
fn thresholds_to_notify_only_returns_reached_and_unreported_ones() {
    assert_eq!(budget::thresholds_to_notify(45.0, &[]), Vec::<u32>::new());
    assert_eq!(budget::thresholds_to_notify(55.0, &[]), vec![50]);
    assert_eq!(budget::thresholds_to_notify(85.0, &[50]), vec![80]);
    assert_eq!(budget::thresholds_to_notify(120.0, &[50, 80]), vec![100]);
    assert_eq!(
        budget::thresholds_to_notify(120.0, &[50, 80, 100]),
        Vec::<u32>::new()
    );
}

#[test]
fn budget_config_round_trips_through_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("budget.json");
    assert_eq!(budget::load_config(&path), BudgetConfig::default());

    let config = BudgetConfig {
        monthly_usd: Some(42.5),
    };
    budget::save_config(&path, &config).unwrap();
    assert_eq!(budget::load_config(&path), config);
}

#[test]
fn backup_and_restore_round_trips_records_and_user_config() {
    let root = tempfile::tempdir().unwrap();
    let live = root.path().join("live");
    let dest = root.path().join("backup");
    std::fs::create_dir_all(&live).unwrap();

    let db_path = live.join("usage.sqlite");
    let prices_path = live.join("prices.json");
    let snapshot_path = live.join("litellm_prices.json");
    let budget_path = live.join("budget.json");
    let paths = backup::AppDataPaths {
        db_path: db_path.clone(),
        prices_path: prices_path.clone(),
        snapshot_path: snapshot_path.clone(),
        budget_path: budget_path.clone(),
    };

    let conn = store::open_db(db_path.to_str().unwrap()).unwrap();
    store::insert_records(
        &conn,
        &[rec(
            "2026-08-18T00:00:00.000Z",
            Source::Claude,
            "claude-sonnet-5",
            "anthropic",
            "/proj",
            "s1",
            42,
        )],
    )
    .unwrap();

    let prices = PriceTable {
        prices: vec![PriceEntry {
            model: "claude-sonnet-5".into(),
            provider: Some("anthropic".into()),
            input: 0.003,
            output: 0.015,
            cache_read: 0.0,
            cache_creation: 0.0,
            origin: PriceOrigin::User,
        }],
    };
    std::fs::write(&prices_path, serde_json::to_string_pretty(&prices).unwrap()).unwrap();
    budget::save_config(
        &budget_path,
        &BudgetConfig {
            monthly_usd: Some(20.0),
        },
    )
    .unwrap();
    std::fs::write(
        &snapshot_path,
        r#"{"as_of":"2026-01-01","source":"test","entries":[]}"#,
    )
    .unwrap();

    let manifest = backup::backup_to(&conn, &dest, &paths).unwrap();
    assert!(manifest.files.contains(&"usage.sqlite".to_string()));
    assert!(manifest.files.contains(&"prices.json".to_string()));
    assert!(manifest.files.contains(&"budget.json".to_string()));
    assert!(manifest.note.contains("钥匙串"));
    drop(conn);

    std::fs::write(&prices_path, "{\"prices\":[]}").unwrap();
    budget::save_config(&budget_path, &BudgetConfig { monthly_usd: None }).unwrap();
    std::fs::remove_file(&db_path).unwrap();
    let _ = std::fs::remove_file(live.join("usage.sqlite-wal"));
    let _ = std::fs::remove_file(live.join("usage.sqlite-shm"));

    backup::restore_from(&dest, &paths).unwrap();
    let restored = store::open_db(db_path.to_str().unwrap()).unwrap();
    let rows = store::load_all(&restored).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].total_tokens, 42);
    assert_eq!(budget::load_config(&budget_path).monthly_usd, Some(20.0));
    let restored_prices: PriceTable =
        serde_json::from_str(&std::fs::read_to_string(&prices_path).unwrap()).unwrap();
    assert_eq!(restored_prices.prices[0].model, "claude-sonnet-5");
}

#[test]
fn should_check_budget_skips_missing_or_non_positive_limits() {
    assert!(!budget::should_check_budget(&BudgetConfig {
        monthly_usd: None
    }));
    assert!(!budget::should_check_budget(&BudgetConfig {
        monthly_usd: Some(0.0),
    }));
    assert!(!budget::should_check_budget(&BudgetConfig {
        monthly_usd: Some(-10.0),
    }));
    assert!(budget::should_check_budget(&BudgetConfig {
        monthly_usd: Some(20.0),
    }));
}

#[test]
fn prepare_notifications_emits_each_threshold_once_in_the_same_month() {
    let empty = budget::NotifyState::default();
    let (after_50, crossed) = budget::prepare_notifications(empty, "2026-08", 50.0);
    assert_eq!(crossed, vec![50]);
    assert_eq!(after_50.month, "2026-08");
    assert_eq!(after_50.notified, vec![50]);

    let (after_repeat, crossed) = budget::prepare_notifications(after_50.clone(), "2026-08", 55.0);
    assert!(crossed.is_empty());
    assert_eq!(after_repeat, after_50);

    let (after_80, crossed) = budget::prepare_notifications(after_50, "2026-08", 80.0);
    assert_eq!(crossed, vec![80]);
    assert_eq!(after_80.notified, vec![50, 80]);

    let (after_100, crossed) = budget::prepare_notifications(after_80, "2026-08", 120.0);
    assert_eq!(crossed, vec![100]);
    assert_eq!(after_100.notified, vec![50, 80, 100]);

    let (after_all, crossed) = budget::prepare_notifications(after_100.clone(), "2026-08", 150.0);
    assert!(crossed.is_empty());
    assert_eq!(after_all, after_100);
}

#[test]
fn prepare_notifications_resets_notified_thresholds_on_month_change() {
    let last_month = budget::NotifyState {
        month: "2026-07".into(),
        notified: vec![50, 80, 100],
    };
    let (next, crossed) = budget::prepare_notifications(last_month, "2026-08", 52.0);
    assert_eq!(crossed, vec![50]);
    assert_eq!(next.month, "2026-08");
    assert_eq!(next.notified, vec![50]);
}

#[test]
fn notify_state_round_trips_through_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("budget-notify.json");
    assert_eq!(
        budget::load_notify_state(&path),
        budget::NotifyState::default()
    );

    let state = budget::NotifyState {
        month: "2026-08".into(),
        notified: vec![50, 80],
    };
    budget::save_notify_state(&path, &state).unwrap();
    assert_eq!(budget::load_notify_state(&path), state);
}

// ---------- Cursor 账号用量 ----------

#[test]
fn cursor_account_parser_maps_token_dimensions_and_keeps_duplicates() {
    let page = cursor_account::parse_cursor_usage_page(&fixture("cursor_account_usage.json"))
        .expect("parse cursor account fixture");
    assert_eq!(page.total_count, 3);
    let events = page.events;
    assert_eq!(events.len(), 3);

    assert_eq!(events[0].occurred_at, "2024-01-01T00:00:00+00:00");
    assert_eq!(events[0].model, "claude-4.5-sonnet");
    assert_eq!(events[0].input_tokens, 100);
    assert_eq!(events[0].output_tokens, 50);
    assert_eq!(events[0].cache_read_tokens, 20);
    assert_eq!(events[0].cache_creation_tokens, 10);
    assert!(!events[0].is_headless);

    assert_eq!(events[1].occurred_at, "2024-01-02T00:00:00+00:00");
    assert_eq!(events[1].model, "composer-2");
    assert_eq!(events[1].input_tokens, 200);
    assert_eq!(events[1].output_tokens, 80);
    assert_eq!(events[1].cache_read_tokens, 0);
    assert_eq!(events[1].cache_creation_tokens, 5);
    assert!(events[1].is_headless);

    assert_eq!(events[2], events[0]);
}

#[test]
fn cursor_account_parser_rejects_bad_json_and_skips_empty_payload() {
    let err =
        cursor_account::parse_cursor_usage_events("{not-json").expect_err("bad json should fail");
    assert!(err.contains("解析失败"), "错误应可读：{err}");

    let empty = cursor_account::parse_cursor_usage_events(r#"{"usageEventsDisplay":[]}"#)
        .expect("empty list is valid");
    assert!(empty.is_empty());

    let missing = cursor_account::parse_cursor_usage_events(r#"{"totalUsageEventsCount":0}"#)
        .expect_err("missing list is a structure change");
    assert!(missing.contains("结构已变更"), "{missing}");
}

#[test]
fn cursor_account_summary_adds_token_dimensions_without_dedup() {
    let events = cursor_account::parse_cursor_usage_events(&fixture("cursor_account_usage.json"))
        .expect("parse");
    let dto = cursor_account::summarize_cursor_usage(&events);
    assert_eq!(dto.event_count, 3);
    assert_eq!(dto.input_tokens, 400);
    assert_eq!(dto.output_tokens, 180);
    assert_eq!(dto.cache_read_tokens, 40);
    assert_eq!(dto.cache_creation_tokens, 25);
    assert_eq!(dto.total_tokens, 645);
    assert_eq!(dto.as_of, None);

    let empty = cursor_account::summarize_cursor_usage(&[]);
    assert_eq!(empty, crate::domain::CursorAccountUsageDto::empty());
}

#[test]
fn cursor_account_summary_buckets_tokens_by_local_day() {
    use crate::domain::CursorUsageEvent;

    fn ev(occurred_at: &str, input: i64, output: i64) -> CursorUsageEvent {
        CursorUsageEvent {
            occurred_at: occurred_at.to_string(),
            model: "m".into(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            is_headless: false,
        }
    }

    let day_a = chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let day_b = chrono::NaiveDate::from_ymd_opt(2024, 1, 16).unwrap();
    let dto = cursor_account::summarize_cursor_usage(&[
        ev(&local_noon_iso(day_a), 100, 10),
        ev(&local_noon_iso(day_a), 50, 5),
        ev(&local_noon_iso(day_b), 20, 2),
    ]);
    assert_eq!(dto.daily.len(), 2);
    assert_eq!(dto.daily[0].bucket, "2024-01-15");
    assert_eq!(dto.daily[0].input_tokens, 150);
    assert_eq!(dto.daily[0].output_tokens, 15);
    assert_eq!(dto.daily[0].total_tokens, 165);
    assert_eq!(dto.daily[0].cost, None);
    assert_eq!(dto.daily[1].bucket, "2024-01-16");
    assert_eq!(dto.daily[1].total_tokens, 22);

    let single = cursor_account::summarize_cursor_usage(&[ev(&local_noon_iso(day_a), 7, 3)]);
    assert_eq!(single.daily.len(), 1);
    assert_eq!(single.daily[0].bucket, "2024-01-15");
    assert_eq!(single.daily[0].total_tokens, 10);

    let empty = cursor_account::summarize_cursor_usage(&[]);
    assert!(empty.daily.is_empty());
}

#[test]
fn cursor_account_summary_splits_by_model_and_headless() {
    use crate::domain::CursorUsageEvent;

    fn ev(model: &str, input: i64, headless: bool) -> CursorUsageEvent {
        CursorUsageEvent {
            occurred_at: "2024-01-15T12:00:00+00:00".into(),
            model: model.into(),
            input_tokens: input,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            is_headless: headless,
        }
    }

    let dto = cursor_account::summarize_cursor_usage(&[
        ev("claude-4.5-sonnet", 300, false),
        ev("composer-2", 100, true),
        ev("claude-4.5-sonnet", 100, true),
    ]);
    assert_eq!(dto.by_model.len(), 2);
    assert_eq!(dto.by_model[0].name, "claude-4.5-sonnet");
    assert_eq!(dto.by_model[0].total_tokens, 400);
    assert!((dto.by_model[0].share - 0.8).abs() < 1e-9);
    assert_eq!(dto.by_model[1].name, "composer-2");
    assert_eq!(dto.by_model[1].total_tokens, 100);
    assert!((dto.by_model[1].share - 0.2).abs() < 1e-9);
    assert_eq!(dto.headless_tokens, 200);
    assert_eq!(dto.interactive_tokens, 300);
    assert!((dto.headless_share.unwrap() - 0.4).abs() < 1e-9);

    let interactive_only = cursor_account::summarize_cursor_usage(&[ev("gpt-5", 50, false)]);
    assert_eq!(interactive_only.by_model.len(), 1);
    assert_eq!(interactive_only.headless_tokens, 0);
    assert_eq!(interactive_only.interactive_tokens, 50);
    assert_eq!(interactive_only.headless_share, Some(0.0));

    let empty = cursor_account::summarize_cursor_usage(&[]);
    assert!(empty.by_model.is_empty());
    assert_eq!(empty.headless_tokens, 0);
    assert_eq!(empty.interactive_tokens, 0);
    assert_eq!(empty.headless_share, None);
}

#[test]
fn cursor_account_store_dedups_by_fingerprint() {
    let events = cursor_account::parse_cursor_usage_events(&fixture("cursor_account_usage.json"))
        .expect("parse");
    assert_eq!(events.len(), 3);

    let conn = store::open_memory().unwrap();
    let first = store::upsert_cursor_account_events(&conn, &events).unwrap();
    let second = store::upsert_cursor_account_events(&conn, &events).unwrap();
    assert_eq!(first, 2);
    assert_eq!(second, 0);

    let stored = store::load_cursor_account_events(&conn).unwrap();
    assert_eq!(stored.len(), 2);
    let dto = cursor_account::summarize_cursor_usage(&stored);
    assert_eq!(dto.event_count, 2);
    assert_eq!(dto.input_tokens, 300);
    assert_eq!(dto.output_tokens, 130);
    assert_eq!(dto.cache_read_tokens, 20);
    assert_eq!(dto.cache_creation_tokens, 15);
    assert_eq!(dto.total_tokens, 465);

    store::set_cursor_account_as_of(&conn, "2026-08-17T12:00:00+00:00").unwrap();
    assert_eq!(
        store::cursor_account_as_of(&conn).unwrap().as_deref(),
        Some("2026-08-17T12:00:00+00:00")
    );

    store::clear_cursor_account_usage(&conn).unwrap();
    assert!(store::load_cursor_account_events(&conn).unwrap().is_empty());
    assert_eq!(store::cursor_account_as_of(&conn).unwrap(), None);
}

#[test]
fn cursor_account_clear_resets_watermark_without_touching_usage_records() {
    let conn = store::open_memory().unwrap();
    store::insert_records(
        &conn,
        &[rec(
            "2026-01-01T00:00:00+00:00",
            Source::Codex,
            "gpt-5",
            "openai",
            "/tmp/demo",
            "sess-keep",
            42,
        )],
    )
    .unwrap();
    crate::cursor_account::ingest_raw_pages(&conn, &[CURSOR_PAGE_ONE]).unwrap();
    store::set_cursor_account_as_of(&conn, "2026-08-17T12:00:00+00:00").unwrap();
    assert_eq!(
        crate::cursor_account::incremental_start_ms(&conn).unwrap(),
        1_704_153_600_000
    );

    let cleared = crate::cursor_account::clear_cache(&conn).unwrap();
    assert_eq!(cleared.event_count, 0);
    assert_eq!(cleared.total_tokens, 0);
    assert_eq!(cleared.as_of, None);
    assert_eq!(
        crate::cursor_account::incremental_start_ms(&conn).unwrap(),
        0
    );
    assert!(store::load_cursor_account_events(&conn).unwrap().is_empty());

    let kept = store::load_all(&conn).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].session_id, "sess-keep");
    assert_eq!(kept[0].total_tokens, 42);
}

const CURSOR_PAGE_ONE: &str = r#"{
    "totalUsageEventsCount": 3,
    "usageEventsDisplay": [
        {
            "timestamp": "1704067200000",
            "model": "claude-4.5-sonnet",
            "isHeadless": false,
            "tokenUsage": {
                "inputTokens": 100,
                "outputTokens": 50,
                "cacheReadTokens": 20,
                "cacheWriteTokens": 10
            }
        },
        {
            "timestamp": "1704153600000",
            "model": "composer-2",
            "isHeadless": true,
            "tokenUsage": {
                "inputTokens": 200,
                "outputTokens": 80,
                "cacheReadTokens": 0,
                "cacheWriteTokens": 5
            }
        }
    ]
}"#;

const CURSOR_PAGE_TWO: &str = r#"{
    "totalUsageEventsCount": 3,
    "usageEventsDisplay": [
        {
            "timestamp": "1704153600000",
            "model": "composer-2",
            "isHeadless": true,
            "tokenUsage": {
                "inputTokens": 200,
                "outputTokens": 80,
                "cacheReadTokens": 0,
                "cacheWriteTokens": 5
            }
        },
        {
            "timestamp": "1704240000000",
            "model": "gpt-5",
            "isHeadless": false,
            "tokenUsage": {
                "inputTokens": 30,
                "outputTokens": 10,
                "cacheReadTokens": 0,
                "cacheWriteTokens": 0
            }
        }
    ]
}"#;

#[test]
fn cursor_account_ingest_dedups_overlapping_pages() {
    let conn = store::open_memory().unwrap();
    let written =
        crate::cursor_account::ingest_raw_pages(&conn, &[CURSOR_PAGE_ONE, CURSOR_PAGE_TWO])
            .unwrap();
    assert_eq!(written, 3);

    let dto = crate::cursor_account::load_summary(&conn).unwrap();
    assert_eq!(dto.event_count, 3);
    assert_eq!(dto.input_tokens, 330);
    assert_eq!(dto.output_tokens, 140);
    assert_eq!(dto.cache_read_tokens, 20);
    assert_eq!(dto.cache_creation_tokens, 15);
    assert_eq!(dto.total_tokens, 505);
}

#[test]
fn cursor_account_incremental_ingest_only_adds_new_events() {
    let conn = store::open_memory().unwrap();
    assert_eq!(
        crate::cursor_account::incremental_start_ms(&conn).unwrap(),
        0
    );

    let first = crate::cursor_account::ingest_raw_pages(&conn, &[CURSOR_PAGE_ONE]).unwrap();
    assert_eq!(first, 2);
    assert_eq!(
        crate::cursor_account::incremental_start_ms(&conn).unwrap(),
        1_704_153_600_000
    );

    let second = crate::cursor_account::ingest_raw_pages(&conn, &[CURSOR_PAGE_TWO]).unwrap();
    assert_eq!(second, 1);
    assert_eq!(
        crate::cursor_account::incremental_start_ms(&conn).unwrap(),
        1_704_240_000_000
    );

    let again = crate::cursor_account::ingest_raw_pages(&conn, &[CURSOR_PAGE_TWO]).unwrap();
    assert_eq!(again, 0);

    let dto = crate::cursor_account::load_summary(&conn).unwrap();
    assert_eq!(dto.event_count, 3);
    assert_eq!(dto.total_tokens, 505);
}

#[test]
fn cursor_account_failed_refresh_keeps_last_good_cache() {
    let conn = store::open_memory().unwrap();
    crate::cursor_account::ingest_raw_pages(&conn, &[CURSOR_PAGE_ONE]).unwrap();
    store::set_cursor_account_as_of(&conn, "2026-08-17T12:00:00+00:00").unwrap();
    let before = crate::cursor_account::load_summary(&conn).unwrap();
    assert_eq!(before.event_count, 2);
    assert_eq!(before.total_tokens, 465);

    let auth = crate::cursor_account::apply_fetched_pages(
        &conn,
        Err(crate::cursor_account::auth_expired_error()),
    );
    assert!(
        auth.as_ref().unwrap_err().contains("过期"),
        "{}",
        auth.unwrap_err()
    );
    let after_auth = crate::cursor_account::load_summary(&conn).unwrap();
    assert_eq!(after_auth.event_count, 2);
    assert_eq!(after_auth.total_tokens, 465);
    assert_eq!(
        after_auth.as_of.as_deref(),
        Some("2026-08-17T12:00:00+00:00")
    );

    let structure = crate::cursor_account::apply_fetched_pages(
        &conn,
        Ok(vec![r#"{"totalUsageEventsCount":9}"#.to_string()]),
    );
    assert!(
        structure.as_ref().unwrap_err().contains("结构已变更"),
        "{}",
        structure.unwrap_err()
    );
    assert_eq!(
        crate::cursor_account::load_summary(&conn)
            .unwrap()
            .event_count,
        2
    );

    let parse = crate::cursor_account::apply_fetched_pages(
        &conn,
        Ok(vec![CURSOR_PAGE_TWO.to_string(), "{not-json".to_string()]),
    );
    assert!(
        parse.as_ref().unwrap_err().contains("解析失败"),
        "{}",
        parse.unwrap_err()
    );
    let after_parse = crate::cursor_account::load_summary(&conn).unwrap();
    assert_eq!(after_parse.event_count, 2);
    assert_eq!(after_parse.total_tokens, 465);
    assert_eq!(
        after_parse.as_of.as_deref(),
        Some("2026-08-17T12:00:00+00:00")
    );

    let network = crate::cursor_account::apply_fetched_pages(
        &conn,
        Err(crate::cursor_account::network_failure_error()),
    );
    assert!(
        network.as_ref().unwrap_err().contains("网络"),
        "{}",
        network.unwrap_err()
    );
    assert_eq!(
        crate::cursor_account::load_summary(&conn)
            .unwrap()
            .event_count,
        2
    );

    let empty = crate::cursor_account::apply_fetched_pages(
        &conn,
        Ok(vec![r#"{"usageEventsDisplay":[]}"#.to_string()]),
    )
    .unwrap();
    assert_eq!(empty.event_count, 2);
    assert_eq!(empty.total_tokens, 465);
}

#[test]
fn cursor_session_adapter_counts_turns_tools_and_status() {
    let parsed = crate::adapters::cursor_session::parse_cursor_session_transcript(&fixture(
        "cursor-session-transcript.jsonl",
    ))
    .expect("fixture should parse");
    assert_eq!(parsed.turn_count, 2);
    assert_eq!(parsed.success_count, 1);
    assert_eq!(parsed.error_count, 1);
    assert_eq!(parsed.aborted_count, 0);
    assert_eq!(parsed.tool_calls.get("Read"), Some(&1));
    assert_eq!(parsed.tool_calls.get("Shell"), Some(&1));
}

fn seed_cursor_transcript(
    home: &std::path::Path,
    project_slug: &str,
    session_id: &str,
    content: &str,
) -> std::path::PathBuf {
    let path = home.join(format!(
        ".cursor/projects/{project_slug}/agent-transcripts/{session_id}/{session_id}.jsonl"
    ));
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
    std::fs::write(&path, content).expect("write transcript");
    path
}

#[test]
fn cursor_session_ingest_summarize_does_not_touch_usage_records() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    seed_cursor_transcript(
        home,
        "Users-test-project",
        "sess-1",
        &fixture("cursor-session-transcript.jsonl"),
    );

    let conn = store::open_memory().unwrap();
    let mut report = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut report);
    assert_eq!(report.files_parsed, 1);
    assert!(store::load_all(&conn).unwrap().is_empty());

    let summary = crate::cursor_session::load_summary(&conn).unwrap();
    assert_eq!(summary.session_count, 1);
    assert_eq!(summary.turn_count, 2);
    assert_eq!(summary.error_rate, Some(0.5));
    assert_eq!(summary.active_project_count, 1);
    assert_eq!(summary.by_project.len(), 1);
    assert_eq!(summary.by_project[0].name, "/Users/test/project");
    assert_eq!(summary.by_project[0].session_count, 1);
    assert_eq!(summary.by_project[0].turn_count, 2);
    assert_eq!(summary.daily.len(), 1);
    assert_eq!(summary.daily[0].session_count, 1);
    assert_eq!(summary.daily[0].turn_count, 2);
}

#[test]
fn cursor_session_ingest_skips_unchanged_transcripts() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    seed_cursor_transcript(
        home,
        "Users-test-project",
        "sess-1",
        &fixture("cursor-session-transcript.jsonl"),
    );

    let conn = store::open_memory().unwrap();
    let mut first = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut first);
    assert_eq!(first.files_parsed, 1);
    assert_eq!(first.files_skipped, 0);

    let mut second = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut second);
    assert_eq!(second.files_parsed, 0);
    assert_eq!(second.files_skipped, 1);
}

#[test]
fn cursor_session_ingest_reconciles_deleted_transcripts() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let path = seed_cursor_transcript(
        home,
        "Users-test-project",
        "sess-1",
        &fixture("cursor-session-transcript.jsonl"),
    );

    let conn = store::open_memory().unwrap();
    let mut report = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut report);
    assert_eq!(
        crate::cursor_session::load_summary(&conn)
            .unwrap()
            .session_count,
        1
    );

    std::fs::remove_file(path).expect("remove transcript");
    let mut again = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut again);
    assert_eq!(
        crate::cursor_session::load_summary(&conn)
            .unwrap()
            .session_count,
        0
    );
    assert_eq!(again.records_removed, 1);
}

#[test]
fn cursor_session_ingest_skips_reconcile_when_parse_failed() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let path_one = seed_cursor_transcript(
        home,
        "Users-test-project",
        "sess-1",
        &fixture("cursor-session-transcript.jsonl"),
    );
    let path_two = seed_cursor_transcript(
        home,
        "Users-test-project",
        "sess-2",
        &fixture("cursor-session-transcript.jsonl"),
    );

    let conn = store::open_memory().unwrap();
    let mut first = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut first);
    assert_eq!(
        crate::cursor_session::load_summary(&conn)
            .unwrap()
            .session_count,
        2
    );

    std::fs::remove_file(path_one).expect("remove first transcript");
    std::fs::write(&path_two, "{not-json").expect("corrupt second transcript");
    let mut failed = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut failed);
    assert_eq!(failed.files_failed, 1);
    assert_eq!(
        crate::cursor_session::load_summary(&conn)
            .unwrap()
            .session_count,
        2,
        "reconcile should be skipped while a transcript parse fails"
    );

    std::fs::write(&path_two, fixture("cursor-session-transcript.jsonl")).expect("fix transcript");
    let mut clean = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut clean);
    assert_eq!(clean.files_failed, 0);
    assert_eq!(
        crate::cursor_session::load_summary(&conn)
            .unwrap()
            .session_count,
        1
    );
    assert_eq!(clean.records_removed, 1);
}

#[test]
fn cursor_session_parse_failure_keeps_last_good_cache() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let path = seed_cursor_transcript(
        home,
        "Users-test-project",
        "sess-1",
        &fixture("cursor-session-transcript.jsonl"),
    );

    let conn = store::open_memory().unwrap();
    let mut report = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut report);
    assert_eq!(
        crate::cursor_session::load_summary(&conn)
            .unwrap()
            .turn_count,
        2
    );

    std::fs::write(&path, "{not-json").expect("write bad json");
    let mut bad = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut bad);
    assert_eq!(bad.files_failed, 1);
    assert_eq!(
        crate::cursor_session::load_summary(&conn)
            .unwrap()
            .turn_count,
        2
    );
}

fn seed_ai_code_hashes(home: &std::path::Path, rows: &[(&str, &str, i64, &str)]) {
    let db_path = home.join(".cursor/ai-tracking/ai-code-tracking.db");
    std::fs::create_dir_all(db_path.parent().expect("parent")).expect("create dirs");
    let conn = rusqlite::Connection::open(&db_path).expect("open tracking db");
    conn.execute_batch(
        r#"
        CREATE TABLE ai_code_hashes (
            hash TEXT,
            source TEXT,
            fileExtension TEXT,
            fileName TEXT,
            requestId TEXT,
            conversationId TEXT,
            timestamp INTEGER,
            createdAt INTEGER,
            model TEXT
        );
        "#,
    )
    .expect("create table");
    for (conversation_id, model, timestamp, file_name) in rows {
        conn.execute(
            r#"
            INSERT INTO ai_code_hashes(
                hash, source, fileExtension, fileName, requestId,
                conversationId, timestamp, createdAt, model
            ) VALUES (?1, 'composer', 'rs', ?2, 'req', ?3, ?4, ?4, ?5)
            "#,
            rusqlite::params![
                format!("hash-{conversation_id}-{file_name}"),
                file_name,
                conversation_id,
                timestamp,
                model
            ],
        )
        .expect("insert hash");
    }
}

#[test]
fn cursor_session_enriches_from_ai_code_hashes() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    seed_cursor_transcript(
        home,
        "Users-test-project",
        "sess-1",
        &fixture("cursor-session-transcript.jsonl"),
    );
    seed_ai_code_hashes(home, &[("sess-1", "grok-4.6", 1_784_511_794_686, "lib.rs")]);

    let conn = store::open_memory().unwrap();
    let mut report = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut report);

    let sessions = store::load_cursor_sessions(&conn).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].files_touched, 1);
    assert!(sessions[0].models_json.contains("grok-4.6"));
    assert!(sessions[0]
        .first_seen_at
        .as_deref()
        .unwrap()
        .contains("2026"));

    let summary = crate::cursor_session::load_summary(&conn).unwrap();
    assert_eq!(summary.by_model.len(), 1);
    assert_eq!(summary.by_model[0].name, "grok-4.6");
    assert_eq!(summary.by_model[0].session_count, 1);
    assert_eq!(summary.top_tools.len(), 2);
    assert_eq!(summary.top_tools[0].name, "Read");
    assert_eq!(summary.top_tools[0].call_count, 1);
}

#[test]
fn cursor_session_transcript_without_hash_stays_counted() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    seed_cursor_transcript(
        home,
        "Users-test-project",
        "sess-1",
        &fixture("cursor-session-transcript.jsonl"),
    );

    let conn = store::open_memory().unwrap();
    let mut report = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut report);

    let sessions = store::load_cursor_sessions(&conn).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].models_json, "[]");
    assert_eq!(sessions[0].files_touched, 0);
    assert_eq!(
        crate::cursor_session::load_summary(&conn)
            .unwrap()
            .session_count,
        1
    );
}

#[test]
fn cursor_session_orphan_hash_does_not_create_session() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    seed_ai_code_hashes(
        home,
        &[("orphan-only", "grok-4.6", 1_784_511_794_686, "lib.rs")],
    );

    let conn = store::open_memory().unwrap();
    let mut report = crate::domain::IngestReport::default();
    crate::cursor_session::ingest(&conn, home, &mut report);

    assert!(store::load_cursor_sessions(&conn).unwrap().is_empty());
    assert_eq!(
        crate::cursor_session::load_summary(&conn)
            .unwrap()
            .session_count,
        0
    );
}
