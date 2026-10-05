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
        #[arg(short, long)]
        env: Option<String>,
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
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

fn main() {
    let cli = Cli::parse();
    match real_main(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e:#}");
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
        Cmd::Check { paths, env, set } => {
            let cfg = Config::load(&find_config(cli.config)?, &overrides(env, &set)?)?;
            let specs = load_all(&cfg, &paths)?;
            let planned = runner::plan(&cfg, &specs)?;
            let warnings = check_identities(&cfg, &planned)?;
            for w in &warnings {
                eprintln!("warning: {w}");
            }
            let steps: usize = planned.iter().map(|p| p.suite.steps.len()).sum();
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
            jobs,
            force,
        } => {
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
                    runner::Output::Direct,
                    None,
                )?;
                r.run(&planned)?
            } else {
                run_parallel(&cfg, &planned, jobs, keep_open, quiet, cache_mode)?
            };
            println!("{}", report.summary());
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
