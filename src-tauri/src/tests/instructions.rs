fn instruction_source<'a>(
    dto: &'a crate::domain::GlobalInstructionDto,
    source: &str,
) -> &'a crate::domain::GlobalInstructionSourceRow {
    dto.sources
        .iter()
        .find(|row| row.source == source)
        .unwrap_or_else(|| panic!("missing source {source}"))
}

fn file_named<'a>(
    row: &'a crate::domain::GlobalInstructionSourceRow,
    display_path: &str,
) -> &'a crate::domain::GlobalInstructionFile {
    row.files
        .iter()
        .find(|file| file.display_path == display_path)
        .unwrap_or_else(|| panic!("missing file {display_path}"))
}

#[test]
fn scan_lists_claude_main_file_and_user_instruction_files() {
    let home = tempfile::tempdir().unwrap();
    let claude = home.path().join(".claude");
    let user_dir = claude.join("rules");
    std::fs::create_dir_all(&user_dir).unwrap();
    std::fs::write(claude.join("CLAUDE.md"), "prefer-chinese\n").unwrap();
    std::fs::write(user_dir.join("routing.md"), "# routing\n").unwrap();
    std::fs::write(user_dir.join("skills.md"), "# skills\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        Some(home.path().join("proj").as_path()),
        &crate::domain::InstructionUsageSummary::default(),
    );

    let claude = instruction_source(&dto, "claude");
    assert_eq!(claude.application, "Claude");
    let main = file_named(claude, "~/.claude/CLAUDE.md");
    assert_eq!(main.byte_size, 15);
    assert_eq!(main.content, "prefer-chinese\n");
    assert_eq!(
        main.load_status,
        crate::domain::InstructionLoadStatus::Loaded
    );
    assert!(main.modified_at.as_deref().is_some_and(|t| !t.is_empty()));
    let routing = file_named(claude, "~/.claude/rules/routing.md");
    assert_eq!(routing.content, "# routing\n");
    assert_eq!(
        routing.load_status,
        crate::domain::InstructionLoadStatus::Loaded
    );
    assert_eq!(
        file_named(claude, "~/.claude/rules/skills.md").content,
        "# skills\n"
    );
}

#[test]
fn scan_lists_claude_rules_directory_when_present() {
    let home = tempfile::tempdir().unwrap();
    let claude_rules = home.path().join(".claude/rules");
    let codex_rules = home.path().join(".codex/rules");
    std::fs::create_dir_all(&claude_rules).unwrap();
    std::fs::create_dir_all(&codex_rules).unwrap();
    std::fs::write(home.path().join(".claude/CLAUDE.md"), "ok\n").unwrap();
    std::fs::write(claude_rules.join("routing.md"), "# routing\n").unwrap();
    std::fs::write(codex_rules.join("default.rules"), "third-party\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );

    let claude_dir = file_named(instruction_source(&dto, "claude"), "~/.claude/rules/");
    assert_eq!(
        claude_dir.kind,
        crate::domain::InstructionEntryKind::Directory
    );
    assert_eq!(
        claude_dir.load_status,
        crate::domain::InstructionLoadStatus::Loaded
    );
    assert_eq!(claude_dir.abs_path, claude_rules.to_string_lossy());
    assert!(instruction_source(&dto, "codex")
        .files
        .iter()
        .all(|file| file.kind != crate::domain::InstructionEntryKind::Directory));
}

#[test]
fn scan_marks_missing_claude_main_file_not_created() {
    let home = tempfile::tempdir().unwrap();
    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let files = &instruction_source(&dto, "claude").files;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].display_path, "~/.claude/CLAUDE.md");
    assert_eq!(
        files[0].load_status,
        crate::domain::InstructionLoadStatus::NotCreated
    );
    assert_eq!(files[0].byte_size, 0);
    assert_eq!(files[0].content, "");
    assert!(files[0].modified_at.is_none());
}

#[test]
fn scan_ignores_reserved_project_and_usage_for_now() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    std::fs::write(home.path().join(".claude/CLAUDE.md"), "same\n").unwrap();
    let empty = crate::domain::InstructionUsageSummary::default();
    let populated = crate::domain::InstructionUsageSummary {
        sources: vec![crate::domain::InstructionSourceUsage {
            source: "claude".into(),
            total_tokens: 99_000,
        }],
    };
    let without_project = crate::instructions::scan(home.path(), None, &empty);
    let with_both = crate::instructions::scan(
        home.path(),
        Some(home.path().join("other-project").as_path()),
        &populated,
    );
    assert_eq!(without_project, with_both);
}

#[test]
fn scan_codex_override_shields_base_agents_file() {
    let home = tempfile::tempdir().unwrap();
    let codex = home.path().join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(codex.join("AGENTS.md"), "base-instruction\n").unwrap();
    std::fs::write(codex.join("AGENTS.override.md"), "override-instruction\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let row = instruction_source(&dto, "codex");
    let base = file_named(row, "~/.codex/AGENTS.md");
    let over = file_named(row, "~/.codex/AGENTS.override.md");
    assert_eq!(
        over.load_status,
        crate::domain::InstructionLoadStatus::Loaded
    );
    assert_eq!(over.content, "override-instruction\n");
    assert_eq!(
        base.load_status,
        crate::domain::InstructionLoadStatus::PresentUnloaded
    );
    assert_eq!(base.content, "base-instruction\n");
    assert!(base
        .note
        .as_deref()
        .is_some_and(|note| note.contains("AGENTS.override.md")));
}

#[test]
fn scan_codex_rules_dir_is_present_unloaded() {
    let home = tempfile::tempdir().unwrap();
    let rules = home.path().join(".codex/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(rules.join("default.rules"), "third-party\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let extra = file_named(
        instruction_source(&dto, "codex"),
        "~/.codex/rules/default.rules",
    );
    assert_eq!(
        extra.load_status,
        crate::domain::InstructionLoadStatus::PresentUnloaded
    );
    assert_eq!(extra.content, "third-party\n");
    assert!(extra
        .note
        .as_deref()
        .is_some_and(|note| note.contains("第三方")));
}

#[test]
fn scan_gemini_missing_file_is_not_created_not_absent() {
    let home = tempfile::tempdir().unwrap();
    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let files = &instruction_source(&dto, "gemini").files;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].display_path, "~/.gemini/GEMINI.md");
    assert_eq!(
        files[0].load_status,
        crate::domain::InstructionLoadStatus::NotCreated
    );
}

#[test]
fn scan_cursor_account_preference_is_locally_invisible() {
    let home = tempfile::tempdir().unwrap();
    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let files = &instruction_source(&dto, "cursor").files;
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].load_status,
        crate::domain::InstructionLoadStatus::LocallyInvisible
    );
    assert!(files[0]
        .note
        .as_deref()
        .is_some_and(|note| note.contains("账号服务端")));
    assert_eq!(files[0].action.as_deref(), Some("cursor_settings"));
}

#[test]
fn scan_covers_every_supported_source() {
    let home = tempfile::tempdir().unwrap();
    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let names: Vec<&str> = dto.sources.iter().map(|row| row.source.as_str()).collect();
    assert_eq!(
        names,
        [
            "claude",
            "codex",
            "gemini",
            "cursor",
            "pi",
            "opencode",
            "kimi",
            "dsh",
            "grok",
            "qwen",
            "factory",
            "cursor_agent",
            "copilot",
        ]
    );
    for row in &dto.sources {
        assert!(!row.files.is_empty(), "{} should not be absent", row.source);
    }
}

#[test]
fn scan_remaining_sources_use_documented_evidence() {
    let home = tempfile::tempdir().unwrap();
    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );

    let pi = file_named(instruction_source(&dto, "pi"), "~/.pi/agent/AGENTS.md");
    assert_eq!(pi.evidence, crate::domain::InstructionEvidence::Verified);
    assert_eq!(
        pi.load_status,
        crate::domain::InstructionLoadStatus::NotCreated
    );

    let opencode = file_named(
        instruction_source(&dto, "opencode"),
        "~/.config/opencode/AGENTS.md",
    );
    assert_eq!(
        opencode.evidence,
        crate::domain::InstructionEvidence::Verified
    );
    assert_eq!(
        opencode.load_status,
        crate::domain::InstructionLoadStatus::NotCreated
    );

    let kimi = &instruction_source(&dto, "kimi").files[0];
    assert_eq!(
        kimi.evidence,
        crate::domain::InstructionEvidence::NoMechanism
    );
    assert!(kimi.abs_path.is_empty());
    assert!(
        !kimi.display_path.contains('/'),
        "无机制条目不得给出可创建的假路径"
    );

    let dsh = file_named(instruction_source(&dto, "dsh"), "~/.dsh/AGENTS.md");
    assert_eq!(dsh.evidence, crate::domain::InstructionEvidence::Verified);

    let grok = file_named(instruction_source(&dto, "grok"), "~/.grok/AGENTS.md");
    assert_eq!(grok.evidence, crate::domain::InstructionEvidence::Verified);

    let qwen = file_named(instruction_source(&dto, "qwen"), "~/.qwen/QWEN.md");
    assert_eq!(qwen.evidence, crate::domain::InstructionEvidence::Verified);

    let factory = file_named(instruction_source(&dto, "factory"), "~/.factory/AGENTS.md");
    assert_eq!(
        factory.evidence,
        crate::domain::InstructionEvidence::Verified
    );

    let cursor_agent = &instruction_source(&dto, "cursor_agent").files[0];
    assert_eq!(
        cursor_agent.evidence,
        crate::domain::InstructionEvidence::Inferred
    );
    assert_eq!(
        cursor_agent.load_status,
        crate::domain::InstructionLoadStatus::LocallyInvisible
    );
    assert!(cursor_agent.action.is_none());
    assert!(
        cursor_agent
            .note
            .as_deref()
            .is_some_and(|note| note.contains("推测")),
        "推测条目必须说明尚未证实"
    );

    let copilot = file_named(
        instruction_source(&dto, "copilot"),
        "~/.copilot/copilot-instructions.md",
    );
    assert_eq!(
        copilot.evidence,
        crate::domain::InstructionEvidence::Verified
    );
}

#[test]
fn scan_reads_verified_remaining_instruction_files() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".pi/agent")).unwrap();
    std::fs::write(home.path().join(".pi/agent/AGENTS.md"), "pi-global\n").unwrap();
    std::fs::create_dir_all(home.path().join(".config/opencode")).unwrap();
    std::fs::write(
        home.path().join(".config/opencode/AGENTS.md"),
        "opencode-global\n",
    )
    .unwrap();
    std::fs::create_dir_all(home.path().join(".qwen")).unwrap();
    std::fs::write(home.path().join(".qwen/QWEN.md"), "qwen-global\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    assert_eq!(
        file_named(instruction_source(&dto, "pi"), "~/.pi/agent/AGENTS.md").content,
        "pi-global\n"
    );
    assert_eq!(
        file_named(
            instruction_source(&dto, "opencode"),
            "~/.config/opencode/AGENTS.md"
        )
        .load_status,
        crate::domain::InstructionLoadStatus::Loaded
    );
    assert_eq!(
        file_named(instruction_source(&dto, "qwen"), "~/.qwen/QWEN.md").content,
        "qwen-global\n"
    );
}

#[test]
fn scan_pi_override_shields_base_agents_file() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".pi/agent");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("AGENTS.md"), "base-pi\n").unwrap();
    std::fs::write(dir.join("AGENTS.override.md"), "override-pi\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let row = instruction_source(&dto, "pi");
    let base = file_named(row, "~/.pi/agent/AGENTS.md");
    let over = file_named(row, "~/.pi/agent/AGENTS.override.md");
    assert_eq!(
        over.load_status,
        crate::domain::InstructionLoadStatus::Loaded
    );
    assert_eq!(over.content, "override-pi\n");
    assert_eq!(
        base.load_status,
        crate::domain::InstructionLoadStatus::PresentUnloaded
    );
    assert_eq!(base.content, "base-pi\n");
}

fn checkup_named<'a>(
    dto: &'a crate::domain::GlobalInstructionDto,
    kind: crate::domain::InstructionCheckupKind,
    display_path: &str,
) -> &'a crate::domain::InstructionCheckupFinding {
    dto.findings
        .iter()
        .find(|finding| finding.kind == kind && finding.display_path == display_path)
        .unwrap_or_else(|| panic!("missing finding {kind:?} {display_path}"))
}

#[test]
fn scan_reports_empty_loaded_file() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".gemini")).unwrap();
    std::fs::write(home.path().join(".gemini/GEMINI.md"), "").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let finding = checkup_named(
        &dto,
        crate::domain::InstructionCheckupKind::Empty,
        "~/.gemini/GEMINI.md",
    );
    assert_eq!(
        finding.severity,
        crate::domain::InstructionCheckupSeverity::High
    );
    assert!(finding.problem.contains("0") || finding.problem.contains("空"));
    assert!(!finding.consequence.is_empty());
}

#[test]
fn scan_does_not_report_empty_when_file_has_bytes() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".gemini")).unwrap();
    std::fs::write(home.path().join(".gemini/GEMINI.md"), "prefer-tabs\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    assert!(dto
        .findings
        .iter()
        .all(|finding| { finding.kind != crate::domain::InstructionCheckupKind::Empty }));
}

#[test]
fn scan_does_not_report_empty_for_unloaded_zero_byte_file() {
    let home = tempfile::tempdir().unwrap();
    let rules = home.path().join(".codex/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(rules.join("default.rules"), "").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    assert!(dto
        .findings
        .iter()
        .all(|finding| { finding.kind != crate::domain::InstructionCheckupKind::Empty }));
    checkup_named(
        &dto,
        crate::domain::InstructionCheckupKind::PresentUnloaded,
        "~/.codex/rules/default.rules",
    );
}

#[test]
fn scan_reports_present_unloaded_leftover() {
    let home = tempfile::tempdir().unwrap();
    let rules = home.path().join(".codex/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(rules.join("default.rules"), "third-party\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let finding = checkup_named(
        &dto,
        crate::domain::InstructionCheckupKind::PresentUnloaded,
        "~/.codex/rules/default.rules",
    );
    assert_eq!(
        finding.severity,
        crate::domain::InstructionCheckupSeverity::High
    );
    assert!(finding.problem.contains("不会加载"));
    assert!(finding.consequence.contains("不会改变"));
}

#[test]
fn scan_does_not_report_present_unloaded_for_loaded_file() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".codex")).unwrap();
    std::fs::write(home.path().join(".codex/AGENTS.md"), "base\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    assert!(dto
        .findings
        .iter()
        .all(|finding| { finding.kind != crate::domain::InstructionCheckupKind::PresentUnloaded }));
}

#[test]
fn scan_reports_override_shielding_base_file() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".codex");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("AGENTS.md"), "base\n").unwrap();
    std::fs::write(dir.join("AGENTS.override.md"), "override\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let finding = checkup_named(
        &dto,
        crate::domain::InstructionCheckupKind::OverrideShields,
        "~/.codex/AGENTS.md",
    );
    assert_eq!(
        finding.severity,
        crate::domain::InstructionCheckupSeverity::Medium
    );
    assert!(finding.problem.contains("屏蔽"));
    assert!(finding.consequence.contains("不会生效") || finding.consequence.contains("覆盖"));
    assert!(dto.findings.iter().all(|item| {
        item.kind != crate::domain::InstructionCheckupKind::PresentUnloaded
            || item.display_path != "~/.codex/AGENTS.md"
    }));
}

#[test]
fn scan_does_not_report_override_when_only_base_exists() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".codex")).unwrap();
    std::fs::write(home.path().join(".codex/AGENTS.md"), "base\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    assert!(dto
        .findings
        .iter()
        .all(|finding| { finding.kind != crate::domain::InstructionCheckupKind::OverrideShields }));
}

#[test]
fn scan_reports_over_limit_when_loaded_bytes_exceed_cap() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".codex")).unwrap();
    std::fs::write(
        home.path().join(".codex/AGENTS.md"),
        vec![b'a'; 32 * 1024 + 1],
    )
    .unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let finding = dto
        .findings
        .iter()
        .find(|item| item.kind == crate::domain::InstructionCheckupKind::OverLimit)
        .expect("over_limit");
    assert_eq!(
        finding.severity,
        crate::domain::InstructionCheckupSeverity::Critical
    );
    assert!(finding.problem.contains("超过"));
    assert!(finding.consequence.contains("截断"));
}

#[test]
fn scan_reports_near_limit_when_loaded_bytes_approach_cap() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".codex")).unwrap();
    std::fs::write(home.path().join(".codex/AGENTS.md"), vec![b'a'; 26 * 1024]).unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let finding = dto
        .findings
        .iter()
        .find(|item| item.kind == crate::domain::InstructionCheckupKind::NearLimit)
        .expect("near_limit");
    assert_eq!(
        finding.severity,
        crate::domain::InstructionCheckupSeverity::Low
    );
    assert!(finding.problem.contains("接近"));
    assert!(finding.consequence.contains("截断"));
}

#[test]
fn scan_reports_near_limit_when_loaded_bytes_equal_cap() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".codex")).unwrap();
    std::fs::write(home.path().join(".codex/AGENTS.md"), vec![b'a'; 32 * 1024]).unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    assert!(dto
        .findings
        .iter()
        .all(|finding| { finding.kind != crate::domain::InstructionCheckupKind::OverLimit }));
    assert!(dto
        .findings
        .iter()
        .any(|finding| { finding.kind == crate::domain::InstructionCheckupKind::NearLimit }));
}

#[test]
fn scan_does_not_report_limit_when_loaded_bytes_are_small() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".codex")).unwrap();
    std::fs::write(home.path().join(".codex/AGENTS.md"), "short\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    assert!(dto.findings.iter().all(|finding| {
        finding.kind != crate::domain::InstructionCheckupKind::NearLimit
            && finding.kind != crate::domain::InstructionCheckupKind::OverLimit
    }));
}

#[test]
fn scan_sorts_checkup_findings_by_severity() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".codex/rules")).unwrap();
    std::fs::create_dir_all(home.path().join(".gemini")).unwrap();
    std::fs::write(
        home.path().join(".codex/AGENTS.md"),
        vec![b'a'; 32 * 1024 + 8],
    )
    .unwrap();
    std::fs::write(home.path().join(".codex/rules/default.rules"), "left\n").unwrap();
    std::fs::write(home.path().join(".gemini/GEMINI.md"), "").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    let kinds: Vec<_> = dto.findings.iter().map(|finding| finding.kind).collect();
    assert_eq!(
        kinds,
        [
            crate::domain::InstructionCheckupKind::OverLimit,
            crate::domain::InstructionCheckupKind::Empty,
            crate::domain::InstructionCheckupKind::PresentUnloaded,
        ]
    );
}

#[test]
fn scan_emits_no_findings_when_loaded_files_are_healthy() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    std::fs::write(home.path().join(".claude/CLAUDE.md"), "prefer-chinese\n").unwrap();

    let dto = crate::instructions::scan(
        home.path(),
        None,
        &crate::domain::InstructionUsageSummary::default(),
    );
    assert!(dto.findings.is_empty());
}

fn file_mtime(path: &std::path::Path) -> String {
    let meta = std::fs::metadata(path).unwrap();
    chrono::DateTime::<chrono::Utc>::from(meta.modified().unwrap()).to_rfc3339()
}

#[test]
fn write_user_file_replaces_content_when_mtime_matches() {
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = home.path().join(".claude/CLAUDE.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "old\n").unwrap();
    let expected = file_mtime(&path);

    crate::user_files::write(
        home.path(),
        data.path(),
        &path,
        "new-content\n",
        Some(expected.as_str()),
    )
    .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new-content\n");
}

#[test]
fn write_user_file_rejects_stale_mtime_and_keeps_original() {
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = home.path().join(".claude/CLAUDE.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "keep-me\n").unwrap();

    let error = crate::user_files::write(
        home.path(),
        data.path(),
        &path,
        "stolen\n",
        Some("2000-01-01T00:00:00+00:00"),
    )
    .unwrap_err();

    assert!(error.contains("外部被修改"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep-me\n");
}

#[test]
fn write_user_file_backs_up_original_before_replace() {
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = home.path().join(".codex/AGENTS.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "before-backup\n").unwrap();
    let expected = file_mtime(&path);

    crate::user_files::write(
        home.path(),
        data.path(),
        &path,
        "after-backup\n",
        Some(expected.as_str()),
    )
    .unwrap();

    let backups: Vec<_> = std::fs::read_dir(data.path().join("instruction-backups"))
        .unwrap()
        .flatten()
        .flat_map(|entry| std::fs::read_dir(entry.path()).into_iter().flatten())
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("bak"))
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read_to_string(&backups[0]).unwrap(),
        "before-backup\n"
    );
}

#[test]
fn write_user_file_rejects_path_outside_allowlist() {
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = home.path().join(".codex/rules/default.rules");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "third-party\n").unwrap();

    let error = crate::user_files::write(
        home.path(),
        data.path(),
        &path,
        "nope\n",
        Some(file_mtime(&path).as_str()),
    )
    .unwrap_err();

    assert!(error.contains("可写名单"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "third-party\n");
}

#[test]
fn write_user_file_rejects_third_party_database() {
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = home
        .path()
        .join("Library/Application Support/Cursor/User/globalStorage/state.vscdb");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "db").unwrap();

    let error = crate::user_files::write(home.path(), data.path(), &path, "x", None).unwrap_err();
    assert!(error.contains("可写名单"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "db");
}

#[test]
fn write_user_file_rejects_parent_dir_name_in_allowlist() {
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = home.path().join(".claude/rules/../CLAUDE.md");
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    std::fs::write(home.path().join(".claude/CLAUDE.md"), "keep\n").unwrap();

    let error = crate::user_files::write(home.path(), data.path(), &path, "x\n", None).unwrap_err();
    assert!(error.contains("可写名单"));
    assert_eq!(
        std::fs::read_to_string(home.path().join(".claude/CLAUDE.md")).unwrap(),
        "keep\n"
    );
}

#[test]
fn open_target_uses_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("CLAUDE.md");
    std::fs::write(&path, "x\n").unwrap();
    assert_eq!(
        crate::instructions::resolve_open_path(path.to_str().unwrap()).unwrap(),
        path
    );
}

#[test]
fn open_target_uses_directory_when_path_is_dir() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules");
    std::fs::create_dir_all(&path).unwrap();
    assert_eq!(
        crate::instructions::resolve_open_path(path.to_str().unwrap()).unwrap(),
        path
    );
}

#[test]
fn open_target_falls_back_to_parent_when_file_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("CLAUDE.md");
    assert_eq!(
        crate::instructions::resolve_open_path(path.to_str().unwrap()).unwrap(),
        dir.path()
    );
}

#[test]
fn open_target_rejects_empty_path() {
    let error = crate::instructions::resolve_open_path("").unwrap_err();
    assert!(error.contains("没有可打开"));
}
