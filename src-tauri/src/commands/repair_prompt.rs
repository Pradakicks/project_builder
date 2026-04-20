//! Assembles the CTO-repair user-turn prompt.
//!
//! Kept as a pure module so the prompt shape is snapshot-testable without
//! dragging in the executor's async surface. The input is a
//! [`PhaseFailureContext`]; the output is the string that becomes the final
//! `user` message appended to the CTO system prompt.
//!
//! Non-Verification callers construct contexts via `from_summary`, producing
//! an empty `failed_checks` / `passed_checks` pair. The renderer degrades to
//! the current minimal shape in that case — "Summary: ..." followed by the
//! action-block instructions — so those call sites are behavior-compatible.

use crate::models::{CheckKind, GoalRunPhase, PhaseFailureContext, VerificationCheck};

const MAX_FIELD_CHARS: usize = 400;
const MAX_LOG_BLOCK_CHARS: usize = 1_600;
const MAX_PASSED_LISTED: usize = 6;

/// Build the user-turn prompt sent to the CTO repair agent.
pub fn build_repair_prompt(phase: GoalRunPhase, ctx: &PhaseFailureContext) -> String {
    let phase_str = phase_token(&phase);

    let mut out = String::new();
    out.push_str(&format!(
        "The goal run has failed during the **{phase_str}** phase.\n\n"
    ));
    out.push_str(&format!("Summary: {}\n", ctx.summary.trim()));

    if let Some(role) = ctx.failing_role {
        out.push_str(&format!("Failing role: {}\n", role.as_str()));
    }

    if !ctx.failed_checks.is_empty() {
        out.push_str(&format!(
            "\nFailed checks ({}):\n",
            ctx.failed_checks.len()
        ));
        for (idx, check) in ctx.failed_checks.iter().enumerate() {
            out.push('\n');
            render_failed_check(&mut out, idx + 1, check);
        }
    }

    if !ctx.passed_checks.is_empty() {
        render_passed_line(&mut out, &ctx.passed_checks);
    }

    if let (Some(started), Some(finished)) = (ctx.started_at.as_deref(), ctx.finished_at.as_deref()) {
        if let Some(window) = format_window(started, finished) {
            out.push_str(&format!("\n{}\n", window));
        }
    }

    out.push_str("\n");
    out.push_str(
        "Diagnose the failure and propose concrete fixes using action blocks. \
         Focus on updatePiece, createPiece, configureRuntime, generatePlan, or approvePlan — \
         do NOT use runPiece, runAllTasks, or retryGoalStep, as the system retries the phase automatically after your fixes.",
    );

    out
}

fn render_failed_check(out: &mut String, index: usize, check: &VerificationCheck) {
    let kind_tag = kind_token(&check.kind);
    out.push_str(&format!("{index}. [{kind_tag}] {}\n", check.name));

    match (check.expected.as_deref(), check.actual.as_deref()) {
        (Some(expected), Some(actual)) => {
            out.push_str(&format!(
                "   Expected: {}\n",
                snip(expected, MAX_FIELD_CHARS)
            ));
            render_actual(out, check.kind.clone(), actual);
        }
        (Some(expected), None) => {
            out.push_str(&format!(
                "   Expected: {}\n",
                snip(expected, MAX_FIELD_CHARS)
            ));
            out.push_str(&format!("   Detail: {}\n", snip(&check.detail, MAX_FIELD_CHARS)));
        }
        (None, Some(actual)) => {
            render_actual(out, check.kind.clone(), actual);
        }
        (None, None) => {
            out.push_str(&format!("   Detail: {}\n", snip(&check.detail, MAX_FIELD_CHARS)));
        }
    }

    out.push_str(&format!("   Duration: {}ms\n", check.duration_ms));
}

fn render_actual(out: &mut String, kind: CheckKind, actual: &str) {
    // For log-scan checks, a multi-line `actual` is usually a log excerpt; block-quote it.
    if matches!(kind, CheckKind::LogScan) && actual.contains('\n') {
        let truncated = snip(actual, MAX_LOG_BLOCK_CHARS);
        out.push_str("   Actual:\n");
        for line in truncated.lines() {
            out.push_str("   > ");
            out.push_str(line);
            out.push('\n');
        }
    } else {
        out.push_str(&format!(
            "   Actual:   {}\n",
            snip(actual, MAX_FIELD_CHARS)
        ));
    }
}

fn render_passed_line(out: &mut String, passed: &[VerificationCheck]) {
    out.push_str(&format!("\nPassed checks ({}): ", passed.len()));
    let shown = passed.iter().take(MAX_PASSED_LISTED);
    let names: Vec<String> = shown
        .map(|c| format!("{} ({})", c.name, kind_token(&c.kind)))
        .collect();
    out.push_str(&names.join(", "));
    if passed.len() > MAX_PASSED_LISTED {
        out.push_str(&format!(" …and {} more", passed.len() - MAX_PASSED_LISTED));
    }
    out.push('\n');
}

fn format_window(started_at: &str, finished_at: &str) -> Option<String> {
    let start = chrono::DateTime::parse_from_rfc3339(started_at).ok()?;
    let end = chrono::DateTime::parse_from_rfc3339(finished_at).ok()?;
    let duration_ms = (end - start).num_milliseconds();
    if duration_ms < 0 {
        return None;
    }
    Some(format!(
        "Window: {started_at} → {finished_at} ({duration_ms}ms)"
    ))
}

fn phase_token(phase: &GoalRunPhase) -> String {
    serde_json::to_string(phase)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

fn kind_token(kind: &CheckKind) -> &'static str {
    match kind {
        CheckKind::Shell => "shell",
        CheckKind::Http => "http",
        CheckKind::TcpPort => "tcpPort",
        CheckKind::LogScan => "logScan",
        CheckKind::Skipped => "skipped",
    }
}

fn snip(s: &str, max: usize) -> String {
    let total = s.chars().count();
    if total <= max {
        return s.to_string();
    }
    let kept: String = s.chars().take(max).collect();
    format!("{kept}…[truncated {} more chars]", total - max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentRole, CheckKind, GoalRunPhase, VerificationCheck, VerificationResult};

    fn check(
        name: &str,
        kind: CheckKind,
        passed: bool,
        detail: &str,
        duration_ms: i64,
        expected: Option<&str>,
        actual: Option<&str>,
    ) -> VerificationCheck {
        VerificationCheck {
            name: name.to_string(),
            kind,
            passed,
            detail: detail.to_string(),
            duration_ms,
            expected: expected.map(str::to_string),
            actual: actual.map(str::to_string),
        }
    }

    #[test]
    fn renders_full_verification_shape_with_mixed_checks() {
        let verification = VerificationResult {
            passed: false,
            checks: vec![
                check(
                    "verify command",
                    CheckKind::Shell,
                    true,
                    "exited 0 via `npm test`",
                    120,
                    Some("exit code 0"),
                    Some("exit code 0"),
                ),
                check(
                    "http readiness",
                    CheckKind::Http,
                    false,
                    "HTTP 500 from http://127.0.0.1:3000",
                    450,
                    Some("HTTP 200 from http://127.0.0.1:3000"),
                    Some("HTTP 500 — Internal Server Error: db connection refused"),
                ),
            ],
            started_at: "2026-04-17T18:22:03+00:00".to_string(),
            finished_at: "2026-04-17T18:22:04+00:00".to_string(),
            message: "HTTP readiness check failed".to_string(),
        };
        let ctx = PhaseFailureContext::from_verification(&verification);
        let got = build_repair_prompt(GoalRunPhase::Verification, &ctx);

        let expected = "The goal run has failed during the **verification** phase.\n\n\
Summary: HTTP readiness check failed\n\
\n\
Failed checks (1):\n\
\n\
1. [http] http readiness\n   \
Expected: HTTP 200 from http://127.0.0.1:3000\n   \
Actual:   HTTP 500 — Internal Server Error: db connection refused\n   \
Duration: 450ms\n\
\n\
Passed checks (1): verify command (shell)\n\
\n\
Window: 2026-04-17T18:22:03+00:00 → 2026-04-17T18:22:04+00:00 (1000ms)\n\
\n\
Diagnose the failure and propose concrete fixes using action blocks. \
Focus on updatePiece, createPiece, configureRuntime, generatePlan, or approvePlan — \
do NOT use runPiece, runAllTasks, or retryGoalStep, as the system retries the phase automatically after your fixes.";
        assert_eq!(got, expected);
    }

    #[test]
    fn renders_without_passed_section_when_none_passed() {
        let ctx = PhaseFailureContext {
            summary: "all checks failed".to_string(),
            failed_checks: vec![check(
                "http readiness",
                CheckKind::Http,
                false,
                "connection refused",
                60_000,
                Some("HTTP 200"),
                Some("connection refused"),
            )],
            passed_checks: vec![],
            started_at: None,
            finished_at: None,
            failing_role: None,
        };
        let got = build_repair_prompt(GoalRunPhase::Verification, &ctx);
        assert!(!got.contains("Passed checks"));
        assert!(got.contains("Failed checks (1):"));
    }

    #[test]
    fn non_verification_summary_only_path_degrades_to_minimal_shape() {
        let ctx = PhaseFailureContext::from_summary("piece agent returned exit code 1");
        let got = build_repair_prompt(GoalRunPhase::Implementation, &ctx);

        let expected = "The goal run has failed during the **implementation** phase.\n\n\
Summary: piece agent returned exit code 1\n\
\n\
Diagnose the failure and propose concrete fixes using action blocks. \
Focus on updatePiece, createPiece, configureRuntime, generatePlan, or approvePlan — \
do NOT use runPiece, runAllTasks, or retryGoalStep, as the system retries the phase automatically after your fixes.";
        assert_eq!(got, expected);
    }

    #[test]
    fn log_scan_multiline_actual_is_block_quoted() {
        let ctx = PhaseFailureContext {
            summary: "panic detected in logs".to_string(),
            failed_checks: vec![check(
                "panic detector",
                CheckKind::LogScan,
                false,
                "matched fatal pattern",
                5,
                Some("no match for /panic!?|FATAL|unhandled/"),
                Some(
                    "[info] starting server\n[info] loading config\n[fatal] FATAL: missing DATABASE_URL",
                ),
            )],
            passed_checks: vec![],
            started_at: None,
            finished_at: None,
            failing_role: None,
        };
        let got = build_repair_prompt(GoalRunPhase::Verification, &ctx);
        assert!(got.contains("   Actual:\n"));
        assert!(got.contains("   > [info] starting server"));
        assert!(got.contains("   > [fatal] FATAL: missing DATABASE_URL"));
    }

    #[test]
    fn long_actual_is_truncated_with_marker_and_prompt_stays_bounded() {
        let huge = "x".repeat(5_000);
        let ctx = PhaseFailureContext {
            summary: "boom".to_string(),
            failed_checks: vec![check(
                "shell",
                CheckKind::Shell,
                false,
                "exited 1",
                10,
                Some("exit code 0"),
                Some(&huge),
            )],
            passed_checks: vec![],
            started_at: None,
            finished_at: None,
            failing_role: None,
        };
        let got = build_repair_prompt(GoalRunPhase::Verification, &ctx);
        assert!(got.contains("…[truncated"));
        // 400-char snip + header/instructions ⇒ well under 2 KB.
        assert!(
            got.len() < 2_000,
            "prompt unexpectedly grew to {} bytes",
            got.len()
        );
    }

    #[test]
    fn detail_fallback_is_used_when_expected_and_actual_are_missing() {
        let ctx = PhaseFailureContext {
            summary: "legacy row".to_string(),
            failed_checks: vec![check(
                "legacy check",
                CheckKind::Shell,
                false,
                "exited 2 via `npm run check`",
                10,
                None,
                None,
            )],
            passed_checks: vec![],
            started_at: None,
            finished_at: None,
            failing_role: None,
        };
        let got = build_repair_prompt(GoalRunPhase::Verification, &ctx);
        assert!(got.contains("   Detail: exited 2 via `npm run check`"));
        assert!(!got.contains("Expected:"));
        assert!(!got.contains("Actual:"));
    }

    #[test]
    fn renders_failing_role_line_when_set() {
        let ctx = PhaseFailureContext::from_summary("x").with_failing_role(AgentRole::Review);
        let got = build_repair_prompt(GoalRunPhase::Implementation, &ctx);
        assert!(
            got.contains("Failing role: review"),
            "expected prompt to contain 'Failing role: review', got:\n{got}"
        );
    }

    #[test]
    fn passed_checks_are_capped_with_and_n_more_suffix() {
        let mut passed = Vec::new();
        for i in 0..9 {
            passed.push(check(
                &format!("check {i}"),
                CheckKind::Shell,
                true,
                "ok",
                1,
                None,
                None,
            ));
        }
        let ctx = PhaseFailureContext {
            summary: "one failed".to_string(),
            failed_checks: vec![check(
                "http readiness",
                CheckKind::Http,
                false,
                "500",
                20,
                Some("HTTP 200"),
                Some("HTTP 500"),
            )],
            passed_checks: passed,
            started_at: None,
            finished_at: None,
            failing_role: None,
        };
        let got = build_repair_prompt(GoalRunPhase::Verification, &ctx);
        assert!(got.contains("Passed checks (9): check 0 (shell)"));
        assert!(got.contains("…and 3 more"));
    }
}
