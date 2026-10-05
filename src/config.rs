//! `qaspec.toml`: projects, environments, identities (credentials), params, LLM and browser settings.

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    default_env: Option<String>,
    #[serde(default)]
    projects: BTreeMap<String, RawProject>,
    #[serde(default)]
    params: BTreeMap<String, toml::Value>,
    #[serde(default)]
    env: BTreeMap<String, RawEnvOverride>,
    llm: Option<RawLlm>,
    #[serde(default)]
    browser: RawBrowser,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawEnvOverride {
    #[serde(default)]
    params: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProject {
    specs: Option<String>,
    base_url: Option<String>,
    health: Option<String>,
    #[serde(default)]
    depends_on: Vec<String>,
    locale: Option<String>,
    device: Option<String>,
    viewport: Option<Vec<f64>>,
    #[serde(default)]
    env: BTreeMap<String, RawProjectEnv>,
    #[serde(default)]
    identities: BTreeMap<String, RawIdentity>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProjectEnv {
    base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIdentity {
    username: Option<String>,
    password: Option<SecretSource>,
    #[serde(default)]
    secrets: BTreeMap<String, SecretSource>,
    login: Option<String>,
    valid_if: Option<ValidIf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLlm {
    provider: Option<String>,
    base_url: String,
    api_key: Option<SecretSource>,
    actor: String,
    judge: Option<String>,
    max_turns: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawBrowser {
    headed: Option<bool>,
    session: Option<String>,
    command: Option<String>,
    device: Option<String>,
    viewport: Option<Vec<f64>>,
}

/// Where a secret value comes from. The value is read only when it is used.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SecretSource {
    pub file: Option<String>,
    pub env: Option<String>,
    pub value: Option<String>,
}

impl SecretSource {
    pub fn read(&self, base: &Path) -> Result<String> {
        if let Some(f) = &self.file {
            let p = expand_path(f, base);
            return Ok(std::fs::read_to_string(&p)
                .with_context(|| format!("cannot read secret file {}", p.display()))?
                .trim_end_matches(['\n', '\r'])
                .to_string());
        }
        if let Some(e) = &self.env {
            return std::env::var(e).map_err(|_| anyhow!("environment variable {e} is not set"));
        }
        if let Some(v) = &self.value {
            return Ok(v.clone());
        }
        bail!("secret has no source: use {{ file = \"...\" }}, {{ env = \"...\" }} or {{ value = \"...\" }}")
    }

    fn validate(&self, what: &str) -> Result<()> {
        let n = [
            self.file.is_some(),
            self.env.is_some(),
            self.value.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count();
        if n != 1 {
            bail!("{what}: a secret needs exactly one of `file`, `env` or `value`");
        }
        Ok(())
    }

    pub fn describe(&self) -> String {
        if let Some(f) = &self.file {
            format!("file {f}")
        } else if let Some(e) = &self.env {
            format!("env {e}")
        } else {
            "inline value".into()
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct ValidIf {
    /// The session is valid if, after opening the project's base URL, the URL does not contain this.
    pub url_not: Option<String>,
    /// The session is valid if this JS expression is truthy on the base URL.
    pub js: Option<String>,
}

/// Device emulation for the browser: a named device or an explicit viewport.
/// `device` wins over `viewport` (a device implies its own metrics).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Emulation {
    /// Device name as agent-browser knows it (`set device "iPhone 14"`).
    pub device: Option<String>,
    /// `[width, height]` or `[width, height, scale]`.
    pub viewport: Option<Viewport>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    /// Device pixel ratio (`set viewport <w> <h> [scale]`).
    pub scale: Option<f64>,
}

impl Emulation {
    pub fn is_empty(&self) -> bool {
        self.device.is_none() && self.viewport.is_none()
    }

    /// One line for the run output: what was asked of agent-browser.
    pub fn describe(&self) -> String {
        match (&self.device, &self.viewport) {
            (Some(d), _) => d.clone(),
            (None, Some(v)) => match v.scale {
                Some(s) => format!("{}x{} at {}x", v.width, v.height, s),
                None => format!("{}x{}", v.width, v.height),
            },
            (None, None) => String::new(),
        }
    }

    /// One line for the agent, including what agent-browser reports once applied: the size and
    /// whether the device is a touch/mobile one (`set device` implies `mobile: true`).
    pub fn describe_for_agent(&self, applied: Option<&AppliedEmulation>) -> String {
        let Some(a) = applied else {
            return self.describe();
        };
        let size = format!("{}x{}", a.width, a.height);
        match &self.device {
            Some(d) if a.mobile => format!("{d}, {size}, touch"),
            Some(d) => format!("{d}, {size}"),
            None if a.mobile => format!("{size}, touch"),
            None => size,
        }
    }
}

/// What agent-browser answered after a `set device` / `set viewport`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppliedEmulation {
    pub width: u32,
    pub height: u32,
    /// `mobile: true` from agent-browser (a phone/tablet layout, not just a narrow window).
    pub mobile: bool,
}

/// Reads `device`/`viewport` from a raw TOML table, checking the shapes.
fn emulation_from(
    device: &Option<String>,
    viewport: &Option<Vec<f64>>,
    what: &str,
) -> Result<Emulation> {
    let device = device.clone().filter(|d| !d.trim().is_empty());
    if let Some(d) = &device {
        if d.contains('"') || d.contains('\n') {
            bail!("{what}.device `{d}` is not a device name");
        }
    }
    let vp = match viewport {
        None => None,
        Some(v) => {
            if v.len() < 2 || v.len() > 3 {
                bail!("{what}.viewport needs [width, height] or [width, height, scale], got {v:?}");
            }
            let (w, h) = (v[0], v[1]);
            if w < 1.0 || h < 1.0 {
                bail!("{what}.viewport: width and height must be positive, got {v:?}");
            }
            let scale = match v.as_slice() {
                [_, _, s] if *s > 0.0 => Some(*s),
                [_, _, s] => bail!("{what}.viewport: scale must be positive, got {s}"),
                _ => None,
            };
            Some(Viewport {
                width: w as u32,
                height: h as u32,
                scale,
            })
        }
    };
    Ok(Emulation {
        device,
        viewport: vp,
    })
}

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub specs: Option<String>,
    pub base_url: String,
    pub health: Option<String>,
    pub depends_on: Vec<String>,
    pub locale: Option<String>,
    pub identities: BTreeMap<String, Identity>,
    /// Device or viewport this project's suites run on (overridden by `[browser]` only if unset).
    pub emulation: Emulation,
}

#[derive(Debug, Clone)]
pub struct Identity {
    pub name: String,
    pub username: Option<String>,
    /// Secret handles by name (`password` included when configured).
    pub secrets: BTreeMap<String, SecretSource>,
    pub login: Option<String>,
    pub valid_if: ValidIf,
}

#[derive(Debug, Clone)]
pub struct Llm {
    pub judge_explicit: bool,
    pub base_url: String,
    pub api_key: Option<SecretSource>,
    pub actor: String,
    pub judge: String,
    pub max_turns: usize,
}

#[derive(Debug, Clone)]
pub struct Browser {
    pub headed: bool,
    pub session: Option<String>,
    pub command: String,
    /// Default device emulation for every suite (suite/project options take precedence).
    pub emulation: Emulation,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub root: PathBuf,
    pub env: String,
    pub projects: BTreeMap<String, Project>,
    pub params: BTreeMap<String, String>,
    pub llm: Option<Llm>,
    pub browser: Browser,
}

pub fn expand_path(p: &str, base: &Path) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    let pb = PathBuf::from(p);
    if pb.is_absolute() {
        pb
    } else {
        base.join(pb)
    }
}

fn toml_to_string(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Command-line overrides: `--env` and `--set key=value`.
#[derive(Debug, Default, Clone)]
pub struct Overrides {
    pub env: Option<String>,
    pub set: Vec<(String, String)>,
}

impl Config {
    pub fn load(path: &Path, ov: &Overrides) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let root = path
            .parent()
            .map(|p| {
                if p.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    p
                }
            })
            .unwrap_or(Path::new("."))
            .to_path_buf();
        Self::from_str(&text, root, ov).with_context(|| format!("invalid {}", path.display()))
    }

    pub fn from_str(text: &str, root: PathBuf, ov: &Overrides) -> Result<Config> {
        let raw: RawConfig = toml::from_str(text)?;
        let env = ov
            .env
            .clone()
            .or_else(|| std::env::var("QASPEC_ENV").ok())
            .or(raw.default_env.clone())
            .unwrap_or_else(|| "default".into());

        // Params: base < [env.<env>.params] < QASPEC_PARAM_<NAME> < --set params.<name>=
        let mut params: BTreeMap<String, String> = raw
            .params
            .iter()
            .map(|(k, v)| (k.clone(), toml_to_string(v)))
            .collect();
        if let Some(e) = raw.env.get(&env) {
            for (k, v) in &e.params {
                params.insert(k.clone(), toml_to_string(v));
            }
        }
        for (k, v) in std::env::vars() {
            if let Some(name) = k.strip_prefix("QASPEC_PARAM_") {
                params.insert(name.to_ascii_lowercase(), v);
            }
        }

        let mut projects = BTreeMap::new();
        for (name, p) in &raw.projects {
            validate_name(name, "project")?;
            let base_url = p
                .env
                .get(&env)
                .and_then(|e| e.base_url.clone())
                .or_else(|| p.base_url.clone())
                .or_else(|| {
                    std::env::var(format!(
                        "QASPEC_{}_BASE_URL",
                        name.to_ascii_uppercase().replace('-', "_")
                    ))
                    .ok()
                });
            let mut identities = BTreeMap::new();
            for (iname, i) in &p.identities {
                validate_name(iname, "identity")?;
                let mut secrets = i.secrets.clone();
                if let Some(pw) = &i.password {
                    secrets.insert("password".into(), pw.clone());
                }
                for (sname, s) in &secrets {
                    s.validate(&format!("projects.{name}.identities.{iname}.{sname}"))?;
                }
                identities.insert(
                    iname.clone(),
                    Identity {
                        name: iname.clone(),
                        username: i.username.clone(),
                        secrets,
                        login: i.login.clone(),
                        valid_if: i.valid_if.clone().unwrap_or_default(),
                    },
                );
            }
            projects.insert(
                name.clone(),
                Project {
                    name: name.clone(),
                    specs: p.specs.clone(),
                    base_url: base_url.unwrap_or_default(),
                    health: p.health.clone(),
                    depends_on: p.depends_on.clone(),
                    locale: p.locale.clone(),
                    identities,
                    emulation: emulation_from(&p.device, &p.viewport, &format!("projects.{name}"))?,
                },
            );
        }

        let llm = match raw.llm {
            None => None,
            Some(l) => {
                if let Some(p) = &l.provider {
                    if p != "openai-compatible" && p != "openai" {
                        bail!("llm.provider `{p}` is not supported yet (supported: openai-compatible)");
                    }
                }
                if let Some(k) = &l.api_key {
                    k.validate("llm.api_key")?;
                }
                Some(Llm {
                    judge_explicit: l.judge.is_some(),
                    base_url: l.base_url.trim_end_matches('/').to_string(),
                    api_key: l.api_key,
                    judge: l.judge.unwrap_or_else(|| l.actor.clone()),
                    actor: l.actor,
                    max_turns: l.max_turns.unwrap_or(25),
                })
            }
        };

        let mut cfg = Config {
            root,
            env,
            projects,
            params,
            llm,
            browser: Browser {
                headed: raw.browser.headed.unwrap_or(false),
                session: raw.browser.session,
                command: raw
                    .browser
                    .command
                    .unwrap_or_else(|| "agent-browser".into()),
                emulation: emulation_from(&raw.browser.device, &raw.browser.viewport, "[browser]")?,
            },
        };
        for (k, v) in &ov.set {
            cfg.apply_set(k, v)?;
        }
        cfg.validate()?;
        Ok(cfg)
    }

    fn apply_set(&mut self, key: &str, value: &str) -> Result<()> {
        let parts: Vec<&str> = key.split('.').collect();
        match parts.as_slice() {
            ["params", name] => {
                self.params.insert(name.to_string(), value.to_string());
            }
            ["projects", p, "base_url"] => {
                self.projects.get_mut(*p).ok_or_else(|| anyhow!("--set {key}: unknown project `{p}`"))?.base_url = value.to_string();
            }
            ["llm", "base_url"] => self.llm.as_mut().ok_or_else(|| anyhow!("--set {key}: no [llm] section"))?.base_url = value.trim_end_matches('/').to_string(),
            ["llm", "actor"] => {
                let l = self.llm.as_mut().ok_or_else(|| anyhow!("--set {key}: no [llm] section"))?;
                l.actor = value.to_string();
                if !l.judge_explicit {
                    l.judge = value.to_string();
                }
            }
            ["llm", "judge"] => {
                let l = self.llm.as_mut().ok_or_else(|| anyhow!("--set {key}: no [llm] section"))?;
                l.judge = value.to_string();
                l.judge_explicit = true;
            }
            ["browser", "headed"] => self.browser.headed = value == "true",
            ["browser", "device"] => self.browser.emulation.device = Some(value.to_string()),
            ["projects", p, "device"] => {
                self.projects
                    .get_mut(*p)
                    .ok_or_else(|| anyhow!("--set {key}: unknown project `{p}`"))?
                    .emulation
                    .device = Some(value.to_string());
            }
            _ => bail!("--set {key}: unsupported key (use params.<name>, projects.<p>.base_url, projects.<p>.device, llm.base_url, llm.actor, llm.judge, browser.headed, browser.device)"),
        }
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        for p in self.projects.values() {
            if p.base_url.is_empty() {
                bail!(
                    "project `{}` has no base_url for env `{}`",
                    p.name,
                    self.env
                );
            }
            if !(p.base_url.starts_with("http://")
                || p.base_url.starts_with("https://")
                || p.base_url.starts_with("file://"))
            {
                bail!(
                    "project `{}`: base_url must start with http://, https:// or file://",
                    p.name
                );
            }
            for d in &p.depends_on {
                if !self.projects.contains_key(d) {
                    bail!("project `{}` depends on unknown project `{d}`", p.name);
                }
            }
        }
        // Cycle detection in depends_on.
        for start in self.projects.keys() {
            let mut stack = vec![(start.clone(), vec![start.clone()])];
            while let Some((cur, path)) = stack.pop() {
                for d in &self.projects[&cur].depends_on {
                    if d == start {
                        bail!(
                            "dependency cycle between projects: {} -> {d}",
                            path.join(" -> ")
                        );
                    }
                    if !path.contains(d) {
                        let mut np = path.clone();
                        np.push(d.clone());
                        stack.push((d.clone(), np));
                    }
                }
            }
        }
        Ok(())
    }

    /// Projects in dependency order (dependencies first), stable by name.
    pub fn project_order(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        fn visit(cfg: &Config, n: &str, out: &mut Vec<String>) {
            if out.iter().any(|x| x == n) {
                return;
            }
            for d in &cfg.projects[n].depends_on {
                visit(cfg, d, out);
            }
            out.push(n.to_string());
        }
        for n in self.projects.keys() {
            visit(self, n, &mut out);
        }
        out
    }

    pub fn project(&self, name: &str) -> Result<&Project> {
        self.projects.get(name).ok_or_else(|| {
            anyhow!(
                "unknown project `{name}` (defined: {})",
                self.projects.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })
    }

    /// Resolves a relative path (`/x`) against the project's base URL; absolute URLs pass through.
    pub fn resolve_url(&self, project: &str, path: &str) -> Result<String> {
        if path.contains("://") || path.starts_with("data:") || path.starts_with("about:") {
            return Ok(path.to_string());
        }
        let base = self.project(project)?.base_url.trim_end_matches('/');
        if path.is_empty() || path == "/" {
            return Ok(format!("{base}/"));
        }
        Ok(format!("{base}/{}", path.trim_start_matches('/')))
    }

    /// Emulation for a suite: `suite > project > browser`.
    /// The most specific level that sets anything wins, so a suite with `device` never keeps
    /// the project's viewport (a device implies its own metrics).
    pub fn suite_emulation(
        &self,
        project: &str,
        device: Option<&str>,
        viewport: Option<Viewport>,
    ) -> Emulation {
        let from_suite = Emulation {
            device: device.map(|d| d.to_string()),
            viewport,
        };
        if !from_suite.is_empty() {
            return from_suite;
        }
        self.projects
            .get(project)
            .map(|p| p.emulation.clone())
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| self.browser.emulation.clone())
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join(".qaspec").join("state").join(&self.env)
    }
}

fn validate_name(n: &str, what: &str) -> Result<()> {
    if n.is_empty()
        || !n
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        bail!("{what} name `{n}` must use letters, digits, `-` or `_`");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CFG: &str = r#"
default_env = "dev"
[projects.api]
base_url = "http://localhost:1"
[projects.web]
depends_on = ["api"]
[projects.web.env.dev]
base_url = "https://dev.example.com/"
[projects.web.env.local]
base_url = "http://localhost:5173"
[projects.web.identities.admin]
username = "admin@example.com"
password = { env = "QASPEC_TEST_PW" }
login = "specs/login.qa.ts"
valid_if = { url_not = "/login" }
[params]
q = "hello"
n = 3
[env.local.params]
q = "hola"
[llm]
base_url = "http://x/v1/"
actor = "m1"
"#;

    fn load(ov: Overrides) -> Result<Config> {
        Config::from_str(CFG, PathBuf::from("/r"), &ov)
    }

    #[test]
    fn env_and_params() {
        let c = load(Overrides::default()).unwrap();
        assert_eq!(c.env, "dev");
        assert_eq!(c.projects["web"].base_url, "https://dev.example.com/");
        assert_eq!(c.params["q"], "hello");
        assert_eq!(c.params["n"], "3");
        assert_eq!(c.llm.as_ref().unwrap().judge, "m1");
        assert_eq!(c.llm.as_ref().unwrap().base_url, "http://x/v1");
        assert_eq!(c.project_order(), vec!["api", "web"]);
        assert_eq!(
            c.resolve_url("web", "/a/b").unwrap(),
            "https://dev.example.com/a/b"
        );
        assert_eq!(
            c.resolve_url("web", "https://o.com/x").unwrap(),
            "https://o.com/x"
        );

        let c = load(Overrides {
            env: Some("local".into()),
            set: vec![("params.q".into(), "cli".into())],
        })
        .unwrap();
        assert_eq!(c.projects["web"].base_url, "http://localhost:5173");
        assert_eq!(c.params["q"], "cli");
        let c = load(Overrides {
            env: Some("local".into()),
            set: vec![],
        })
        .unwrap();
        assert_eq!(c.params["q"], "hola");
        let c = load(Overrides {
            env: None,
            set: vec![("llm.actor".into(), "m2".into())],
        })
        .unwrap();
        assert_eq!(
            c.llm.as_ref().unwrap().judge,
            "m2",
            "judge follows actor unless set"
        );
    }

    #[test]
    fn identities_and_secrets() {
        let c = load(Overrides::default()).unwrap();
        let id = &c.projects["web"].identities["admin"];
        assert_eq!(id.username.as_deref(), Some("admin@example.com"));
        std::env::set_var("QASPEC_TEST_PW", "pw1");
        assert_eq!(id.secrets["password"].read(Path::new("/")).unwrap(), "pw1");
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("s"), "fromfile\n").unwrap();
        let s = SecretSource {
            file: Some("s".into()),
            env: None,
            value: None,
        };
        assert_eq!(s.read(dir.path()).unwrap(), "fromfile");
    }

    const MOBILE: &str = r#"
[projects.app]
base_url = "http://x"
[projects.app.env.dev]
base_url = "http://y"
[projects.phone]
base_url = "http://z"
device = "Pixel 7"
viewport = [412, 915, 3]
[projects.tablet]
base_url = "http://t"
[browser]
device = "iPhone 14"
viewport = [1280, 720]
"#;

    fn mobile() -> Config {
        Config::from_str(MOBILE, PathBuf::from("/r"), &Overrides::default()).unwrap()
    }

    #[test]
    fn emulation_is_read_from_every_level() {
        let c = mobile();
        assert_eq!(c.browser.emulation.device.as_deref(), Some("iPhone 14"));
        assert_eq!(c.browser.emulation.viewport.unwrap().width, 1280);
        let app = c.projects["app"].emulation.clone();
        assert!(app.is_empty(), "a project without options sets nothing");
        let phone = c.projects["phone"].emulation.clone();
        assert_eq!(phone.device.as_deref(), Some("Pixel 7"));
        let vp = phone.viewport.unwrap();
        assert_eq!((vp.width, vp.height, vp.scale), (412, 915, Some(3.0)));
        assert_eq!(phone.describe(), "Pixel 7");
        assert_eq!(c.projects["tablet"].emulation.describe(), "");
    }

    #[test]
    fn emulation_precedence_is_suite_then_project_then_browser() {
        let c = mobile();
        let vp = spec_viewport(360, 640, None);
        // No suite option: the project wins over [browser].
        let e = c.suite_emulation("phone", None, None);
        assert_eq!(e.device.as_deref(), Some("Pixel 7"));
        // No project option either: [browser].
        assert_eq!(
            c.suite_emulation("app", None, None).device.as_deref(),
            Some("iPhone 14")
        );
        // A suite device replaces the project's device AND its viewport (a device has metrics).
        let e = c.suite_emulation("phone", Some("iPhone 14"), None);
        assert_eq!(e.device.as_deref(), Some("iPhone 14"));
        assert_eq!(e.viewport, None);
        // A suite viewport replaces the project's device as well.
        let e = c.suite_emulation("phone", None, Some(vp));
        assert_eq!(e.device, None);
        assert_eq!(e.viewport.unwrap().width, 360);
        // A suite viewport on a project without options still beats [browser].
        let e = c.suite_emulation("app", None, Some(vp));
        assert_eq!(e.viewport.unwrap().height, 640);
        assert_eq!(e.device, None);
    }

    /// The viewport a spec suite carries, in config form.
    fn spec_viewport(w: u32, h: u32, scale: Option<f64>) -> Viewport {
        Viewport {
            width: w,
            height: h,
            scale,
        }
    }

    #[test]
    fn emulation_descriptions() {
        let mob = AppliedEmulation {
            width: 390,
            height: 844,
            mobile: true,
        };
        let desk = AppliedEmulation {
            width: 1280,
            height: 720,
            mobile: false,
        };
        let device = Emulation {
            device: Some("iPhone 14".into()),
            viewport: None,
        };
        assert_eq!(
            device.describe_for_agent(Some(&mob)),
            "iPhone 14, 390x844, touch",
            "the agent is told the device, the size and that it is touch"
        );
        assert_eq!(
            device.describe_for_agent(Some(&desk)),
            "iPhone 14, 1280x720"
        );
        assert_eq!(device.describe_for_agent(None), "iPhone 14");
        let vp = Emulation {
            device: None,
            viewport: Some(Viewport {
                width: 390,
                height: 844,
                scale: Some(3.0),
            }),
        };
        assert_eq!(vp.describe(), "390x844 at 3x");
        assert_eq!(vp.describe_for_agent(Some(&mob)), "390x844, touch");
        assert_eq!(
            vp.describe_for_agent(Some(&desk)),
            "1280x720",
            "the size is what agent-browser reports, not what was asked for"
        );
        assert_eq!(Emulation::default().describe_for_agent(None), "");
    }

    #[test]
    fn emulation_validation() {
        let bad = |t: &str| {
            Config::from_str(t, PathBuf::from("."), &Overrides::default())
                .unwrap_err()
                .to_string()
        };
        let one = |v: &str| format!("[projects.a]\nbase_url='http://x'\n{v}");
        assert!(bad(&one("viewport = [390]")).contains("viewport needs"));
        assert!(bad(&one("viewport = [390, 844, 3, 1]")).contains("viewport needs"));
        assert!(bad(&one("viewport = [0, 844]")).contains("must be positive"));
        assert!(bad(&one("viewport = [390, 844, 0]")).contains("scale must be positive"));
        // A non-number never reaches the check: TOML refuses the array itself.
        assert!(
            bad("[browser]\nviewport = ['a']\n[projects.a]\nbase_url='http://x'")
                .contains("invalid type")
        );
        assert!(bad(&one("device = 'say \"hi\"'")).contains("not a device name"));
        // An empty device means "not set", so [browser] still applies.
        let c = Config::from_str(
            "[projects.a]\nbase_url='http://x'\ndevice=''\n[browser]\ndevice='iPhone 14'",
            PathBuf::from("."),
            &Overrides::default(),
        )
        .unwrap();
        assert_eq!(
            c.suite_emulation("a", None, None).device.as_deref(),
            Some("iPhone 14")
        );
    }

    #[test]
    fn device_can_be_set_from_the_command_line() {
        let c = Config::from_str(
            MOBILE,
            PathBuf::from("."),
            &Overrides {
                env: None,
                set: vec![
                    ("browser.device".into(), "Galaxy S25".into()),
                    ("projects.phone.device".into(), "Pixel 9".into()),
                ],
            },
        )
        .unwrap();
        assert_eq!(c.browser.emulation.device.as_deref(), Some("Galaxy S25"));
        assert_eq!(
            c.projects["phone"].emulation.device.as_deref(),
            Some("Pixel 9")
        );
        assert_eq!(
            c.suite_emulation("phone", None, None).device.as_deref(),
            Some("Pixel 9")
        );
    }

    #[test]
    fn validation_errors() {
        let bad = |t: &str| {
            Config::from_str(t, PathBuf::from("."), &Overrides::default())
                .unwrap_err()
                .to_string()
        };
        assert!(bad("[projects.a]\n").contains("no base_url"));
        assert!(bad("[projects.a]\nbase_url='ftp://x'").contains("must start with"));
        assert!(
            bad("[projects.a]\nbase_url='http://x'\ndepends_on=['z']").contains("unknown project")
        );
        let cyc = "[projects.a]\nbase_url='http://x'\ndepends_on=['b']\n[projects.b]\nbase_url='http://y'\ndepends_on=['a']";
        assert!(bad(cyc).contains("cycle"));
        assert!(format!(
            "{:#}",
            Config::from_str("bogus=1", PathBuf::from("."), &Overrides::default()).unwrap_err()
        )
        .contains("unknown field"));
        assert!(bad("[projects.a]\nbase_url='http://x'\n[projects.a.identities.u]\npassword={file='a',env='b'}").contains("exactly one"));
        assert!(load(Overrides {
            env: None,
            set: vec![("nope".into(), "1".into())]
        })
        .unwrap_err()
        .to_string()
        .contains("unsupported key"));
    }
}
