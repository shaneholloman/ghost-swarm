use serde_json::Value;
use std::{path::Path, process::Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRequestStatusState {
    Success,
    Pending,
    Failure,
    Merged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestStatus {
    pub number: u64,
    pub state: PullRequestStatusState,
    pub summary: String,
    pub url: Option<String>,
}

pub fn workspace_pull_request_status(workspace_path: &Path) -> Option<PullRequestStatus> {
    if !is_github_workspace(workspace_path) {
        return None;
    }

    let output = Command::new("gh")
        .current_dir(workspace_path)
        .args(["pr", "view", "--json", "number,state,statusCheckRollup,url"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    parse_pull_request_status(&output.stdout).ok().flatten()
}

fn is_github_workspace(workspace_path: &Path) -> bool {
    let output = Command::new("git")
        .current_dir(workspace_path)
        .args(["config", "--get", "remote.origin.url"])
        .output();
    let Ok(output) = output else {
        return false;
    };

    if !output.status.success() {
        return false;
    }

    let remote = String::from_utf8_lossy(&output.stdout);
    let remote = remote.trim();

    remote.contains("github.com")
        || remote.starts_with("git@github:")
        || remote.starts_with("ssh://git@github.com/")
}

fn parse_pull_request_status(bytes: &[u8]) -> Result<Option<PullRequestStatus>, serde_json::Error> {
    let payload: Value = serde_json::from_slice(bytes)?;
    let Some(number) = payload.get("number").and_then(Value::as_u64) else {
        return Ok(None);
    };
    let Some(pr_state) = payload.get("state").and_then(Value::as_str) else {
        return Ok(None);
    };
    if pr_state == "MERGED" {
        return Ok(Some(PullRequestStatus {
            number,
            state: PullRequestStatusState::Merged,
            summary: format!("PR #{number} merged"),
            url: payload
                .get("url")
                .and_then(Value::as_str)
                .map(str::to_string),
        }));
    }
    if pr_state != "OPEN" {
        return Ok(None);
    }

    let checks = payload
        .get("statusCheckRollup")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let state = summarize_checks(&checks);
    let summary = match state {
        PullRequestStatusState::Success => format!("PR #{number} checks passing"),
        PullRequestStatusState::Pending => format!("PR #{number} checks pending"),
        PullRequestStatusState::Failure => format!("PR #{number} checks failing"),
        PullRequestStatusState::Merged => format!("PR #{number} merged"),
    };

    Ok(Some(PullRequestStatus {
        number,
        state,
        summary,
        url: payload
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string),
    }))
}

fn summarize_checks(checks: &[Value]) -> PullRequestStatusState {
    let mut has_pending = false;

    for check in checks {
        if is_failed_check(check) {
            return PullRequestStatusState::Failure;
        }

        if is_pending_check(check) {
            has_pending = true;
        }
    }

    if has_pending || checks.is_empty() {
        PullRequestStatusState::Pending
    } else {
        PullRequestStatusState::Success
    }
}

fn is_failed_check(check: &Value) -> bool {
    let conclusion = check.get("conclusion").and_then(Value::as_str);
    let status = check
        .get("status")
        .and_then(Value::as_str)
        .or_else(|| check.get("state").and_then(Value::as_str));

    matches!(
        conclusion,
        Some(
            "ACTION_REQUIRED" | "CANCELLED" | "FAILURE" | "STALE" | "STARTUP_FAILURE" | "TIMED_OUT"
        )
    ) || matches!(status, Some("FAILURE" | "ERROR"))
}

fn is_pending_check(check: &Value) -> bool {
    let conclusion = check.get("conclusion").and_then(Value::as_str);
    let status = check
        .get("status")
        .and_then(Value::as_str)
        .or_else(|| check.get("state").and_then(Value::as_str));

    conclusion.is_none()
        || matches!(
            status,
            Some("EXPECTED" | "IN_PROGRESS" | "PENDING" | "QUEUED" | "REQUESTED" | "WAITING")
        )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{PullRequestStatusState, parse_pull_request_status, summarize_checks};

    #[test]
    fn reports_failing_checks() {
        let checks = vec![
            json!({ "conclusion": "SUCCESS", "status": "COMPLETED" }),
            json!({ "conclusion": "FAILURE", "status": "COMPLETED" }),
        ];

        assert_eq!(summarize_checks(&checks), PullRequestStatusState::Failure);
    }

    #[test]
    fn reports_pending_checks() {
        let checks = vec![json!({ "conclusion": null, "status": "IN_PROGRESS" })];

        assert_eq!(summarize_checks(&checks), PullRequestStatusState::Pending);
    }

    #[test]
    fn reports_successful_checks() {
        let checks = vec![
            json!({ "conclusion": "SUCCESS", "status": "COMPLETED" }),
            json!({ "conclusion": "SKIPPED", "status": "COMPLETED" }),
        ];

        assert_eq!(summarize_checks(&checks), PullRequestStatusState::Success);
    }

    #[test]
    fn parses_open_pull_request_payload() {
        let payload = json!({
            "number": 42,
            "state": "OPEN",
            "statusCheckRollup": [
                { "conclusion": "SUCCESS", "status": "COMPLETED" }
            ],
            "url": "https://github.com/example/repo/pull/42"
        });

        let status = parse_pull_request_status(payload.to_string().as_bytes())
            .expect("payload should parse")
            .expect("status should exist");

        assert_eq!(status.number, 42);
        assert_eq!(status.state, PullRequestStatusState::Success);
        assert_eq!(status.summary, "PR #42 checks passing");
    }

    #[test]
    fn parses_merged_pull_request_payload() {
        let payload = json!({
            "number": 42,
            "state": "MERGED",
            "statusCheckRollup": []
        });

        let status = parse_pull_request_status(payload.to_string().as_bytes())
            .expect("payload should parse")
            .expect("status should exist");

        assert_eq!(status.number, 42);
        assert_eq!(status.state, PullRequestStatusState::Merged);
        assert_eq!(status.summary, "PR #42 merged");
    }

    #[test]
    fn ignores_closed_pull_requests() {
        let payload = json!({
            "number": 42,
            "state": "CLOSED",
            "statusCheckRollup": []
        });

        let status = parse_pull_request_status(payload.to_string().as_bytes())
            .expect("payload should parse");

        assert!(status.is_none());
    }
}
