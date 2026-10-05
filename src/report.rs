//! Run report: terminal summary, JSON (`report-1` schema) and JUnit XML.

use crate::browser::Signals;
use crate::cache::Origin;
use crate::runner::Planned;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Passed,
    Failed,
    Blocked,
    Skipped,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Passed => "passed",
            Status::Failed => "failed",
            Status::Blocked => "blocked",
            Status::Skipped => "skipped",
        }
    }
    pub fn icon(&self) -> &'static str {
        match self {
            Status::Passed => "✓",
            Status::Failed => "✗",
            Status::Blocked => "⊘",
            Status::Skipped => "○",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemResult {
    pub label: String,
    pub status: Status,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// For goal items: whether it replayed from the cache or the agent drove the browser.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<Origin>,
}

impl ItemResult {
    pub fn new(label: String, status: Status, detail: &str) -> Self {
        ItemResult {
            label,
            status,
            detail: detail.to_string(),
            cache: None,
        }
    }

    pub fn cached(mut self, origin: Origin) -> Self {
        self.cache = Some(origin);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalSummary {
    pub console_errors: usize,
    pub page_errors: usize,
    pub requests: usize,
    pub failed_requests: usize,
}

impl From<&Signals> for SignalSummary {
    fn from(s: &Signals) -> Self {
        SignalSummary {
            console_errors: s.console_errors.len(),
            page_errors: s.page_errors.len(),
            requests: s.requests.len(),
            failed_requests: s
                .requests
                .iter()
                .filter(|r| r.status.is_some_and(|c| c >= 400))
                .count(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub name: String,
    pub status: Status,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
    pub items: Vec<ItemResult>,
    pub actions: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signals: Option<SignalSummary>,
}

impl StepResult {
    pub fn skipped(name: &str, reason: &str) -> Self {
        StepResult {
            name: name.into(),
            status: Status::Skipped,
            reason: reason.into(),
            items: vec![],
            actions: vec![],
            url: String::new(),
            duration_ms: 0,
            signals: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuiteResult {
    pub file: String,
    pub suite: String,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    pub status: Status,
    /// A login suite run to authenticate an identity.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub setup: bool,
    /// Device or viewport the suite ran on (empty when it ran on the browser default).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub device: String,
    pub duration_ms: u64,
    pub steps: Vec<StepResult>,
}

impl SuiteResult {
    pub fn blocked(p: &Planned, reason: &str) -> Self {
        SuiteResult {
            file: p.file.clone(),
            suite: p.suite.name.clone(),
            project: p.project.clone(),
            identity: p.suite.identity.clone(),
            status: Status::Blocked,
            setup: false,
            device: String::new(),
            duration_ms: 0,
            steps: p
                .suite
                .steps
                .iter()
                .map(|s| StepResult {
                    status: Status::Blocked,
                    ..StepResult::skipped(&s.name, reason)
                })
                .collect(),
        }
    }

    pub fn from_steps(p: &Planned, steps: Vec<StepResult>, duration_ms: u64) -> Self {
        let status = if steps.iter().any(|s| s.status == Status::Failed) {
            Status::Failed
        } else if steps.iter().any(|s| s.status == Status::Blocked) {
            Status::Blocked
        } else if steps.iter().all(|s| s.status == Status::Skipped) {
            Status::Skipped
        } else {
            Status::Passed
        };
        SuiteResult {
            file: p.file.clone(),
            suite: p.suite.name.clone(),
            project: p.project.clone(),
            identity: p.suite.identity.clone(),
            status,
            setup: false,
            device: String::new(),
            duration_ms,
            steps,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub passed: usize,
    pub failed: usize,
    pub blocked: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema_version: &'static str,
    pub version: &'static str,
    pub run_id: String,
    pub env: String,
    /// First session; kept for backward compatibility with readers of `report-1`.
    pub browser_session: String,
    /// Every session of the run (one entry unless `--jobs N` was used).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub browser_sessions: Vec<String>,
    pub status: Status,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub browser_calls: u64,
    pub model_calls: u64,
    pub tokens: u64,
    pub steps: Totals,
    pub suites: Vec<SuiteResult>,
}

impl Report {
    pub fn new(run_id: &str, env: &str, session: &str) -> Self {
        Report {
            schema_version: "report-1",
            version: env!("CARGO_PKG_VERSION"),
            run_id: run_id.into(),
            env: env.into(),
            browser_session: session.into(),
            browser_sessions: vec![session.into()],
            status: Status::Passed,
            exit_code: 0,
            duration_ms: 0,
            browser_calls: 0,
            model_calls: 0,
            tokens: 0,
            steps: Totals {
                passed: 0,
                failed: 0,
                blocked: 0,
                skipped: 0,
            },
            suites: vec![],
        }
    }

    pub fn finish(&mut self) {
        let all = || {
            self.suites
                .iter()
                .filter(|s| !s.setup)
                .flat_map(|s| s.steps.iter())
        };
        let count = |st: Status| all().filter(|s| s.status == st).count();
        self.steps = Totals {
            passed: count(Status::Passed),
            failed: count(Status::Failed),
            blocked: count(Status::Blocked),
            skipped: count(Status::Skipped),
        };
        (self.status, self.exit_code) = if self.steps.failed > 0 {
            (Status::Failed, 1)
        } else if self.steps.blocked > 0
            || self
                .suites
                .iter()
                .any(|s| s.setup && s.status != Status::Passed)
        {
            (Status::Blocked, 2)
        } else if self.steps.passed == 0 {
            (Status::Skipped, 0)
        } else {
            (Status::Passed, 0)
        };
    }

    /// Goals answered by a recording, and goals the agent drove and stored.
    pub fn cache_counts(&self) -> (usize, usize) {
        let mut replayed = 0;
        let mut recorded = 0;
        for s in &self.suites {
            for st in &s.steps {
                for i in &st.items {
                    match i.cache {
                        Some(Origin::Replayed) => replayed += 1,
                        Some(Origin::Recorded) => recorded += 1,
                        _ => {}
                    }
                }
            }
        }
        (replayed, recorded)
    }

    pub fn summary(&self) -> String {
        let t = &self.steps;
        let (replayed, recorded) = self.cache_counts();
        let sessions = self.browser_sessions.len().max(1);
        format!(
            "\n{} {} — steps: {} passed, {} failed, {} blocked, {} skipped · {} suites · {:.1}s · {} browser session{} · {} browser calls · {} model calls ({} tokens){}",
            self.status.icon(),
            self.status.as_str().to_uppercase(),
            t.passed,
            t.failed,
            t.blocked,
            t.skipped,
            self.suites.len(),
            self.duration_ms as f64 / 1000.0,
            sessions,
            if sessions == 1 { "" } else { "s" },
            self.browser_calls,
            self.model_calls,
            self.tokens,
            if replayed + recorded == 0 {
                String::new()
            } else {
                format!(
                    " · {replayed} goal{} replayed from cache, {recorded} recorded",
                    if replayed == 1 { "" } else { "s" }
                )
            }
        )
    }

    pub fn junit(&self) -> String {
        let esc = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        };
        let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        x.push_str(&format!(
            "<testsuites name=\"qaspec\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"{:.3}\">\n",
            self.steps.passed + self.steps.failed + self.steps.blocked + self.steps.skipped,
            self.steps.failed,
            self.steps.blocked,
            self.steps.skipped,
            self.duration_ms as f64 / 1000.0
        ));
        for s in &self.suites {
            x.push_str(&format!(
                "  <testsuite name=\"{}\" tests=\"{}\" time=\"{:.3}\">\n",
                esc(&format!("{} › {}", s.file, s.suite)),
                s.steps.len(),
                s.duration_ms as f64 / 1000.0
            ));
            for st in &s.steps {
                x.push_str(&format!(
                    "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\"",
                    esc(&s.file),
                    esc(&st.name),
                    st.duration_ms as f64 / 1000.0
                ));
                match st.status {
                    Status::Passed => x.push_str("/>\n"),
                    Status::Failed => x.push_str(&format!(
                        "><failure message=\"{}\"/></testcase>\n",
                        esc(&st.reason)
                    )),
                    Status::Blocked => x.push_str(&format!(
                        "><error message=\"{}\"/></testcase>\n",
                        esc(&st.reason)
                    )),
                    Status::Skipped => x.push_str(&format!(
                        "><skipped message=\"{}\"/></testcase>\n",
                        esc(&st.reason)
                    )),
                }
            }
            x.push_str("  </testsuite>\n");
        }
        x.push_str("</testsuites>\n");
        x
    }
}
