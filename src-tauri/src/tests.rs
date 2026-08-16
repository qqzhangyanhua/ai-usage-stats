use crate::adapters::cursor::{parse_cursor_commits, summarize_code_volume, CursorCommitRow};
use crate::adapters::opencode::{parse_opencode_messages, OpencodeMessage};
use crate::adapters::{claude, codex, dsh, factory, gemini, grok, kimi, pi, qwen};
use crate::aggregate;
use crate::cost::derive_cost;
use crate::domain::{Filter, PriceEntry, PriceTable, Source, UsageRecord};
use crate::ingest;
use crate::store;

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
    assert_eq!(records[0].project, "/Users/zhangyanhua/AI/chord-creator-studio");
    assert_eq!(records[0].session_id, "019a9618-5abf-7892-be63-df90ece3d676");
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
fn claude_adapter_maps_usage_and_project_dir() {
    let records = claude::parse_claude_jsonl(
        &fixture("claude.jsonl"),
        "/Users/zhangyanhua/.claude/projects/-Users-zhangyanhua-AI-TradingAgents-CN/04868551-34c3-4588-b984-6ae9a5d95f8a.jsonl",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].source, Source::Claude);
    assert_eq!(records[0].model, "claude-sonnet-5");
    assert_eq!(records[0].session_id, "04868551-34c3-4588-b984-6ae9a5d95f8a");
    assert_eq!(
        records[0].project,
        "/Users/zhangyanhua/AI/TradingAgents-CN"
    );
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
    assert_eq!(records[0].project, "/Users/zhangyanhua/workCode/ruoyi-ui-vue3");
    assert_eq!(records[0].session_id, "019f5abc-b360-79e4-bd7d-9a794da8cfc5");
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
    assert_eq!(records[0].project, "/Users/zhangyanhua/workCode/project_front");
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
fn kimi_adapter_keeps_last_status_update_per_turn() {
    let records = kimi::parse_kimi_wire(
        &fixture("kimi-wire.jsonl"),
        "/Users/zhangyanhua/.kimi/sessions/hash/bd1ab6fc-768d-4cff-b4c4-221a583c3af8/wire.jsonl",
        "/Users/zhangyanhua/workCode/app-storage",
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].source, Source::Kimi);
    assert_eq!(records[0].session_id, "bd1ab6fc-768d-4cff-b4c4-221a583c3af8");
    assert_eq!(records[0].project, "/Users/zhangyanhua/workCode/app-storage");
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
    assert_eq!(records[0].session_id, "session-f1cbbe01-e379-4152-8d13-46440f595d2d");
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
    assert_eq!(records[0].session_id, "2392a2f0-142a-407e-a08f-8f37781ba76c");
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
    assert_eq!(records[0].session_id, "9ab2ca7b-bd30-495b-9434-07892ee0e5e6");
    assert_eq!(records[0].input_tokens, 3);
    assert_eq!(records[0].output_tokens, 1022);
    assert_eq!(records[0].cache_creation_tokens, 8125);
    assert_eq!(records[0].cache_read_tokens, 11084);
    assert_eq!(records[0].reasoning_tokens, 0);
    assert_eq!(records[0].total_tokens, 20234);
}

#[test]
fn factory_adapter_root_settings_have_empty_project() {
    let records = factory::parse_factory_settings(
        &fixture("factory.settings.json"),
        "/Users/zhangyanhua/.factory/sessions/9ab2ca7b-bd30-495b-9434-07892ee0e5e6.settings.json",
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].project, "");
    assert_eq!(records[0].session_id, "9ab2ca7b-bd30-495b-9434-07892ee0e5e6");
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
    assert_eq!(days[1].bucket, "2026-08-02");
    assert_eq!(days[1].total_tokens, 300);
    assert_eq!(days[2].bucket, "2026-08-08");
    assert_eq!(days[2].total_tokens, 50);

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

    let by_provider = aggregate::by_name(&records, &Filter::default(), &PriceTable::default(), |r| {
        r.provider.clone()
    });
    assert_eq!(by_provider[0].name, "anthropic");

    let by_project = aggregate::by_name(&records, &Filter::default(), &PriceTable::default(), |r| {
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
    let top = aggregate::top_sessions(&records, &Filter::default(), &PriceTable::default(), 2);
    assert_eq!(top[0].session_id, "s2");
    assert_eq!(top[0].total_tokens, 300);
    assert_eq!(top[1].session_id, "s1");
    assert_eq!(top[1].total_tokens, 120);
    assert_eq!(top[1].source_file, "/s1.jsonl");
    let turns = aggregate::session_turns(
        &records,
        "s1",
        Some("codex"),
        &Filter::default(),
        &PriceTable::default(),
    );
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].total_tokens, 100);
    assert_eq!(turns[1].total_tokens, 20);

    records.push(rec(
        "2026-08-01T12:00:00Z",
        Source::Claude,
        "claude-sonnet-5",
        "anthropic",
        "/proj/a",
        "s1",
        99,
    ));
    let same_id_other_source = aggregate::session_turns(
        &records,
        "s1",
        Some("codex"),
        &Filter::default(),
        &PriceTable::default(),
    );
    assert_eq!(same_id_other_source.len(), 2);

    let recent = Filter {
        from: Some("2026-08-01T10:30:00Z".into()),
        ..Filter::default()
    };
    let filtered = aggregate::session_turns(&records, "s1", Some("codex"), &recent, &PriceTable::default());
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].total_tokens, 20);
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
