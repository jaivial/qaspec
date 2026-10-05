//! Runs spec files in order inside ONE agent-browser session (one Chromium), one tab per project,
//! reusing authenticated identities across suites and across runs.

use crate::agent::{self, Context as AgentCtx, Verdict};
use crate::browser::{request_matches, truncate, Browser};
use crate::config::{Config, Identity};
use crate::llm::Llm;
use crate::report::{ItemResult, Report, Status, StepResult, SuiteResult};
use crate::spec::{self, CaptureSource, Check, Item, OnFail, SpecFile, Suite, UrlOp};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct RunOptions {
    pub keep_open: bool,
    pub quiet: bool,
}

pub struct Runner<'a> {
    cfg: &'a Config,
    browser: Browser,
    llm: Option<Llm>,
    llm_error: Option<String>,
    run_id: String,
    vars: BTreeMap<String, String>,
    /// project -> identity currently loaded in the browser.
    active: HashMap<String, String>,
    /// identities that failed to authenticate in this run: (project, identity) -> reason.
    broken_identities: HashMap<(String, String), String>,
    /// project -> reason it is unavailable (health failed or dependency unavailable).
    unavailable: HashMap<String, String>,
    health_checked: HashMap<String, bool>,
    /// Login suites run during this run (reported as setup).
    setup_results: Vec<SuiteResult>,
    opts: RunOptions,
}

/// One suite to run, with the project it belongs to.
#[derive(Clone)]
pub struct Planned {
    pub file: String,
    pub suite: Suite,
    pub project: String,
}

pub fn suite_project(cfg: &Config, file: &str, suite: &Suite) -> Result<String> {
    if let Some(p) = &suite.project {
        cfg.project(p)?;
        return Ok(p.clone());
    }
    for p in cfg.projects.values() {
        if let Some(g) = &p.specs {
            let pat = cfg.root.join(g);
            if let Ok(m) = glob::Pattern::new(&pat.to_string_lossy()) {
                if m.matches_path(&cfg.root.join(file)) || m.matches_path(Path::new(file)) {
                    return Ok(p.name.clone());
                }
            }
        }
    }
    if cfg.projects.len() == 1 {
        return Ok(cfg.projects.keys().next().cloned().unwrap_or_default());
    }
    bail!(
        "{file}: suite `{}` has no `project` and the file matches no project's `specs` glob",
        suite.name
    )
}

/// Orders suites: projects in dependency order, then grouped by identity, file order preserved.
pub fn plan(cfg: &Config, files: &[SpecFile]) -> Result<Vec<Planned>> {
    let mut all = Vec::new();
    for f in files {
        for s in &f.suites {
            all.push(Planned {
                file: f.path.clone(),
                suite: s.clone(),
                project: suite_project(cfg, &f.path, s)?,
            });
        }
    }
    let order = cfg.project_order();
    let mut out = Vec::new();
    for p in &order {
        let mine: Vec<&Planned> = all.iter().filter(|x| &x.project == p).collect();
        let mut idents: Vec<Option<String>> = Vec::new();
        for x in &mine {
            if !idents.contains(&x.suite.identity) {
                idents.push(x.suite.identity.clone());
            }
        }
        for id in idents {
            out.extend(
                mine.iter()
                    .filter(|x| x.suite.identity == id)
                    .map(|x| (*x).clone()),
            );
        }
    }
    Ok(out)
}

/// Finds spec files: explicit paths (files or dirs), else each project's `specs` glob, else `**/*.qa.ts`.
/// Files starting with `_` and identity login specs are only run as setup.
pub fn discover(cfg: &Config, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    let push = |p: PathBuf, out: &mut Vec<PathBuf>| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    let logins: Vec<PathBuf> = cfg
        .projects
        .values()
        .flat_map(|p| p.identities.values())
        .filter_map(|i| i.login.as_ref())
        .map(|l| cfg.root.join(l))
        .collect();
    let is_setup = |p: &Path| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('_'))
            || logins.iter().any(|l| same_path(l, p))
    };
    if !paths.is_empty() {
        for p in paths {
            if p.is_dir() {
                for e in glob::glob(&format!("{}/**/*.qa.ts", p.display()))? {
                    let e = e?;
                    if !is_setup(&e) {
                        push(e, &mut out);
                    }
                }
            } else if p.exists() {
                push(p.clone(), &mut out);
            } else {
                bail!("{} does not exist", p.display());
            }
        }
    } else {
        let globs: Vec<String> = cfg
            .projects
            .values()
            .filter_map(|p| p.specs.clone())
            .collect();
        let globs = if globs.is_empty() {
            vec!["**/*.qa.ts".to_string()]
        } else {
            globs
        };
        for g in globs {
            let mut found: Vec<PathBuf> = glob::glob(&cfg.root.join(&g).to_string_lossy())?
                .filter_map(|e| e.ok())
                .collect();
            found.sort();
            for e in found {
                if !is_setup(&e)
                    && !e
                        .components()
                        .any(|c| c.as_os_str() == "node_modules" || c.as_os_str() == ".qaspec")
                {
                    push(e, &mut out);
                }
            }
        }
    }
    Ok(out)
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

pub fn load_spec(path: &Path, cfg_root: &Path) -> Result<SpecFile> {
    let src =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let shown = path
        .strip_prefix(cfg_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();
    spec::parse(&shown, &src)
}

fn host_of(url: &str) -> String {
    url.split_once("://")
        .map(|(_, r)| r)
        .unwrap_or(url)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_string()
}

/// Keeps only the cookies and storage origins that belong to `host` in a `state save` file.
pub fn filter_state(state: &Value, host: &str) -> Value {
    let cookie_ok = |c: &Value| {
        let d = c["domain"].as_str().unwrap_or("").trim_start_matches('.');
        !d.is_empty() && (host == d || host.ends_with(&format!(".{d}")))
    };
    let origin_ok = |o: &Value| host_of(o["origin"].as_str().unwrap_or("")) == host;
    let mut out = state.clone();
    if let Some(a) = state["cookies"].as_array() {
        out["cookies"] = Value::Array(a.iter().filter(|c| cookie_ok(c)).cloned().collect());
    }
    if let Some(a) = state["origins"].as_array() {
        out["origins"] = Value::Array(a.iter().filter(|o| origin_ok(o)).cloned().collect());
    }
    out
}

fn write_private(path: &Path, data: &str) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Replaces `${...}` placeholders. Secrets are never interpolated.
pub fn interpolate(
    text: &str,
    cfg: &Config,
    project: &str,
    identity: Option<&Identity>,
    run_id: &str,
    vars: &BTreeMap<String, String>,
) -> Result<String> {
    let re = regex::Regex::new(r"\$\{\s*([A-Za-z0-9_.\-]+)\s*\}").unwrap();
    let mut err = None;
    let out = re.replace_all(text, |c: &regex::Captures| {
        let key = &c[1];
        let parts: Vec<&str> = key.split('.').collect();
        let v = match parts.as_slice() {
            ["params", n] => cfg.params.get(*n).cloned(),
            ["project", "base_url"] => cfg.projects.get(project).map(|p| p.base_url.clone()),
            ["project", "name"] => Some(project.to_string()),
            ["projects", p, "base_url"] => cfg.projects.get(*p).map(|p| p.base_url.clone()),
            ["identity", "username"] => identity.and_then(|i| i.username.clone()),
            ["identity", "name"] => identity.map(|i| i.name.clone()),
            ["run", "id"] => Some(run_id.to_string()),
            ["env"] => Some(cfg.env.clone()),
            [n] => vars.get(*n).cloned(),
            _ => None,
        };
        v.unwrap_or_else(|| {
            err.get_or_insert_with(|| format!("unknown placeholder `${{{key}}}`"));
            String::new()
        })
    });
    match err {
        Some(e) => bail!(e),
        None => Ok(out.into_owned()),
    }
}

impl<'a> Runner<'a> {
    pub fn new(cfg: &'a Config, opts: RunOptions) -> Result<Self> {
        let run_id = format!(
            "{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis()
                % 0xffff_ffff
        );
        let session = cfg.browser.session.clone().unwrap_or_else(|| {
            if opts.keep_open {
                format!("qaspec-{}", cfg.env)
            } else {
                format!("qaspec-{run_id}")
            }
        });
        let owns = !opts.keep_open && cfg.browser.session.is_none();
        let (llm, llm_error) = match &cfg.llm {
            Some(l) => match l.api_key.as_ref().map(|k| k.read(&cfg.root)).transpose() {
                Ok(key) => (Some(Llm::new(&l.base_url, key)), None),
                Err(e) => (None, Some(format!("LLM_UNAVAILABLE: llm.api_key: {e:#}"))),
            },
            None => (
                None,
                Some("LLM_UNAVAILABLE: qaspec.toml has no [llm] section".to_string()),
            ),
        };
        Ok(Runner {
            cfg,
            browser: Browser::new(&cfg.browser.command, &session, cfg.browser.headed, owns),
            llm,
            llm_error,
            run_id,
            vars: BTreeMap::new(),
            active: HashMap::new(),
            broken_identities: HashMap::new(),
            unavailable: HashMap::new(),
            health_checked: HashMap::new(),
            setup_results: Vec::new(),
            opts,
        })
    }

    fn say(&self, s: &str) {
        if !self.opts.quiet {
            eprintln!("{s}");
        }
    }

    pub fn run(&mut self, planned: &[Planned]) -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new(&self.run_id, &self.cfg.env, self.browser.session());
        for p in planned {
            let r = self.run_suite(p);
            report.suites.append(&mut self.setup_results);
            report.suites.push(r);
        }
        if !self.opts.keep_open {
            self.browser.close();
        }
        report.duration_ms = started.elapsed().as_millis() as u64;
        report.browser_calls = self.browser.calls;
        if let Some(l) = &self.llm {
            report.model_calls = l.calls;
            report.tokens = l.tokens;
        }
        report.finish();
        Ok(report)
    }

    fn check_health(&mut self, project: &str) {
        if self.health_checked.contains_key(project) {
            return;
        }
        let p = &self.cfg.projects[project];
        for d in p.depends_on.clone() {
            self.check_health(&d);
            if let Some(r) = self.unavailable.get(&d).cloned() {
                self.unavailable.insert(
                    project.to_string(),
                    format!("dependency `{d}` unavailable: {r}"),
                );
            }
        }
        let mut ok = !self.unavailable.contains_key(project);
        if ok {
            if let Some(h) = &p.health {
                let url = self.cfg.resolve_url(project, h).unwrap_or_default();
                let res = ureq::AgentBuilder::new()
                    .timeout(std::time::Duration::from_secs(15))
                    .build()
                    .get(&url)
                    .call();
                if let Err(e) = res {
                    let msg = match e {
                        ureq::Error::Status(c, _) => format!("HTTP {c}"),
                        other => other.to_string(),
                    };
                    self.unavailable.insert(
                        project.to_string(),
                        format!(
                            "ENVIRONMENT_UNAVAILABLE: health {url} -> {}",
                            truncate(&msg, 120)
                        ),
                    );
                    ok = false;
                }
            }
        }
        self.health_checked.insert(project.to_string(), ok);
    }

    fn state_path(&self, project: &str, identity: &str) -> PathBuf {
        self.cfg
            .state_dir()
            .join(format!("{project}.{identity}.json"))
    }

    /// Saves the cookies/storage of every active identity into its own file (filtered by host).
    fn save_active_states(&mut self) -> Result<()> {
        if self.active.is_empty() {
            return Ok(());
        }
        let tmp = std::env::temp_dir().join(format!(
            "qaspec-state-{}-{}.json",
            std::process::id(),
            self.run_id
        ));
        self.browser.state_save(&tmp.to_string_lossy())?;
        let full: Value = serde_json::from_str(&std::fs::read_to_string(&tmp)?)?;
        let _ = std::fs::remove_file(&tmp);
        for (p, i) in self.active.clone() {
            let host = host_of(&self.cfg.projects[&p].base_url);
            write_private(
                &self.state_path(&p, &i),
                &serde_json::to_string_pretty(&filter_state(&full, &host))?,
            )?;
        }
        Ok(())
    }

    /// Makes `identity` the signed-in user of `project` in the shared browser.
    fn ensure_identity(&mut self, project: &str, identity: &str) -> Result<()> {
        if self.active.get(project).map(|s| s.as_str()) == Some(identity) {
            return Ok(());
        }
        if let Some(r) = self
            .broken_identities
            .get(&(project.to_string(), identity.to_string()))
        {
            bail!("{r}");
        }
        let res = self.switch_identity(project, identity);
        if let Err(e) = &res {
            self.broken_identities.insert(
                (project.to_string(), identity.to_string()),
                format!("{e:#}"),
            );
        }
        res
    }

    fn switch_identity(&mut self, project: &str, identity: &str) -> Result<()> {
        let proj = self.cfg.project(project)?.clone();
        let id = proj.identities.get(identity).cloned().ok_or_else(|| {
            anyhow!("AUTH_CREDENTIAL_UNAVAILABLE: project `{project}` has no identity `{identity}`")
        })?;
        // Another identity of this project is loaded: cookies are global in agent-browser, so save
        // everyone, clear, and restore the other projects' identities.
        if self.active.contains_key(project) {
            self.say(&format!(
                "  ↺ switching {project} identity {} → {identity}",
                self.active[project]
            ));
            self.save_active_states()?;
            self.browser.run(&["cookies", "clear"])?;
            let _ = self.browser.run(&["storage", "local", "clear"]);
            let _ = self.browser.run(&["storage", "session", "clear"]);
            self.active.remove(project);
            for (p, i) in self.active.clone() {
                let f = self.state_path(&p, &i);
                if f.exists() {
                    self.browser.state_load(&f.to_string_lossy())?;
                }
            }
        }
        let file = self.state_path(project, identity);
        if file.exists() {
            self.browser.state_load(&file.to_string_lossy())?;
            if self.identity_valid(project, &id)? {
                self.say(&format!("  ✓ {project}/{identity}: reused saved session"));
                self.active
                    .insert(project.to_string(), identity.to_string());
                return Ok(());
            }
            self.say(&format!(
                "  … {project}/{identity}: saved session expired, signing in again"
            ));
        }
        self.login(project, &id)?;
        if !self.identity_valid(project, &id)? {
            bail!("AUTH_CREDENTIAL_INVALID: {project}/{identity} is not signed in after login (valid_if failed)");
        }
        self.active
            .insert(project.to_string(), identity.to_string());
        self.save_active_states()?;
        self.say(&format!(
            "  ✓ {project}/{identity}: signed in, session saved"
        ));
        Ok(())
    }

    fn identity_valid(&mut self, project: &str, id: &Identity) -> Result<bool> {
        let v = &id.valid_if;
        if v.url_not.is_none() && v.js.is_none() {
            return Ok(true);
        }
        self.browser.goto(&self.cfg.resolve_url(project, "/")?)?;
        if let Some(u) = &v.url_not {
            if self.browser.url()?.contains(u.as_str()) {
                return Ok(false);
            }
        }
        if let Some(js) = &v.js {
            let r = self.browser.eval(js)?;
            if !truthy(&r) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn login(&mut self, project: &str, id: &Identity) -> Result<()> {
        self.say(&format!("  → {project}/{}: signing in", id.name));
        if let Some(login) = &id.login {
            let path = self.cfg.root.join(login);
            let f = load_spec(&path, &self.cfg.root)?;
            for s in &f.suites {
                let planned = Planned {
                    file: f.path.clone(),
                    suite: s.clone(),
                    project: project.to_string(),
                };
                let mut r = self.run_steps(&planned, Some(id));
                r.setup = true;
                r.identity = Some(id.name.clone());
                let status = r.status;
                let why = r
                    .steps
                    .iter()
                    .find(|s| s.status != Status::Passed)
                    .map(|s| s.reason.clone())
                    .unwrap_or_default();
                self.setup_results.push(r);
                if status != Status::Passed {
                    bail!(
                        "AUTH_CREDENTIAL_INVALID: login spec {} {}: {why}",
                        f.path,
                        status.as_str()
                    );
                }
            }
            return Ok(());
        }
        // No login spec: the agent signs in.
        let goal = format!(
            "Sign in to the app{}. Use fill_secret for the password. The goal is reached when you are signed in (no longer on a login form).",
            id.username.as_ref().map(|u| format!(" as {u}")).unwrap_or_default()
        );
        self.browser.goto(&self.cfg.resolve_url(project, "/")?)?;
        let out = self.goal(project, Some(id), &goal, &[])?;
        if out.verdict != Verdict::Passed {
            bail!(
                "AUTH_CREDENTIAL_INVALID: automatic sign-in failed: {}",
                out.reason
            );
        }
        Ok(())
    }

    fn goal(
        &mut self,
        project: &str,
        id: Option<&Identity>,
        goal: &str,
        history: &[String],
    ) -> Result<agent::Outcome> {
        let cfg = self.cfg;
        let llm_err = self
            .llm_error
            .clone()
            .unwrap_or_else(|| "LLM_UNAVAILABLE".into());
        let llm_cfg = cfg.llm.as_ref().ok_or_else(|| anyhow!("{llm_err}"))?;
        let llm = self.llm.as_mut().ok_or_else(|| anyhow!("{llm_err}"))?;
        let proj = &cfg.projects[project];
        let ctx = AgentCtx {
            project,
            base_url: &proj.base_url,
            locale: proj.locale.as_deref(),
            username: id.and_then(|i| i.username.as_deref()),
            secret_names: id
                .map(|i| i.secrets.keys().cloned().collect())
                .unwrap_or_default(),
            history,
        };
        let secrets = id.map(|i| i.secrets.clone()).unwrap_or_default();
        let root = cfg.root.clone();
        let mut secret = |name: &str| -> Result<String> {
            secrets
                .get(name)
                .ok_or_else(|| anyhow!("unknown secret `{name}`"))?
                .read(&root)
        };
        let resolve = |u: &str| {
            cfg.resolve_url(project, u)
                .unwrap_or_else(|_| u.to_string())
        };
        agent::run_goal(
            llm,
            &llm_cfg.actor,
            llm_cfg.max_turns,
            &mut self.browser,
            &ctx,
            goal,
            &mut secret,
            &resolve,
        )
    }

    fn run_suite(&mut self, p: &Planned) -> SuiteResult {
        self.say(&format!(
            "\n▶ {} › {}  [{}{}]",
            p.file,
            p.suite.name,
            p.project,
            p.suite
                .identity
                .as_ref()
                .map(|i| format!(" as {i}"))
                .unwrap_or_default()
        ));
        let mut blocked = None;
        if p.suite.has_llm_work() && self.llm.is_none() {
            blocked = self.llm_error.clone();
        }
        // Every project the suite touches must be healthy.
        let mut projects = vec![p.project.clone()];
        projects.extend(p.suite.steps.iter().filter_map(|s| s.project.clone()));
        for pr in &projects {
            if blocked.is_none() {
                if let Err(e) = self.cfg.project(pr) {
                    blocked = Some(e.to_string());
                    continue;
                }
                self.check_health(pr);
                if let Some(r) = self.unavailable.get(pr) {
                    blocked = Some(format!("project `{pr}`: {r}"));
                }
            }
        }
        if let Some(reason) = blocked {
            self.say(&format!("  ⊘ blocked: {reason}"));
            return SuiteResult::blocked(p, &reason);
        }
        self.run_steps(p, None)
    }

    /// Runs a suite's steps in order, sharing browser state between them.
    fn run_steps(&mut self, p: &Planned, login_identity: Option<&Identity>) -> SuiteResult {
        let started = Instant::now();
        let mut steps: Vec<StepResult> = Vec::new();
        let mut history: Vec<String> = Vec::new();
        let mut state_broken = false;
        let mut aborted = false;
        for (idx, st) in p.suite.steps.iter().enumerate() {
            let project = st.project.clone().unwrap_or_else(|| p.project.clone());
            let ident_name = st.identity.clone().or_else(|| {
                if st.project.is_none() {
                    p.suite.identity.clone()
                } else {
                    None
                }
            });
            let start = st.start.clone().or_else(|| {
                if idx == 0 {
                    Some(p.suite.start.clone().unwrap_or_else(|| "/".into()))
                } else {
                    None
                }
            });
            // Each project has its own tab: switching back to a project continues in that tab's
            // state, so a project switch only counts as a fresh start for the first visit.
            let switching_project = idx > 0 && project != steps_project(p, idx - 1);
            let first_visit =
                switching_project && !(0..idx).any(|j| steps_project(p, j) == project);
            let has_start = start.is_some() || first_visit;

            let skip_reason = if aborted {
                Some("an earlier step failed with onFail: 'abort'".to_string())
            } else if let Some(n) = st.needs.iter().find(|n| {
                steps
                    .iter()
                    .any(|r| &r.name == *n && r.status != Status::Passed)
            }) {
                Some(format!("needs `{n}`, which did not pass"))
            } else if state_broken && !has_start {
                Some("an earlier step failed and this one continues from its state (give it a `start` to make it independent)".to_string())
            } else {
                None
            };
            if let Some(r) = skip_reason {
                self.say(&format!("  ○ {} — skipped: {r}", st.name));
                steps.push(StepResult::skipped(&st.name, &r));
                continue;
            }
            if has_start {
                state_broken = false;
            }
            let t = Instant::now();
            let res = self.run_step(
                p,
                st,
                &project,
                ident_name.as_deref(),
                login_identity,
                start
                    .as_deref()
                    .or(if first_visit { Some("/") } else { None }),
                &history,
            );
            let mut r = match res {
                Ok(r) => r,
                Err(e) => StepResult {
                    name: st.name.clone(),
                    status: Status::Blocked,
                    reason: format!("{e:#}"),
                    items: vec![],
                    actions: vec![],
                    url: String::new(),
                    duration_ms: 0,
                    signals: None,
                },
            };
            r.duration_ms = t.elapsed().as_millis() as u64;
            r.reason = self.browser.redact(&r.reason);
            self.say(&format!(
                "  {} {} ({:.1}s){}",
                r.status.icon(),
                st.name,
                r.duration_ms as f64 / 1000.0,
                if r.reason.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", truncate(&r.reason, 200))
                }
            ));
            for it in &r.items {
                if it.status != Status::Passed {
                    self.say(&format!(
                        "      {} {}{}",
                        it.status.icon(),
                        it.label,
                        if it.detail.is_empty() {
                            String::new()
                        } else {
                            format!(": {}", truncate(&it.detail, 200))
                        }
                    ));
                }
            }
            history.push(format!(
                "`{}`: {} (URL {})",
                st.name,
                r.status.as_str(),
                r.url
            ));
            if r.status != Status::Passed {
                match st.on_fail {
                    OnFail::Stop => state_broken = true,
                    OnFail::Abort => aborted = true,
                    OnFail::Continue => {}
                }
            }
            steps.push(r);
        }
        SuiteResult::from_steps(p, steps, started.elapsed().as_millis() as u64)
    }

    #[allow(clippy::too_many_arguments)]
    fn run_step(
        &mut self,
        p: &Planned,
        st: &spec::Step,
        project: &str,
        ident_name: Option<&str>,
        login_identity: Option<&Identity>,
        start: Option<&str>,
        history: &[String],
    ) -> Result<StepResult> {
        let cfg = self.cfg;
        self.browser.use_tab(project)?;
        let identity: Option<Identity> = match (login_identity, ident_name) {
            (Some(i), _) => Some(i.clone()),
            (None, Some(n)) => {
                self.ensure_identity(project, n)?;
                Some(cfg.projects[project].identities[n].clone())
            }
            (None, None) => None,
        };
        let interp = |s: &str, vars: &BTreeMap<String, String>, run_id: &str| {
            interpolate(s, cfg, project, identity.as_ref(), run_id, vars)
        };
        self.browser.mark_signals()?;
        if let Some(s) = start {
            let url = cfg.resolve_url(project, &interp(s, &self.vars, &self.run_id)?)?;
            self.browser.goto(&url)?;
        }
        // Evaluation order inside each segment between goals: deterministic expectations first,
        // then judged ones (skipped if something already failed), then captures. Results are
        // reported in file order.
        let order = evaluation_order(&st.items);
        let mut results: Vec<Option<ItemResult>> = vec![None; st.items.len()];
        let mut actions = Vec::new();
        let mut failed = false;
        let mut blocked: Option<String> = None;
        for idx in order {
            let it = &st.items[idx];
            let r = match it {
                Item::Goal { text, .. } => {
                    let text = interp(text, &self.vars, &self.run_id)?;
                    if failed || blocked.is_some() {
                        ItemResult::new(format!("goal('{text}')"), Status::Skipped, "")
                    } else {
                        let out = self.goal(project, identity.as_ref(), &text, history)?;
                        actions.extend(out.actions.clone());
                        let s = verdict_status(&out.verdict);
                        if s == Status::Failed {
                            failed = true;
                        }
                        if s == Status::Blocked {
                            blocked = Some(out.reason.clone());
                        }
                        ItemResult::new(format!("goal('{text}')"), s, &out.reason)
                    }
                }
                Item::Expect { check, .. } => {
                    let check = interp_check(check, &|s| interp(s, &self.vars, &self.run_id))?;
                    let label = check.label();
                    if blocked.is_some() || (failed && !check.is_deterministic()) {
                        ItemResult::new(
                            label,
                            Status::Skipped,
                            if failed {
                                "not judged: the step already failed"
                            } else {
                                ""
                            },
                        )
                    } else {
                        let (s, detail) =
                            match self.eval_check(project, identity.as_ref(), &check, history) {
                                Ok(x) => x,
                                Err(e) => (Status::Blocked, format!("{e:#}")),
                            };
                        match s {
                            Status::Failed => failed = true,
                            Status::Blocked => blocked = Some(detail.clone()),
                            _ => {}
                        }
                        ItemResult::new(label, s, &detail)
                    }
                }
                Item::Capture { name, source, .. } => {
                    if failed || blocked.is_some() {
                        ItemResult::new(format!("capture('{name}')"), Status::Skipped, "")
                    } else {
                        let v = match source {
                            CaptureSource::State { js } => {
                                Some(json_to_plain(&self.browser.eval(js)?))
                            }
                            CaptureSource::Url => Some(self.browser.url()?),
                            CaptureSource::Describe { description } => {
                                let d = interp(description, &self.vars, &self.run_id)?;
                                let model = cfg
                                    .llm
                                    .as_ref()
                                    .map(|l| l.judge.clone())
                                    .ok_or_else(|| anyhow!("LLM_UNAVAILABLE"))?;
                                let err = self
                                    .llm_error
                                    .clone()
                                    .unwrap_or_else(|| "LLM_UNAVAILABLE".into());
                                agent::extract(
                                    self.llm.as_mut().ok_or_else(|| anyhow!("{err}"))?,
                                    &model,
                                    &mut self.browser,
                                    &d,
                                )?
                            }
                        };
                        match v {
                            Some(v) if !v.is_empty() && v != "null" => {
                                let shown = self.browser.redact(&v);
                                self.vars.insert(name.clone(), v);
                                ItemResult::new(
                                    format!("capture('{name}')"),
                                    Status::Passed,
                                    &truncate(&shown, 120),
                                )
                            }
                            _ => {
                                failed = true;
                                ItemResult::new(
                                    format!("capture('{name}')"),
                                    Status::Failed,
                                    "value not found",
                                )
                            }
                        }
                    }
                }
            };
            results[idx] = Some(r);
        }
        let items: Vec<ItemResult> = results.into_iter().flatten().collect();
        let signals = self.browser.signals().ok();
        let url = self.browser.url().unwrap_or_default();
        let status = if blocked.is_some() {
            Status::Blocked
        } else if failed {
            Status::Failed
        } else {
            Status::Passed
        };
        let reason = match (&blocked, failed) {
            (Some(b), _) => b.clone(),
            (None, true) => items
                .iter()
                .find(|i| i.status == Status::Failed)
                .map(|i| {
                    format!(
                        "{}{}",
                        i.label,
                        if i.detail.is_empty() {
                            String::new()
                        } else {
                            format!(": {}", i.detail)
                        }
                    )
                })
                .unwrap_or_default(),
            _ => String::new(),
        };
        let _ = p;
        Ok(StepResult {
            name: st.name.clone(),
            status,
            reason,
            items,
            actions,
            url,
            duration_ms: 0,
            signals: signals.map(|s| crate::report::SignalSummary::from(&s)),
        })
    }

    fn eval_check(
        &mut self,
        project: &str,
        identity: Option<&Identity>,
        check: &Check,
        history: &[String],
    ) -> Result<(Status, String)> {
        let pass = |ok: bool, detail: String| {
            Ok((if ok { Status::Passed } else { Status::Failed }, detail))
        };
        match check {
            Check::Judge { text } => {
                let sig = self.browser.signals()?;
                let cfg = self.cfg;
                let proj = &cfg.projects[project];
                let ctx = AgentCtx {
                    project,
                    base_url: &proj.base_url,
                    locale: proj.locale.as_deref(),
                    username: identity.and_then(|i| i.username.as_deref()),
                    secret_names: vec![],
                    history,
                };
                let model = cfg.llm.as_ref().map(|l| l.judge.clone()).ok_or_else(|| {
                    anyhow!(
                        "{}",
                        self.llm_error
                            .clone()
                            .unwrap_or_else(|| "LLM_UNAVAILABLE".into())
                    )
                })?;
                let o = agent::judge(
                    self.llm.as_mut().ok_or_else(|| {
                        anyhow!(
                            "{}",
                            self.llm_error
                                .clone()
                                .unwrap_or_else(|| "LLM_UNAVAILABLE".into())
                        )
                    })?,
                    &model,
                    &mut self.browser,
                    &ctx,
                    text,
                    &sig,
                )?;
                Ok((verdict_status(&o.verdict), o.reason))
            }
            Check::ConsoleNoErrors => {
                let s = self.browser.signals()?;
                let all: Vec<String> = s
                    .console_errors
                    .iter()
                    .chain(s.page_errors.iter())
                    .cloned()
                    .collect();
                pass(
                    all.is_empty(),
                    all.iter()
                        .take(3)
                        .map(|e| truncate(e, 160))
                        .collect::<Vec<_>>()
                        .join(" | "),
                )
            }
            Check::ErrorsNone => {
                let s = self.browser.signals()?;
                pass(
                    s.page_errors.is_empty(),
                    s.page_errors
                        .iter()
                        .take(3)
                        .map(|e| truncate(e, 160))
                        .collect::<Vec<_>>()
                        .join(" | "),
                )
            }
            Check::NoNetworkFailures => {
                let s = self.browser.signals()?;
                let bad: Vec<String> = s
                    .requests
                    .iter()
                    .filter(|r| r.status.is_some_and(|c| c >= 400))
                    .map(|r| {
                        format!(
                            "{} {} -> {}",
                            r.method,
                            truncate(&r.url, 120),
                            r.status.unwrap_or(0)
                        )
                    })
                    .collect();
                pass(
                    bad.is_empty(),
                    bad.into_iter().take(5).collect::<Vec<_>>().join(" | "),
                )
            }
            Check::Network {
                pattern,
                status,
                ok,
            } => {
                let s = self.browser.signals()?;
                let m: Vec<_> = s
                    .requests
                    .iter()
                    .filter(|r| request_matches(pattern, &r.method, &r.url))
                    .collect();
                if m.is_empty() {
                    return pass(
                        false,
                        format!(
                            "no request matched `{pattern}` ({} requests in this step)",
                            s.requests.len()
                        ),
                    );
                }
                let bad: Vec<String> = m
                    .iter()
                    .filter(|r| match (status, ok) {
                        (Some(code), _) => r.status != Some(*code),
                        (None, true) => !r.status.is_some_and(|c| (200..400).contains(&c)),
                        _ => false,
                    })
                    .map(|r| {
                        format!(
                            "{} {} -> {}",
                            r.method,
                            truncate(&r.url, 120),
                            r.status.map(|x| x.to_string()).unwrap_or("pending".into())
                        )
                    })
                    .collect();
                pass(
                    bad.is_empty(),
                    if bad.is_empty() {
                        format!("{} matching request(s)", m.len())
                    } else {
                        bad.join(" | ")
                    },
                )
            }
            Check::Url { op, value, negate } => {
                let u = self.browser.url()?;
                let hit = match op {
                    UrlOp::Contains => u.contains(value.as_str()),
                    UrlOp::Equals => {
                        u == *value
                            || u.trim_end_matches('/')
                                == self.cfg.resolve_url(project, value)?.trim_end_matches('/')
                    }
                };
                pass(hit != *negate, format!("URL is {u}"))
            }
            Check::State {
                js,
                expected,
                negate,
            } => {
                let v = match self.browser.eval(js) {
                    Ok(v) => v,
                    Err(e) => {
                        return pass(
                            false,
                            format!("{js} threw: {}", truncate(&e.to_string(), 160)),
                        )
                    }
                };
                let ok = match expected {
                    Some(e) => json_eq(&v, e),
                    None => truthy(&v),
                };
                pass(
                    ok != *negate,
                    format!("{js} = {}", truncate(&v.to_string(), 160)),
                )
            }
            Check::Visible { text, negate } => {
                let page = self.browser.text()?;
                let norm = |x: &str| x.split_whitespace().collect::<Vec<_>>().join(" ");
                let seen = norm(&page).contains(&norm(text));
                pass(
                    seen != *negate,
                    if seen {
                        format!("`{text}` is on the page")
                    } else {
                        format!("`{text}` is not on the page")
                    },
                )
            }
            Check::StorageLocal { key, expected } => {
                let js = format!("localStorage.getItem({})", serde_json::to_string(key)?);
                let v = self.browser.eval(&js)?;
                let ok = match expected {
                    None => !v.is_null(),
                    Some(e) => match (&v, e) {
                        (Value::String(s), Value::String(x)) => s == x,
                        (Value::String(s), other) => serde_json::from_str::<Value>(s)
                            .map(|p| json_eq(&p, other))
                            .unwrap_or(false),
                        _ => false,
                    },
                };
                pass(
                    ok,
                    format!("localStorage[{key}] = {}", truncate(&v.to_string(), 160)),
                )
            }
        }
    }
}

/// Indices of `items` in evaluation order: segments are delimited by goals; inside a segment
/// deterministic expectations run first, then judged ones, then captures.
fn evaluation_order(items: &[Item]) -> Vec<usize> {
    let rank = |it: &Item| match it {
        Item::Goal { .. } => 0,
        Item::Expect { check, .. } if check.is_deterministic() => 1,
        Item::Expect { .. } => 2,
        Item::Capture { .. } => 3,
    };
    let mut out = Vec::new();
    let mut seg: Vec<usize> = Vec::new();
    let flush = |seg: &mut Vec<usize>, out: &mut Vec<usize>| {
        seg.sort_by_key(|i| rank(&items[*i]));
        out.append(seg);
    };
    for (i, it) in items.iter().enumerate() {
        if matches!(it, Item::Goal { .. }) {
            flush(&mut seg, &mut out);
            out.push(i);
        } else {
            seg.push(i);
        }
    }
    flush(&mut seg, &mut out);
    out
}

fn steps_project(p: &Planned, idx: usize) -> String {
    p.suite.steps[idx]
        .project
        .clone()
        .unwrap_or_else(|| p.project.clone())
}

fn interp_check(c: &Check, f: &dyn Fn(&str) -> Result<String>) -> Result<Check> {
    Ok(match c.clone() {
        Check::Judge { text } => Check::Judge { text: f(&text)? },
        Check::Network {
            pattern,
            status,
            ok,
        } => Check::Network {
            pattern: f(&pattern)?,
            status,
            ok,
        },
        Check::Url { op, value, negate } => Check::Url {
            op,
            value: f(&value)?,
            negate,
        },
        Check::State {
            js,
            expected,
            negate,
        } => Check::State {
            js: f(&js)?,
            expected: interp_json(expected, f)?,
            negate,
        },
        Check::Visible { text, negate } => Check::Visible {
            text: f(&text)?,
            negate,
        },
        Check::StorageLocal { key, expected } => Check::StorageLocal {
            key: f(&key)?,
            expected: interp_json(expected, f)?,
        },
        other => other,
    })
}

fn interp_json(v: Option<Value>, f: &dyn Fn(&str) -> Result<String>) -> Result<Option<Value>> {
    Ok(match v {
        Some(Value::String(s)) => Some(Value::String(f(&s)?)),
        other => other,
    })
}

fn verdict_status(v: &Verdict) -> Status {
    match v {
        Verdict::Passed => Status::Passed,
        Verdict::Failed => Status::Failed,
        Verdict::Blocked => Status::Blocked,
    }
}

pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn json_eq(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() < 1e-9,
        _ => a == b,
    }
}

fn json_to_plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Overrides;
    use serde_json::json;

    fn cfg() -> Config {
        Config::from_str(
            "[projects.api]\nbase_url='http://api.local:9'\n[projects.web]\nbase_url='https://app.example.com'\ndepends_on=['api']\nspecs='specs/web/*.qa.ts'\n[projects.web.identities.admin]\nusername='a@x.io'\n[params]\nq='hi'",
            PathBuf::from("/root"),
            &Overrides::default(),
        )
        .unwrap()
    }

    #[test]
    fn interpolation() {
        let c = cfg();
        let id = c.projects["web"].identities["admin"].clone();
        let mut vars = BTreeMap::new();
        vars.insert("order".to_string(), "42".to_string());
        let s = interpolate("${params.q} ${project.base_url} ${projects.api.base_url} ${identity.username} ${run.id} ${order}", &c, "web", Some(&id), "r1", &vars).unwrap();
        assert_eq!(
            s,
            "hi https://app.example.com http://api.local:9 a@x.io r1 42"
        );
        assert!(interpolate("${params.nope}", &c, "web", None, "r", &vars)
            .unwrap_err()
            .to_string()
            .contains("params.nope"));
        assert_eq!(
            interpolate("no vars $ {x}", &c, "web", None, "r", &vars).unwrap(),
            "no vars $ {x}"
        );
    }

    #[test]
    fn planning_orders_by_dependency_and_identity() {
        let c = cfg();
        let f = spec::parse(
            "specs/web/a.qa.ts",
            "suite('w1', {as:'admin'}, () => { expect.errors.none() })\nsuite('w2', () => { expect.errors.none() })\nsuite('a1', {project:'api'}, () => { expect.errors.none() })\nsuite('w3', {as:'admin'}, () => { expect.errors.none() })",
        )
        .unwrap();
        let p = plan(&c, &[f]).unwrap();
        let names: Vec<_> = p.iter().map(|x| x.suite.name.as_str()).collect();
        assert_eq!(names, vec!["a1", "w1", "w3", "w2"]);
        assert_eq!(p[1].project, "web");
    }

    #[test]
    fn state_filtering() {
        let st = json!({"cookies":[{"name":"a","domain":".example.com"},{"name":"b","domain":"other.org"},{"name":"c","domain":"app.example.com"}],
            "origins":[{"origin":"https://app.example.com","localStorage":[]},{"origin":"https://other.org"}]});
        let f = filter_state(&st, "app.example.com");
        let names: Vec<_> = f["cookies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["a", "c"]);
        assert_eq!(f["origins"].as_array().unwrap().len(), 1);
        assert_eq!(host_of("http://localhost:8080/x"), "localhost");
    }

    #[test]
    fn deterministic_checks_run_before_the_judge() {
        let f = spec::parse("x.qa.ts", "suite('s', () => { goal('a'); capture('c', 'd'); expect('j'); expect.errors.none(); goal('b'); expect('k'); expect.url().toContain('/') })").unwrap();
        assert_eq!(
            evaluation_order(&f.suites[0].steps[0].items),
            vec![0, 3, 2, 1, 4, 6, 5]
        );
    }

    #[test]
    fn truthiness() {
        assert!(truthy(&json!(1)) && truthy(&json!("x")) && truthy(&json!({})));
        assert!(!truthy(&json!(0)) && !truthy(&json!("")) && !truthy(&Value::Null));
        assert!(json_eq(&json!(1), &json!(1.0)));
    }
}
