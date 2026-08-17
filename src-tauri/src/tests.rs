use crate::adapters::cursor::{parse_cursor_commits, summarize_code_volume, CursorCommitRow};
use crate::adapters::opencode::{parse_opencode_messages, OpencodeMessage};
use crate::adapters::{claude, codex, cursor_agent, dsh, factory, gemini, grok, kimi, pi, qwen};
use crate::aggregate;
use crate::billing_window;
use crate::cost::derive_cost;
use crate::domain::{Filter, PriceEntry, PriceTable, SessionQuery, Source, UsageRecord};
use crate::ingest;
use crate::query;
use crate::store;
use chrono::{Local, NaiveTime, TimeZone, Utc};

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
fn source_maps_to_user_facing_application_names() {
    assert_eq!(Source::Claude.application_name(), "Claude Code");
    assert_eq!(Source::Codex.application_name(), "Codex");
    assert_eq!(Source::Factory.application_name(), "Droid");
    assert_eq!(Source::Opencode.application_name(), "OpenCode");
    assert_eq!(Source::Dsh.application_name(), "DeepSeek Harness");
    assert_eq!(Source::CursorAgent.application_name(), "Cursor Agent");
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
            },
            PriceEntry {
                model: "gpt-5.5".into(),
                provider: None,
                input: 0.01,
                output: 0.0,
                cache_read: 0.0,
                cache_creation: 0.0,
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
        }],
    };

    let dto = aggregate::overview(&stored, &Filter::default(), &table);
    assert_eq!(dto.cost, Some(1.5));
    assert!(dto.unpriced);

    let priced_turns =
        aggregate::session_turns(&stored, "s1", Some("codex"), &Filter::default(), &table);
    assert_eq!(priced_turns[0].cost, Some(1.0));
    assert_eq!(priced_turns[0].cost_note, None);
    let unpriced_turns =
        aggregate::session_turns(&stored, "s2", Some("claude"), &Filter::default(), &table);
    assert_eq!(unpriced_turns[0].cost, None);
    assert!(unpriced_turns[0].unpriced);
    assert_eq!(unpriced_turns[0].cost_note.as_deref(), Some("单价未配置"));
    let native_turns =
        aggregate::session_turns(&stored, "s3", Some("pi"), &Filter::default(), &table);
    assert_eq!(native_turns[0].cost, Some(0.5));
    assert_eq!(native_turns[0].cost_note, None);

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
fn ingest_reconciles_records_after_a_source_file_is_deleted() {
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

    assert_eq!(report.records_removed, 2);
    assert!(store::load_all(&conn).unwrap().is_empty());
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
        let (cached_files, record_count, _) = store::source_cache_stats(&conn, source).unwrap();
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
    let paths: [(&str, &str); 7] = [
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
    assert_eq!(first.files_parsed, 9);
    assert_eq!(first.records_written, 14);
    let stored = store::load_all(&conn).unwrap();
    assert_eq!(stored.len(), 14);
    assert_eq!(stored.iter().map(|r| r.total_tokens).sum::<i64>(), 335997);
    assert_rollups_match_overview(&stored, &Filter::default());

    let second = ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(second.files_parsed, 0);
    assert_eq!(second.files_skipped, 9);
    assert_eq!(second.records_written, 0);
    let again = store::load_all(&conn).unwrap();
    assert_eq!(again.len(), 14);
    assert_eq!(again.iter().map(|r| r.total_tokens).sum::<i64>(), 335997);
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
            },
            PriceEntry {
                model: "claude-sonnet-5".into(),
                provider: None,
                input: 0.003,
                output: 0.015,
                cache_read: 0.001,
                cache_creation: 0.0,
            },
            PriceEntry {
                model: "gpt-5.5".into(),
                provider: Some("subapi".into()),
                input: 0.02,
                output: 0.0,
                cache_read: 0.0,
                cache_creation: 0.0,
            },
            PriceEntry {
                model: "gpt-5.5".into(),
                provider: None,
                input: 0.01,
                output: 0.0,
                cache_read: 0.0,
                cache_creation: 0.0,
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

    // trend 三种粒度
    for grain in ["day", "week", "month"] {
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

    let by_model: std::collections::HashMap<&str, &PriceEntry> =
        snapshot.entries.iter().map(|e| (e.model.as_str(), e)).collect();

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
        }],
    };
    let merged = crate::litellm::merge(&user, &snapshot);

    // 用户配置过的 gpt-4o 不被快照覆盖，只保留用户那条。
    let gpt: Vec<&PriceEntry> = merged.prices.iter().filter(|e| e.model == "gpt-4o").collect();
    assert_eq!(gpt.len(), 1);
    assert_eq!(gpt[0].input, 9.9);
    // 用户没配的模型由快照补齐。
    assert!(merged.prices.iter().any(|e| e.model == "claude-3-5-sonnet"));
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
    assert_eq!(derived.amount, Some(2.5 + 10.0));

    // 有来源自带费用时优先 native。
    let native = UsageRecord {
        native_cost: Some(4.2),
        ..record.clone()
    };
    assert_eq!(derive_cost(&native, &effective).amount, Some(4.2));

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
    assert!(derive_cost(&unknown, &effective).unpriced);
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
