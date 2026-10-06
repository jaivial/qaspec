mod agent;
mod browser;
mod cache;
mod config;
mod llm;
mod parallel;
mod report;
mod runner;
mod spec;

use anyhow::{bail, Context, Result};
use cache::CacheMode;
use clap::{Parser, Subcommand};
use config::{Config, Overrides};
use std::path::{Path, PathBuf};

const INIT_CONFIG: &str = r#"# qaspec configuration — https://github.com/jaivial/qaspec
default_env = "local"

[projects.app]
specs = "specs/**/*.qa.ts"
# health = "/api/health"           # if it fails, the project's suites are `blocked`
[projects.app.env.local]
base_url = "http://localhost:3000"
# [projects.app.env.dev]
# base_url = "https://dev.example.com"

# [projects.app.identities.user]
# username = "qa@example.com"
# password = { file = "~/.config/qaspec/app-password" }   # or { env = "APP_PASSWORD" }
# valid_if = { url_not = "/login" }
# login = "specs/_login.qa.ts"     # optional; without it the agent signs in by itself

[params]
# search_term = "blue mug"

[llm]
# Any OpenAI-compatible endpoint that supports tool calls.
base_url = "https://api.openai.com/v1"
api_key = { env = "OPENAI_API_KEY" }
actor = "gpt-4.1-mini"
# judge = "gpt-4.1-mini"
"#;

const INIT_SPEC: &str = r#"import { suite, step, goal, expect } from 'qaspec';

suite('home page', { start: '/' }, () => {
  step('loads cleanly', () => {
    expect.console.noErrors();
    expect.network.noFailures();
  });

  step('main navigation works', () => {
    goal('open the first link of the main navigation');
    expect('a different page with its own content is shown');
    expect.errors.none();
  });
});
"#;

#[derive(Parser)]
#[command(
    name = "qaspec",
    version,
    about = "Agentic E2E specs driven by agent-browser"
)]
struct Cli {
    /// Path to qaspec.toml (default: search upwards from the current directory).
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run specs in one browser session.
    Run {
        /// Spec files or directories (default: every project's `specs` glob).
        paths: Vec<PathBuf>,
        /// Environment (overrides default_env and QASPEC_ENV).
        #[arg(short, long)]
        env: Option<String>,
        /// Only run suites of this project (dependencies are still health-checked).
        #[arg(short, long)]
        project: Option<String>,
        /// Override a value: params.<name>=v, projects.<p>.base_url=u, llm.actor=m, browser.headed=true.
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        /// Keep the browser open after the run and reuse it next time (session `qaspec-<env>`).
        #[arg(long)]
        keep_open: bool,
        /// Show the browser window.
        #[arg(long)]
        headed: bool,
        /// Write the JSON report here (default: .qaspec/report.json).
        #[arg(long)]
        json: Option<PathBuf>,
        /// Also write a JUnit XML report.
        #[arg(long)]
        junit: Option<PathBuf>,
        /// Replay cache: `auto` replays recorded goals and records new ones, `strict` fails when
        /// a recording is missing or stale (for CI), `off` never uses it.
        #[arg(long, value_name = "MODE")]
        cache: Option<String>,
        #[arg(short, long)]
        quiet: bool,
        /// Output on stdout: `text` (default), `json` (the full report) or `ndjson` (one event per step, then a `run` line). The text summary goes to stderr for json and ndjson.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: String,
        /// Run suites in N browser sessions (threads), in parallel. Default: 1, one session.
        #[arg(short = 'j', long)]
        jobs: Option<usize>,
        /// Skip the memory guard that lowers --jobs to what the machine can afford.
        #[arg(long)]
        force: bool,
    },
    /// Parse specs and validate config without opening a browser.
    Check {
        paths: Vec<PathBuf>,
        /// Print the result as JSON (ok, env, counts, warnings and the suite list).
        #[arg(long)]
        json: bool,
        #[arg(short, long)]
        env: Option<String>,
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
    },
    /// Read a finished run: summary, failed steps with evidence, or the raw JSON.
    Report {
        /// Report file (default: .qaspec/report.json next to qaspec.toml).
        path: Option<PathBuf>,
        /// Print every test as a checkbox, including the agent's explanation for failures.
        #[arg(long)]
        todo: bool,
        /// Only show failed and blocked steps, with their failing checks.
        #[arg(long)]
        failed: bool,
        /// Only show steps whose name contains this text (case-insensitive).
        #[arg(long, value_name = "TEXT")]
        step: Option<String>,
        /// Print machine-readable JSON (the filtered report) instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Create a new spec file from a template: `qaspec new checkout`.
    New {
        /// Spec name; the file is written to specs/<name>.qa.ts.
        name: String,
    },
    /// Create qaspec.toml and an example spec.
    Init,
    /// Manage saved authentication state.
    State {
        #[command(subcommand)]
        op: StateOp,
    },
}

#[derive(Subcommand)]
enum StateOp {
    /// List saved identities.
    List,
    /// Delete saved state (all, or one project / project.identity).
    Clear { target: Option<String> },
}

fn find_config(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    let mut dir = std::env::current_dir()?;
    loop {
        let c = dir.join("qaspec.toml");
        if c.exists() {
            return Ok(c);
        }
        if !dir.pop() {
            bail!("no qaspec.toml found (run `qaspec init`)");
        }
    }
}

fn overrides(env: Option<String>, set: &[String]) -> Result<Overrides> {
    let mut o = Overrides { env, set: vec![] };
    for s in set {
        let (k, v) = s
            .split_once('=')
            .with_context(|| format!("--set {s}: expected KEY=VALUE"))?;
        o.set.push((k.trim().to_string(), v.to_string()));
    }
    Ok(o)
}

fn load_all(cfg: &Config, paths: &[PathBuf]) -> Result<Vec<spec::SpecFile>> {
    let files = runner::discover(cfg, paths)?;
    if files.is_empty() {
        bail!("no spec files found");
    }
    let mut errors = Vec::new();
    let mut specs = Vec::new();
    for f in &files {
        match runner::load_spec(f, &cfg.root) {
            Ok(s) => specs.push(s),
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    if !errors.is_empty() {
        bail!("{}", errors.join("\n"));
    }
    Ok(specs)
}

fn check_identities(cfg: &Config, planned: &[runner::Planned]) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    for p in planned {
        let mut refs = vec![(p.project.clone(), p.suite.identity.clone())];
        for s in &p.suite.steps {
            if s.project.is_some() || s.identity.is_some() {
                refs.push((
                    s.project.clone().unwrap_or_else(|| p.project.clone()),
                    s.identity.clone(),
                ));
            }
        }
        for (proj, id) in refs {
            let pr = cfg
                .project(&proj)
                .with_context(|| format!("{} › {}", p.file, p.suite.name))?;
            if let Some(id) = id {
                let ident = pr.identities.get(&id).with_context(|| {
                    format!(
                        "{} › {}: project `{proj}` has no identity `{id}`",
                        p.file, p.suite.name
                    )
                })?;
                if let Some(l) = &ident.login {
                    let lp = cfg.root.join(l);
                    runner::load_spec(&lp, &cfg.root)
                        .with_context(|| format!("login spec of {proj}/{id}"))?;
                }
                for (name, s) in &ident.secrets {
                    if let Err(e) = s.read(&cfg.root) {
                        warnings.push(format!("secret {proj}.{id}.{name} ({}): {e}", s.describe()));
                    }
                }
            }
        }
        if p.suite.has_llm_work() && cfg.llm.is_none() {
            warnings.push(format!(
                "{} › {}: has goals/judged expectations but there is no [llm] section",
                p.file, p.suite.name
            ));
        }
    }
    warnings.sort();
    warnings.dedup();
    Ok(warnings)
}

/// Runs the planned suites in `jobs` browser sessions, one thread each, and merges their reports.
fn run_parallel(
    cfg: &Config,
    planned: &[runner::Planned],
    jobs: usize,
    keep_open: bool,
    quiet: bool,
    cache_mode: CacheMode,
) -> Result<report::Report> {
    let groups = parallel::partition(cfg, planned, jobs)?;
    parallel::assert_disjoint_identities(cfg, &groups, planned)?;
    let total = groups.len();
    let run_id = new_run_id();
    let wall = std::time::Instant::now();
    let mut handles = Vec::new();
    for (w, g) in groups.into_iter().enumerate() {
        let cfg_owned = cfg.clone();
        let mine: Vec<runner::Planned> = g.into_iter().map(|i| planned[i].clone()).collect();
        let id = run_id.clone();
        handles.push(std::thread::spawn(
            move || -> Result<(report::Report, Vec<String>)> {
                let mut r = runner::Runner::new(
                    &cfg_owned,
                    runner::RunOptions {
                        keep_open,
                        quiet,
                        cache: cache_mode,
                        session_suffix: format!("-w{w}"),
                    },
                    runner::Output::Buffered,
                    Some(&id),
                )?;
                let rep = r.run(&mine)?;
                Ok((rep, r.take_buffer()))
            },
        ));
    }
    let mut reports: Vec<Option<report::Report>> = (0..total).map(|_| None).collect();
    for (w, h) in handles.into_iter().enumerate() {
        let (rep, left) = h
            .join()
            .map_err(|_| anyhow::anyhow!("worker {w} panicked"))??;
        for l in left {
            eprintln!("[w{w}] {l}");
        }
        reports[w] = Some(rep);
    }
    let mut merged = report::Report::new(
        &run_id,
        &cfg.env,
        &reports[0].as_ref().unwrap().browser_session,
    );
    // One entry per worker session, in worker order (Report::new seeded it with the first one).
    merged.browser_sessions.clear();
    for r in reports.into_iter().flatten() {
        merged.browser_sessions.extend(r.browser_sessions);
        merged.browser_calls += r.browser_calls;
        merged.model_calls += r.model_calls;
        merged.tokens += r.tokens;
        merged.suites.extend(r.suites);
    }
    // Every suite keeps its plan position in the merged report.
    merged.suites.sort_by_key(|s| {
        planned
            .iter()
            .position(|p| p.file == s.file && p.suite.name == s.suite)
            .unwrap_or(usize::MAX)
    });
    merged.duration_ms = wall.elapsed().as_millis() as u64;
    merged.finish();
    Ok(merged)
}

fn new_run_id() -> String {
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            % 0xffff_ffff
    )
}

/// True when the command asked for machine-readable output, so a failure should be JSON too.
fn wants_json(cmd: &Cmd) -> bool {
    match cmd {
        Cmd::Run { format, .. } => format == "json" || format == "ndjson",
        Cmd::Check { json, .. } | Cmd::Report { json, .. } => *json,
        _ => false,
    }
}

fn error_json(e: &anyhow::Error) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": format!("{e:#}"),
        "exitCode": 3,
    })
}

fn todo_text(rep: &serde_json::Value) -> String {
    let mut out = format!(
        "test todo list — run {} [{}]\n",
        rep["runId"].as_str().unwrap_or("?"),
        rep["status"].as_str().unwrap_or("?")
    );
    for suite in rep["suites"].as_array().into_iter().flatten() {
        out.push_str(&format!("\n{} ({})\n", suite["suite"], suite["file"]));
        for step in suite["steps"].as_array().into_iter().flatten() {
            let status = step["status"].as_str().unwrap_or("?");
            let mark = if status == "passed" { "x" } else { " " };
            out.push_str(&format!("  [{mark}] {} — {status}\n", step["name"]));
            if status != "passed" {
                if let Some(reason) = step["reason"].as_str().filter(|s| !s.is_empty()) {
                    out.push_str(&format!("      agent answer: {reason}\n"));
                }
                for item in step["items"].as_array().into_iter().flatten() {
                    let item_status = item["status"].as_str().unwrap_or("");
                    if item_status == "failed" || item_status == "blocked" {
                        let detail = item["detail"].as_str().unwrap_or("");
                        out.push_str(&format!(
                            "      {}: {}{}\n",
                            item["label"],
                            item_status,
                            if detail.is_empty() {
                                String::new()
                            } else {
                                format!(" — {detail}")
                            }
                        ));
                    }
                }
            }
        }
    }
    out
}

fn main() {
    let cli = Cli::parse();
    let json = wants_json(&cli.cmd);
    match real_main(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e:#}");
            if json {
                println!("{}", error_json(&e));
            }
            std::process::exit(3);
        }
    }
}

fn real_main(cli: Cli) -> Result<i32> {
    match cli.cmd {
        Cmd::Init => {
            if Path::new("qaspec.toml").exists() {
                bail!("qaspec.toml already exists");
            }
            std::fs::write("qaspec.toml", INIT_CONFIG)?;
            std::fs::create_dir_all("specs")?;
            if !Path::new("specs/home.qa.ts").exists() {
                std::fs::write("specs/home.qa.ts", INIT_SPEC)?;
            }
            let gi = Path::new(".gitignore");
            let cur = std::fs::read_to_string(gi).unwrap_or_default();
            if !cur.lines().any(|l| l.trim() == ".qaspec/") {
                std::fs::write(
                    gi,
                    format!(
                        "{cur}{}.qaspec/\n",
                        if cur.is_empty() || cur.ends_with('\n') {
                            ""
                        } else {
                            "\n"
                        }
                    ),
                )?;
            }
            println!("created qaspec.toml, specs/home.qa.ts and added .qaspec/ to .gitignore");
            Ok(0)
        }
        Cmd::New { name } => {
            let valid = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if !valid {
                bail!("spec name must use letters, digits, `-` or `_` only, got `{name}`");
            }
            std::fs::create_dir_all("specs")?;
            let path = format!("specs/{name}.qa.ts");
            if Path::new(&path).exists() {
                bail!("{path} already exists");
            }
            let body = INIT_SPEC.replace("home", &name);
            std::fs::write(&path, body)?;
            println!("created {path}; edit it, then run `qaspec check`");
            Ok(0)
        }
        Cmd::State { op } => {
            let cfg = Config::load(&find_config(cli.config)?, &Overrides::default())?;
            let root = cfg.root.join(".qaspec").join("state");
            match op {
                StateOp::List => {
                    for e in glob::glob(&format!("{}/*/*.json", root.display()))?.flatten() {
                        let env = e
                            .parent()
                            .and_then(|p| p.file_name())
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        println!(
                            "{env}\t{}",
                            e.file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default()
                        );
                    }
                }
                StateOp::Clear { target } => {
                    let mut n = 0;
                    for e in glob::glob(&format!("{}/*/*.json", root.display()))?.flatten() {
                        let stem = e
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let hit = match &target {
                            None => true,
                            Some(t) if t.contains('.') => &stem == t,
                            Some(t) => stem.split('.').next() == Some(t.as_str()),
                        };
                        if hit {
                            std::fs::remove_file(&e)?;
                            n += 1;
                        }
                    }
                    println!("removed {n} saved state file(s)");
                }
            }
            Ok(0)
        }
        Cmd::Check {
            paths,
            json,
            env,
            set,
        } => {
            let cfg = Config::load(&find_config(cli.config)?, &overrides(env, &set)?)?;
            let specs = load_all(&cfg, &paths)?;
            let planned = runner::plan(&cfg, &specs)?;
            let warnings = check_identities(&cfg, &planned)?;
            for w in &warnings {
                eprintln!("warning: {w}");
            }
            let steps: usize = planned.iter().map(|p| p.suite.steps.len()).sum();
            if json {
                let suites: Vec<serde_json::Value> = planned
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "file": p.file,
                            "suite": p.suite.name,
                            "project": p.project,
                            "identity": p.suite.identity,
                            "steps": p.suite.steps.len(),
                        })
                    })
                    .collect();
                let out = serde_json::json!({
                    "ok": true,
                    "env": cfg.env,
                    "files": specs.len(),
                    "suites": suites,
                    "steps": steps,
                    "warnings": warnings,
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
                return Ok(0);
            }
            println!(
                "ok — env `{}`, {} file(s), {} suite(s), {} step(s)",
                cfg.env,
                specs.len(),
                planned.len(),
                steps
            );
            // The list below is the real run order, so say why a device suite is at the end.
            let has_device = planned.iter().any(|p| {
                cfg.suite_emulation(
                    &p.project,
                    p.suite.device.as_deref(),
                    p.suite.viewport.map(|v| config::Viewport {
                        width: v.width,
                        height: v.height,
                        scale: v.scale,
                    }),
                )
                .device
                .is_some()
            });
            for p in &planned {
                let device = cfg.suite_emulation(
                    &p.project,
                    p.suite.device.as_deref(),
                    p.suite.viewport.map(|v| config::Viewport {
                        width: v.width,
                        height: v.height,
                        scale: v.scale,
                    }),
                );
                println!(
                    "  {} › {}  [{}{}] {} step(s){}",
                    p.file,
                    p.suite.name,
                    p.project,
                    p.suite
                        .identity
                        .as_ref()
                        .map(|i| format!(" as {i}"))
                        .unwrap_or_default(),
                    p.suite.steps.len(),
                    if device.is_empty() {
                        String::new()
                    } else {
                        format!("  on {}", device.describe())
                    }
                );
            }
            if has_device {
                println!(
                    "note: suites that set a `device` are listed last and run last: agent-browser \
                     0.27 cannot undo the mobile user agent it installs, and the only way back \
                     would relaunch the browser and lose the session"
                );
            }
            Ok(0)
        }
        Cmd::Report {
            path,
            todo,
            failed,
            step,
            json,
        } => {
            let file = match path {
                Some(p) => p,
                None => find_config(cli.config)?
                    .parent()
                    .map(|d| d.join(".qaspec").join("report.json"))
                    .unwrap_or_else(|| PathBuf::from(".qaspec/report.json")),
            };
            let raw = std::fs::read_to_string(&file).with_context(|| {
                format!(
                    "cannot read report {} (run `qaspec run` first)",
                    file.display()
                )
            })?;
            let mut rep: serde_json::Value = serde_json::from_str(&raw)?;
            let needle = step.map(|t| t.to_lowercase());
            if failed || needle.is_some() {
                if let Some(suites) = rep["suites"].as_array_mut() {
                    for su in suites.iter_mut() {
                        if let Some(steps) = su["steps"].as_array_mut() {
                            steps.retain(|st| {
                                let status = st["status"].as_str().unwrap_or("");
                                let by_status =
                                    !failed || status == "failed" || status == "blocked";
                                let by_name = needle.as_ref().map_or(true, |n| {
                                    st["name"].as_str().unwrap_or("").to_lowercase().contains(n)
                                });
                                by_status && by_name
                            });
                        }
                    }
                    suites.retain(|su| su["steps"].as_array().is_some_and(|a| !a.is_empty()));
                }
            }
            if todo && !json {
                print!("{}", todo_text(&rep));
            } else if json {
                println!("{}", serde_json::to_string_pretty(&rep)?);
            } else {
                println!(
                    "run {} [{}] {}: {} passed, {} failed, {} blocked, {} skipped ({} model calls)",
                    rep["runId"].as_str().unwrap_or("?"),
                    rep["env"].as_str().unwrap_or("?"),
                    rep["status"].as_str().unwrap_or("?"),
                    rep["steps"]["passed"],
                    rep["steps"]["failed"],
                    rep["steps"]["blocked"],
                    rep["steps"]["skipped"],
                    rep["modelCalls"]
                );
                for su in rep["suites"].as_array().into_iter().flatten() {
                    println!(
                        "\n{} ({}) {}",
                        su["suite"].as_str().unwrap_or("?"),
                        su["file"].as_str().unwrap_or("?"),
                        su["status"].as_str().unwrap_or("")
                    );
                    for st in su["steps"].as_array().into_iter().flatten() {
                        println!(
                            "  {} {}",
                            st["status"].as_str().unwrap_or("?"),
                            st["name"].as_str().unwrap_or("?")
                        );
                        if let Some(r) = st["reason"].as_str() {
                            println!("      reason: {r}");
                        }
                        for it in st["items"].as_array().into_iter().flatten() {
                            if it["status"] == "failed" || it["status"] == "blocked" {
                                println!(
                                    "      {} {}: {}",
                                    it["status"].as_str().unwrap_or(""),
                                    it["label"].as_str().unwrap_or(""),
                                    it["detail"].as_str().unwrap_or("")
                                );
                            }
                        }
                    }
                }
            }
            Ok(0)
        }
        Cmd::Run {
            paths,
            env,
            project,
            set,
            keep_open,
            headed,
            json,
            junit,
            cache,
            quiet,
            format,
            jobs,
            force,
        } => {
            if format != "text" && format != "json" && format != "ndjson" {
                bail!("--format must be `text`, `json` or `ndjson`, got `{format}`");
            }
            let mut set = set;
            if headed {
                set.push("browser.headed=true".into());
            }
            let cfg = Config::load(&find_config(cli.config)?, &overrides(env, &set)?)?;
            let specs = load_all(&cfg, &paths)?;
            let mut planned = runner::plan(&cfg, &specs)?;
            if let Some(p) = &project {
                cfg.project(p)?;
                planned.retain(|x| &x.project == p);
            }
            if planned.is_empty() {
                bail!("nothing to run");
            }
            for w in check_identities(&cfg, &planned)? {
                eprintln!("warning: {w}");
            }
            let cache_mode = match &cache {
                Some(c) => c.parse()?,
                None => CacheMode::Auto,
            };
            let (jobs, guard_warning) = parallel::guard(jobs.unwrap_or(1), force)?;
            if !guard_warning.is_empty() {
                eprintln!("{guard_warning}");
            }
            let report = if jobs <= 1 {
                let mut r = runner::Runner::new(
                    &cfg,
                    runner::RunOptions::single(keep_open, quiet, cache_mode),
                    if format == "ndjson" {
                        runner::Output::Ndjson
                    } else {
                        runner::Output::Direct
                    },
                    None,
                )?;
                r.run(&planned)?
            } else {
                run_parallel(&cfg, &planned, jobs, keep_open, quiet, cache_mode)?
            };
            if format == "json" {
                eprintln!("{}", report.summary());
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if format == "ndjson" {
                // Single-worker runs stream step events from Runner. Parallel runs retain the
                // buffered fallback so their output remains valid NDJSON.
                eprintln!("{}", report.summary());
                let v = serde_json::to_value(&report)?;
                if jobs > 1 {
                    for su in v["suites"].as_array().into_iter().flatten() {
                        for st in su["steps"].as_array().into_iter().flatten() {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "event": "step",
                                    "suite": su["suite"],
                                    "file": su["file"],
                                    "name": st["name"],
                                    "status": st["status"],
                                    "reason": st["reason"],
                                    "durationMs": st["durationMs"],
                                    "items": st["items"],
                                })
                            );
                        }
                    }
                }
                println!(
                    "{}",
                    serde_json::json!({
                        "event": "run",
                        "runId": v["runId"],
                        "status": v["status"],
                        "exitCode": v["exitCode"],
                        "steps": v["steps"],
                        "modelCalls": v["modelCalls"],
                    })
                );
            } else {
                println!("{}", report.summary());
            }
            let json_path = json.unwrap_or_else(|| cfg.root.join(".qaspec").join("report.json"));
            if let Some(d) = json_path.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(&json_path, serde_json::to_string_pretty(&report)?)?;
            if let Some(j) = junit {
                std::fs::write(&j, report.junit())?;
            }
            if !quiet {
                eprintln!("report: {}", json_path.display());
            }
            Ok(report.exit_code)
        }
    }
}

#[cfg(test)]
mod error_json_tests {
    use super::*;

    #[test]
    fn error_json_has_ok_false_and_message() {
        let v = error_json(&anyhow::anyhow!("no qaspec.toml found"));
        assert_eq!(v["ok"], false);
        assert_eq!(v["exitCode"], 3);
        assert!(v["error"].as_str().unwrap().contains("no qaspec.toml"));
    }

    #[test]
    fn todo_report_lists_passes_and_agent_failure_answers() {
        let report = serde_json::json!({
            "runId": "r1", "status": "failed",
            "suites": [{"suite": "checkout", "file": "checkout.qa.ts", "steps": [
                {"name": "loads", "status": "passed", "items": []},
                {"name": "pays", "status": "failed", "reason": "the payment API returned 500", "items": [
                    {"label": "expect.network", "status": "failed", "detail": "HTTP 500"}
                ]}
            ]}]
        });
        let text = todo_text(&report);
        assert!(text.contains("[x] \"loads\" — passed"));
        assert!(text.contains("[ ] \"pays\" — failed"));
        assert!(text.contains("agent answer: the payment API returned 500"));
        assert!(text.contains("\"expect.network\": failed — HTTP 500"));
    }
}
