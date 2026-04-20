use crate::commands::runtime_commands::RuntimeSessions;
use crate::db::Database;
use crate::models::{
    AutonomyMode, CheckKind, ConflictResolutionPolicy, GoalRun, GoalRunEventKind, GoalRunPhase,
    GoalRunStatus, GoalRunUpdate, PlanStatus, PlanTask, Project, ProjectRuntimeSession,
    ProjectRuntimeSpec, ProjectSettings, RuntimeReadinessCheck, RuntimeSessionStatus,
    RuntimeStopBehavior, TaskPriority, TaskStatus, VerificationCheck, VerificationResult,
    WorkPlan, WorkPlanUpdate,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub struct TestTools;

static TEST_TOOLS: OnceLock<TestTools> = OnceLock::new();

pub fn ensure_test_tools() -> &'static TestTools {
    TEST_TOOLS.get_or_init(|| {
        let root = std::env::temp_dir().join(format!(
            "project-builder-test-tools-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create test tools root");

        let real_git = find_real_git();
        write_script(
            &root.join("git"),
            &format!(
                "#!/bin/sh\n\
                 case \"$1\" in\n\
                   commit)\n\
                     case \"$PWD\" in\n\
                       *rollback*)\n\
                         echo \"simulated git commit failure\" >&2\n\
                         exit 1\n\
                         ;;\n\
                     esac\n\
                     ;;\n\
                 esac\n\
                 export GIT_AUTHOR_NAME=\"${{GIT_AUTHOR_NAME:-Test User}}\"\n\
                 export GIT_AUTHOR_EMAIL=\"${{GIT_AUTHOR_EMAIL:-test@example.com}}\"\n\
                 export GIT_COMMITTER_NAME=\"${{GIT_COMMITTER_NAME:-Test User}}\"\n\
                 export GIT_COMMITTER_EMAIL=\"${{GIT_COMMITTER_EMAIL:-test@example.com}}\"\n\
                 exec '{}' \"$@\"\n",
                real_git.display()
            ),
        );

        write_script(
            &root.join("codex"),
            "#!/bin/sh\n\
             if [ -f \"$PWD/.fake-codex-fail\" ]; then\n\
               echo \"simulated codex failure\" >&2\n\
               exit 13\n\
             fi\n\
             printf '%s\\n' \"fake codex run\" >> \"$PWD/generated-from-codex.txt\"\n\
             exit 0\n",
        );

        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![root.clone()];
        paths.extend(std::env::split_paths(&path));
        let new_path = std::env::join_paths(paths).expect("join PATH for test helpers");
        std::env::set_var("PATH", new_path);

        TestTools
    })
}

pub struct GoalRunScenarioFixture {
    pub db_path: PathBuf,
    pub working_dir: PathBuf,
    pub db: Mutex<Database>,
    pub runtime_sessions: Mutex<RuntimeSessions>,
    pub project: Project,
    pub piece: crate::models::Piece,
    pub plan: WorkPlan,
    pub task: PlanTask,
    pub goal_run: GoalRun,
    pub runtime_session: ProjectRuntimeSession,
    pub runtime_log_path: PathBuf,
}

impl GoalRunScenarioFixture {
    pub fn forced_fatal_retry_exhausted() -> Self {
        Self::build(ScenarioKind::ForcedFatalRetryExhausted)
    }

    pub fn stale_logs_vs_live_logs() -> Self {
        Self::build(ScenarioKind::StaleLogsVsLiveLogs)
    }

    pub fn repair_requested() -> Self {
        Self::build(ScenarioKind::RepairRequested)
    }

    pub fn repair_skipped() -> Self {
        Self::build(ScenarioKind::RepairSkipped)
    }

    pub fn clean_runtime_pass() -> Self {
        Self::build(ScenarioKind::CleanRuntimePass)
    }

    pub fn cleanup(self) {
        let root = self
            .db_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.db_path.clone());
        drop(self.db);
        drop(self.runtime_sessions);
        let _ = fs::remove_dir_all(root);
    }

    fn build(kind: ScenarioKind) -> Self {
        let root = std::env::temp_dir().join(format!(
            "project-builder-deterministic-scenario-{}-{}",
            kind.case_name(),
            uuid::Uuid::new_v4()
        ));
        let working_dir = root.join("work");
        let _ = fs::create_dir_all(&working_dir);

        let db_path = root.join("data.db");
        let db = Database::new_at_path(&db_path).expect("open scenario db");

        let runtime_spec = base_runtime_spec();
        let project = db
            .create_project_with_settings(
                "Scenario project",
                "Deterministic test fixture",
                create_project_settings(&working_dir, runtime_spec.clone()),
            )
            .expect("create scenario project");

        let piece = db
            .create_piece(&project.id, None, "Verification piece", 0.0, 0.0)
            .expect("create scenario piece");

        let plan = db
            .create_work_plan(&project.id, "Verify deterministic contracts")
            .expect("create scenario plan");

        let task = PlanTask {
            id: format!("task-{}", kind.case_name()),
            piece_id: piece.id.clone(),
            piece_name: piece.name.clone(),
            title: "Verify deterministic contracts".to_string(),
            description: "Exercise snapshot, event, and runtime-log contracts".to_string(),
            priority: TaskPriority::High,
            suggested_phase: "verification".to_string(),
            dependencies: vec![],
            status: TaskStatus::Pending,
            order: 0,
        };

        let plan = db
            .update_work_plan(
                &plan.id,
                &WorkPlanUpdate {
                    status: Some(PlanStatus::Approved),
                    tasks: Some(vec![task.clone()]),
                    summary: Some("Deterministic verification plan".to_string()),
                    ..Default::default()
                },
            )
            .expect("seed scenario work plan");

        let goal_run = db
            .create_goal_run(&project.id, "Verify deterministic feedback loop contracts")
            .expect("create scenario goal run");

        let runtime_log_path = root.join("runtime.log");
        let runtime_session = build_runtime_session(&kind, &runtime_log_path);

        write_runtime_log(
            &runtime_log_path,
            kind.runtime_log_file_lines(),
        )
        .expect("write scenario runtime log");

        {
            let db = &db;
            let _ = db
                .upsert_runtime_session(&project.id, Some(&goal_run.id), &runtime_session)
                .expect("seed runtime session");
        }

        let seeded_goal_run = seed_goal_run_state(&db, &goal_run.id, &kind, &plan, &piece, &task)
            .expect("seed goal run state");

        Self {
            db_path,
            working_dir,
            db: Mutex::new(db),
            runtime_sessions: Mutex::new(HashMap::new()),
            project,
            piece,
            plan,
            task,
            goal_run: seeded_goal_run,
            runtime_session,
            runtime_log_path,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ScenarioKind {
    ForcedFatalRetryExhausted,
    StaleLogsVsLiveLogs,
    RepairRequested,
    RepairSkipped,
    CleanRuntimePass,
}

impl ScenarioKind {
    fn case_name(self) -> &'static str {
        match self {
            ScenarioKind::ForcedFatalRetryExhausted => "forced-fatal-retry-exhausted",
            ScenarioKind::StaleLogsVsLiveLogs => "stale-logs-vs-live-logs",
            ScenarioKind::RepairRequested => "repair-requested",
            ScenarioKind::RepairSkipped => "repair-skipped",
            ScenarioKind::CleanRuntimePass => "clean-runtime-pass",
        }
    }

    fn runtime_log_file_lines(self) -> Vec<String> {
        match self {
            ScenarioKind::ForcedFatalRetryExhausted => numbered_lines(
                "runtime",
                15,
                Some("FATAL: forced for test"),
            ),
            ScenarioKind::StaleLogsVsLiveLogs => numbered_lines(
                "persisted",
                15,
                Some("WARNING: stale blocker from a prior run"),
            ),
            ScenarioKind::RepairRequested => vec![
                "runtime: operator requested repair".to_string(),
                "runtime: awaiting executor".to_string(),
            ],
            ScenarioKind::RepairSkipped => numbered_lines(
                "runtime",
                15,
                Some("FATAL: repair skipped after retry exhaustion"),
            ),
            ScenarioKind::CleanRuntimePass => vec![
                "runtime: server ready".to_string(),
                "runtime: healthy".to_string(),
            ],
        }
    }

    fn live_recent_logs(self) -> Vec<String> {
        match self {
            ScenarioKind::ForcedFatalRetryExhausted => numbered_lines(
                "runtime",
                15,
                Some("FATAL: forced for test"),
            ),
            ScenarioKind::StaleLogsVsLiveLogs => vec![
                "runtime: booting cleanly".to_string(),
                "runtime: fresh status ok".to_string(),
                "runtime: no warnings in live tail".to_string(),
            ],
            ScenarioKind::RepairRequested => vec![
                "runtime: repair requested".to_string(),
                "runtime: waiting for executor".to_string(),
            ],
            ScenarioKind::RepairSkipped => numbered_lines(
                "runtime",
                15,
                Some("FATAL: repair skipped after retry exhaustion"),
            ),
            ScenarioKind::CleanRuntimePass => vec![
                "runtime: server ready".to_string(),
                "runtime: healthy".to_string(),
            ],
        }
    }
}

fn base_runtime_spec() -> ProjectRuntimeSpec {
    ProjectRuntimeSpec {
        install_command: None,
        run_command: "node server.js".to_string(),
        readiness_check: RuntimeReadinessCheck::None,
        verify_command: None,
        stop_behavior: RuntimeStopBehavior::Kill,
        app_url: None,
        port_hint: None,
        acceptance_suite: None,
    }
}

fn create_project_settings(working_dir: &Path, runtime_spec: ProjectRuntimeSpec) -> ProjectSettings {
    ProjectSettings {
        llm_configs: vec![],
        default_token_budget: 100_000,
        autonomy_mode: AutonomyMode::Autopilot,
        phase_control: crate::models::PhaseControlPolicy::Manual,
        conflict_resolution: ConflictResolutionPolicy::AiAssisted,
        working_directory: Some(working_dir.display().to_string()),
        default_execution_engine: None,
        post_run_validation_command: None,
        runtime_spec: Some(runtime_spec),
    }
}

fn build_runtime_session(
    kind: &ScenarioKind,
    runtime_log_path: &Path,
) -> ProjectRuntimeSession {
    ProjectRuntimeSession {
        session_id: format!("runtime-{}", kind.case_name()),
        status: RuntimeSessionStatus::Running,
        started_at: Some("2024-01-01T00:00:00Z".to_string()),
        updated_at: "2024-01-01T00:00:01Z".to_string(),
        ended_at: None,
        url: None,
        port_hint: None,
        log_path: Some(runtime_log_path.display().to_string()),
        recent_logs: kind.live_recent_logs(),
        last_error: None,
        exit_code: None,
        pid: Some(12_345),
    }
}

fn seed_goal_run_state(
    db: &Database,
    goal_run_id: &str,
    kind: &ScenarioKind,
    plan: &WorkPlan,
    piece: &crate::models::Piece,
    task: &PlanTask,
) -> Result<GoalRun, String> {
    let (update, event_kind, event_summary, event_payload) = match kind {
        ScenarioKind::ForcedFatalRetryExhausted => (
            GoalRunUpdate {
                phase: Some(GoalRunPhase::Verification),
                status: Some(GoalRunStatus::Blocked),
                current_plan_id: Some(Some(plan.id.clone())),
                current_piece_id: Some(Some(piece.id.clone())),
                current_task_id: Some(Some(task.id.clone())),
                blocker_reason: Some(Some(
                    "log scan — fatal patterns: matched forbidden /(?i)FATAL/ on line 16"
                        .to_string(),
                )),
                verification_summary: Some(Some(
                    fatal_verification_result(
                        "log scan — fatal patterns: matched forbidden /(?i)FATAL/ on line 16",
                    ),
                )),
                retry_count: Some(3),
                last_failure_summary: Some(Some(
                    "log scan — fatal patterns: matched forbidden /(?i)FATAL/ on line 16"
                        .to_string(),
                )),
                last_failure_fingerprint: Some(Some("verification:log-scan:fatal-patterns".to_string())),
                attention_required: Some(true),
                ..Default::default()
            },
            GoalRunEventKind::Blocked,
            "Verification blocked by fatal log scan",
            None,
        ),
        ScenarioKind::StaleLogsVsLiveLogs => (
            GoalRunUpdate {
                phase: Some(GoalRunPhase::Verification),
                status: Some(GoalRunStatus::Running),
                current_plan_id: Some(Some(plan.id.clone())),
                current_piece_id: Some(Some(piece.id.clone())),
                current_task_id: Some(Some(task.id.clone())),
                verification_summary: Some(Some(clean_verification_result(
                    "1/1 checks passed",
                    "log scan remained clean",
                ))),
                ..Default::default()
            },
            GoalRunEventKind::Note,
            "Live logs should win over stale persisted logs",
            Some(serde_json::json!({
                "reason": "prefer-live-tail",
                "source": "runtime-session",
            })
            .to_string()),
        ),
        ScenarioKind::RepairRequested => (
            GoalRunUpdate {
                phase: Some(GoalRunPhase::Verification),
                status: Some(GoalRunStatus::Running),
                current_plan_id: Some(Some(plan.id.clone())),
                current_piece_id: Some(Some(piece.id.clone())),
                current_task_id: Some(Some(task.id.clone())),
                operator_repair_requested: Some(true),
                verification_summary: Some(Some(
                    fatal_verification_result(
                        "log scan — fatal patterns: matched forbidden /(?i)FATAL/ on line 16",
                    ),
                )),
                attention_required: Some(false),
                ..Default::default()
            },
            GoalRunEventKind::RepairRequested,
            "Repair requested by operator",
            Some(serde_json::json!({
                "reason": "operator-forced",
                "retryCount": 3,
            })
            .to_string()),
        ),
        ScenarioKind::RepairSkipped => (
            GoalRunUpdate {
                phase: Some(GoalRunPhase::Verification),
                status: Some(GoalRunStatus::Blocked),
                current_plan_id: Some(Some(plan.id.clone())),
                current_piece_id: Some(Some(piece.id.clone())),
                current_task_id: Some(Some(task.id.clone())),
                blocker_reason: Some(Some("CTO repair skipped: retry budget exhausted".to_string())),
                verification_summary: Some(Some(
                    fatal_verification_result(
                        "log scan — fatal patterns: matched forbidden /(?i)FATAL/ on line 16",
                    ),
                )),
                retry_count: Some(3),
                last_failure_summary: Some(Some(
                    "log scan — fatal patterns: matched forbidden /(?i)FATAL/ on line 16"
                        .to_string(),
                )),
                last_failure_fingerprint: Some(Some("verification:log-scan:fatal-patterns".to_string())),
                attention_required: Some(true),
                operator_repair_requested: Some(false),
                ..Default::default()
            },
            GoalRunEventKind::RepairSkipped,
            "Repair skipped because retry budget was exhausted",
            Some(serde_json::json!({
                "reason": "retry-budget-exhausted",
                "retryCount": 3,
            })
            .to_string()),
        ),
        ScenarioKind::CleanRuntimePass => (
            GoalRunUpdate {
                phase: Some(GoalRunPhase::Verification),
                status: Some(GoalRunStatus::Completed),
                current_plan_id: Some(Some(plan.id.clone())),
                current_piece_id: Some(Some(piece.id.clone())),
                current_task_id: Some(Some(task.id.clone())),
                verification_summary: Some(Some(clean_verification_result(
                    "1/1 checks passed",
                    "log scan remained clean",
                ))),
                retry_count: Some(0),
                blocker_reason: Some(None),
                last_failure_summary: Some(None),
                last_failure_fingerprint: Some(None),
                attention_required: Some(false),
                operator_repair_requested: Some(false),
                ..Default::default()
            },
            GoalRunEventKind::PhaseCompleted,
            "Verification completed",
            Some(serde_json::json!({
                "result": "passed",
            })
            .to_string()),
        ),
    };

    let goal_run = db.update_goal_run(goal_run_id, &update)?;
    let _ = db.append_goal_run_event(
        goal_run_id,
        GoalRunPhase::Verification,
        event_kind,
        event_summary,
        event_payload.as_deref(),
    )?;

    db.get_goal_run(goal_run_id)
}

fn clean_verification_result(message: &str, detail: &str) -> String {
    serde_json::to_string(&VerificationResult {
        passed: true,
        checks: vec![VerificationCheck {
            name: "log scan — fatal patterns".to_string(),
            kind: CheckKind::LogScan,
            passed: true,
            detail: detail.to_string(),
            duration_ms: 0,
            expected: Some(
                "no match for [/(?i)panic!?/, /(?i)FATAL/, /(?i)unhandled (rejection|exception)/, /ECONNREFUSED/] over last 200 lines"
                    .to_string(),
            ),
            actual: Some("0 matches over 2 lines".to_string()),
        }],
        started_at: "2024-01-01T00:00:00Z".to_string(),
        finished_at: "2024-01-01T00:00:01Z".to_string(),
        message: message.to_string(),
    })
    .expect("serialize verification result")
}

fn fatal_verification_result(message: &str) -> String {
    serde_json::to_string(&VerificationResult {
        passed: false,
        checks: vec![VerificationCheck {
            name: "log scan — fatal patterns".to_string(),
            kind: CheckKind::LogScan,
            passed: false,
            detail: "matched forbidden /(?i)FATAL/ on line 16".to_string(),
            duration_ms: 0,
            expected: Some(
                "no match for [/(?i)panic!?/, /(?i)FATAL/, /(?i)unhandled (rejection|exception)/, /ECONNREFUSED/] over last 200 lines"
                    .to_string(),
            ),
            actual: Some(
                "matched /(?i)FATAL/ on line 16:\n"
                    .to_string(),
            ),
        }],
        started_at: "2024-01-01T00:00:00Z".to_string(),
        finished_at: "2024-01-01T00:00:01Z".to_string(),
        message: message.to_string(),
    })
    .expect("serialize verification result")
}

fn write_runtime_log(path: &Path, lines: Vec<String>) -> Result<(), String> {
    fs::write(path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

fn numbered_lines(prefix: &str, count: usize, tail: Option<&str>) -> Vec<String> {
    let mut lines = (1..=count)
        .map(|idx| format!("{prefix}: line {idx}"))
        .collect::<Vec<_>>();
    if let Some(tail) = tail {
        lines.push(tail.to_string());
    }
    lines
}

fn find_real_git() -> PathBuf {
    let output = std::process::Command::new("which")
        .arg("git")
        .output()
        .expect("locate git");
    assert!(output.status.success(), "git must be available for tests");
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(!path.is_empty(), "git path should not be empty");
    PathBuf::from(path)
}

fn write_script(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write test helper script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).expect("stat helper script").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod helper script");
    }
}
