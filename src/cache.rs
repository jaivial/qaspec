//! Replay cache for goals: when a step passes, the browser actions the agent performed for that
//! goal are stored with *semantic locators* (never `@eN` refs, which are ephemeral) and replayed
//! on the next run, so an unchanged UI costs zero model calls.
//!
//! A recording lives in `.qaspec/cache/<env>/<sha>.json`, where the key hashes: project, identity,
//! spec file, suite, step, the interpolated goal text and a fingerprint of the page the goal starts
//! from (URL path + the interactive snapshot's roles and accessible names). Because steps share
//! browser state, that starting-page fingerprint is part of the key: a different page before the
//! goal is a different situation, not a cache hit.
//!
//! The goal text enters the key *after* interpolation, with `${run.id}` put back as the
//! placeholder: the run id is different on every run and the same goal must always key the same
//! way. The recorded action values keep the real run id, so a replay types exactly what the agent
//! typed.
//!
//! `fill_secret` is recorded as the secret *name* only; the value is read at replay time and typed
//! through agent-browser's stdin (never argv, never in this file).

use crate::browser::{truncate, Browser};
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How the cache is used during a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    /// Replay when a recording exists; record and fall back to the agent otherwise.
    Auto,
    /// A missing, stale or broken recording is a failure (for CI).
    Strict,
    /// Never read or write recordings.
    Off,
}

impl CacheMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            CacheMode::Auto => "auto",
            CacheMode::Strict => "strict",
            CacheMode::Off => "off",
        }
    }
}

impl std::str::FromStr for CacheMode {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(CacheMode::Auto),
            "strict" => Ok(CacheMode::Strict),
            "off" => Ok(CacheMode::Off),
            other => bail!("--cache: expected auto, strict or off (got `{other}`)"),
        }
    }
}

/// Where a goal's answer came from, reported per goal item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// The recording replayed with no model call.
    Replayed,
    /// The agent ran and a new recording was stored.
    Recorded,
    /// The agent ran (no recording was stored).
    Agent,
}

/// A semantic locator for one element: ARIA role + accessible name, plus the index among the
/// elements of the snapshot that share that (role, name) when there is more than one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Locator {
    pub role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// 0-based index among matching elements; > 0 only when the snapshot was ambiguous.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub nth: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// One recorded browser action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Recorded {
    Click {
        locator: Locator,
    },
    Fill {
        locator: Locator,
        text: String,
    },
    TypeText {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        locator: Option<Locator>,
        text: String,
    },
    Press {
        key: String,
    },
    Select {
        locator: Locator,
        value: String,
    },
    Hover {
        locator: Locator,
    },
    Scroll {
        direction: String,
        pixels: i64,
    },
    Open {
        url: String,
    },
    Back,
    Wait {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ms: Option<u64>,
    },
    /// Only the secret NAME is stored; the value is resolved at replay time.
    FillSecret {
        locator: Locator,
        name: String,
    },
}

/// Everything needed to identify one goal's situation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyParts {
    pub project: String,
    pub identity: String,
    pub file: String,
    pub suite: String,
    pub step: String,
    pub goal: String,
    /// Fingerprint of the page the goal starts from (URL path + interactive roles/names).
    pub fingerprint: String,
}

/// Key of a recording: hash of [`KeyParts`]. Stable across machines (no timestamps, no paths).
pub fn compute_key(p: &KeyParts) -> String {
    let mut h = Fnv::new();
    for field in [
        p.project.as_str(),
        p.identity.as_str(),
        p.file.as_str(),
        p.suite.as_str(),
        p.step.as_str(),
        p.goal.as_str(),
        p.fingerprint.as_str(),
    ] {
        h.write(field.as_bytes());
        h.write(&[0u8]); // field separator: no field can contain NUL
    }
    h.hex()
}

/// FNV-1a 64-bit, so the key needs no new dependency. Big enough to keep CI caches honest.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

pub fn schema() -> String {
    SCHEMA.to_string()
}

/// Format version of the cache files.
pub const SCHEMA: &str = "qaspec-cache-1";

/// A stored recording for one goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    #[serde(default = "schema")]
    pub schema_version: String,
    pub key: String,
    pub project: String,
    pub identity: String,
    pub file: String,
    pub suite: String,
    pub step: String,
    pub goal: String,
    /// URL path (and query) where the goal started; the host is the environment's business.
    pub start_path: String,
    pub fingerprint: String,
    pub actions: Vec<Recorded>,
    /// Human-readable lines for the report/timeline (no secrets).
    #[serde(default)]
    pub labels: Vec<String>,
}

/// Fingerprint of a page: its URL path plus the roles and accessible names of the interactive
/// snapshot, in order. Text that merely changes does not change the key; moving/renaming a control
/// does.
pub fn fingerprint(url: &str, snapshot: &str) -> String {
    let path = url
        .split_once("://")
        .map(|(_, rest)| {
            rest.find('/')
                .map(|i| &rest[i..])
                .unwrap_or("/")
                .to_string()
        })
        .unwrap_or_else(|| url.to_string());
    let mut h = Fnv::new();
    h.write(path.as_bytes());
    for e in snapshot_entries(snapshot) {
        if e.interactive {
            h.write(b"\x01");
            h.write(e.role.as_bytes());
            h.write(b"\x02");
            h.write(e.name.as_bytes());
        }
    }
    h.hex()
}

/// One line of an accessibility snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub ref_id: String,
    pub role: String,
    pub name: String,
    /// The line carried a ref, so an action can be aimed at this element.
    pub interactive: bool,
}

/// Parses the `- role "name" [ref=eN]` lines of an agent-browser snapshot.
pub fn snapshot_entries(snapshot: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in snapshot.lines() {
        let Some(rest) = line.trim_start().strip_prefix("- ") else {
            continue;
        };
        let (role, name, ref_id) = parse_line(rest);
        let Some(ref_id) = ref_id else { continue };
        out.push(Entry {
            ref_id,
            role,
            name,
            interactive: true,
        });
    }
    out
}

/// `button "Add" [ref=e6]` -> ("button", "Add", "e6").
fn parse_line(rest: &str) -> (String, String, Option<String>) {
    let mut role = String::new();
    let mut name = String::new();
    let mut chars = rest.char_indices().peekable();
    while let Some((_, c)) = chars.peek().copied() {
        if c.is_alphanumeric() || c == '-' {
            role.push(c);
            chars.next();
        } else {
            break;
        }
    }
    let rest: String = rest[role.len()..].to_string();
    let trimmed = rest.trim_start();
    if let Some(after) = trimmed.strip_prefix('"') {
        let mut esc = false;
        for c in after.chars() {
            if esc {
                name.push(c);
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                break;
            } else {
                name.push(c);
            }
        }
    } else {
        // No accessible name (e.g. `textbox [ref=e7]`): the attrs may still carry one.
        name = attr(trimmed, "aria-label")
            .or_else(|| attr(trimmed, "placeholder"))
            .unwrap_or_default();
    }
    let ref_id = trimmed
        .split_once("ref=")
        .and_then(|(_, v)| {
            v.trim_end_matches([']', ' '])
                .trim_start_matches('@')
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect::<String>()
                .into()
        })
        .filter(|s| !s.is_empty());
    (role, name, ref_id)
}

/// Value of `key` in a snapshot attribute list, e.g. `[placeholder="Search", ref=e4]`.
fn attr(line: &str, key: &str) -> Option<String> {
    line.split_once('[')?
        .1
        .split(']')
        .next()?
        .split(',')
        .find_map(|a| {
            a.trim()
                .split_once('=')
                .filter(|(k, _)| k.trim() == key)
                .map(|(_, v)| v.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
        })
}

/// Builds the locator of the element a `@ref` points at in `snapshot`, with `nth` when the same
/// role+name appears more than once.
pub fn locator_of(snapshot: &str, ref_id: &str) -> Option<Locator> {
    let entries = snapshot_entries(snapshot);
    let id = ref_id.trim_start_matches('@');
    let found = entries.iter().find(|e| e.ref_id == id)?;
    let nth = entries
        .iter()
        .filter(|e| e.role == found.role && e.name == found.name)
        .position(|e| e.ref_id == id)
        .unwrap_or(0);
    Some(Locator {
        role: found.role.clone(),
        name: found.name.clone(),
        nth,
    })
}

/// ARIA role -> the element names agent-browser 0.27's `find role` actually matches. `find role`
/// looks up DOM tag names in this build (a `link` is found as `a`, a `textbox` as `input`); each
/// role maps to the tags to try in order, so a UI that uses a different tag still replays.
fn role_candidates(role: &str) -> Vec<&'static str> {
    let v: Vec<&'static str> = match role {
        "link" => vec!["a"],
        "button" => vec!["button", "input", "a", "div", "span"],
        "textbox" | "searchbox" | "spinbutton" => vec!["input", "textarea"],
        // `combobox` is a <select> here: it must stay the only candidate, the ref lookup uses it.
        "combobox" => vec!["select", "input"],
        "checkbox" => vec!["input", "label", "div"],
        "radio" => vec!["input", "label", "div"],
        "switch" => vec!["input", "button", "label"],
        "heading" => vec!["h1", "h2", "h3", "h4", "h5", "h6"],
        "listitem" => vec!["li"],
        "list" => vec!["ul", "ol"],
        "paragraph" => vec!["p"],
        "img" | "image" => vec!["img"],
        "navigation" => vec!["nav"],
        "main" => vec!["main"],
        "banner" => vec!["header"],
        "contentinfo" => vec!["footer"],
        "table" => vec!["table"],
        "row" => vec!["tr"],
        "cell" => vec!["td", "th"],
        "tab" => vec!["button", "a", "li", "div"],
        "menuitem" => vec!["button", "a", "li"],
        "option" => vec!["option", "li", "div"],
        "dialog" | "alertdialog" => vec!["dialog", "div"],
        "alert" => vec!["div", "p", "span"],
        "status" => vec!["div", "p", "span"],
        "form" => vec!["form"],
        _ => vec![],
    };
    if v.is_empty() {
        // Unknown roles: the role name itself is the best guess, and if agent-browser does not
        // recognise it the replay fails and the agent takes over.
        vec![Box::leak(role.to_string().into_boxed_str())]
    } else {
        v
    }
}

/// One agent-browser command per way of finding the recorded element, in the order they should be
/// tried. agent-browser 0.27's `find` is picky: `find role <x>` matches DOM tags (a `link` is an
/// `a`), and `--name` does not work on every tag (it silently finds nothing on `input`), so each
/// action carries a short list of fallbacks. The first one agent-browser can locate wins; if none
/// can, the replay fails and the agent takes over.
fn action_commands(a: &Recorded, resolve_url: &dyn Fn(&str) -> String) -> Vec<Vec<String>> {
    let located = |loc: &Locator, action: &str, value: Option<&str>| {
        let mut out = Vec::new();
        let tags = role_candidates(&loc.role);
        // 1. by role and accessible name (most precise; broken on some tags, tried first anyway).
        for tag in &tags {
            let mut c = vec!["find".into(), "role".into(), (*tag).into(), action.into()];
            if let Some(v) = value {
                c.push(v.to_string());
            }
            if !loc.name.is_empty() {
                c.push("--name".into());
                c.push(loc.name.clone());
            }
            out.push(c);
        }
        // 2. by the <label> an input is associated with.
        if !loc.name.is_empty() {
            let mut c = vec![
                "find".into(),
                "label".into(),
                loc.name.clone(),
                action.into(),
            ];
            if let Some(v) = value {
                c.push(v.to_string());
            }
            out.push(c);
        }
        // 3. by position among the same tag, using the index the snapshot gave.
        for tag in &tags {
            let mut c = vec![
                "find".into(),
                "nth".into(),
                loc.nth.to_string(),
                (*tag).into(),
                action.into(),
            ];
            if let Some(v) = value {
                c.push(v.to_string());
            }
            out.push(c);
        }
        // 4. the tag alone, only when the element was not ambiguous in the snapshot.
        if loc.nth == 0 {
            for tag in &tags {
                let mut c = vec!["find".into(), "role".into(), (*tag).into(), action.into()];
                if let Some(v) = value {
                    c.push(v.to_string());
                }
                out.push(c);
            }
        }
        out
    };
    match a {
        Recorded::Click { locator } => located(locator, "click", None),
        Recorded::Hover { locator } => located(locator, "hover", None),
        Recorded::Fill { locator, text } => located(locator, "fill", Some(text)),
        Recorded::TypeText { locator, text } => match locator {
            Some(l) => located(l, "type", Some(text)),
            None => vec![vec!["keyboard".into(), "type".into(), text.clone()]],
        },
        Recorded::Select { locator, value } => {
            // agent-browser 0.27's `find` cannot act on a <select>, so this one is replayed from a
            // fresh ref instead: `find_nth` marks it in the snapshot text below.
            vec![vec![
                SELECT_BY_REF.to_string(),
                role_candidates(&locator.role)
                    .first()
                    .copied()
                    .unwrap_or("select")
                    .to_string(),
                locator.nth.to_string(),
                value.clone(),
            ]]
        }
        Recorded::FillSecret { .. } => {
            // Never built from the recording: `replay` turns it into a `Fill` with the value read
            // at replay time, so a secret can never reach argv or this file.
            Vec::new()
        }
        Recorded::Press { key } => vec![vec!["press".into(), key.clone()]],
        Recorded::Scroll { direction, pixels } => {
            vec![vec!["scroll".into(), direction.clone(), pixels.to_string()]]
        }
        Recorded::Open { url } => vec![vec!["open".into(), resolve_url(url)]],
        Recorded::Back => vec![vec!["back".into()]],
        Recorded::Wait { text, ms } => match (text, ms) {
            (Some(t), _) => vec![vec!["wait".into(), "--text".into(), t.clone()]],
            (None, Some(ms)) => vec![vec!["wait".into(), ms.to_string()]],
            _ => vec![],
        },
    }
}

/// Short human label for an action (report/timeline). Never contains secret values.
pub fn label(a: &Recorded) -> String {
    let loc = |l: &Locator| {
        if l.name.is_empty() {
            format!("<{}>", l.role)
        } else {
            format!("<{} \"{}\">", l.role, truncate(&l.name, 60))
        }
    };
    match a {
        Recorded::Click { locator } => format!("click {}", loc(locator)),
        Recorded::Hover { locator } => format!("hover {}", loc(locator)),
        Recorded::Fill { locator, text } => {
            format!("fill {} {:?}", loc(locator), truncate(text, 40))
        }
        Recorded::TypeText { locator, text } => format!(
            "type {} {:?}",
            locator
                .as_ref()
                .map(loc)
                .unwrap_or_else(|| "<focused>".into()),
            truncate(text, 40)
        ),
        Recorded::Select { locator, value } => {
            format!("select {} {:?}", loc(locator), truncate(value, 40))
        }
        Recorded::Press { key } => format!("press {key}"),
        Recorded::Scroll { direction, pixels } => format!("scroll {direction} {pixels}"),
        Recorded::Open { url } => format!("open {}", truncate(url, 80)),
        Recorded::Back => "back".into(),
        Recorded::Wait { text, ms } => match (text, ms) {
            (Some(t), _) => format!("wait for {:?}", truncate(t, 40)),
            (None, Some(ms)) => format!("wait {ms}ms"),
            _ => "wait".into(),
        },
        Recorded::FillSecret { locator, name } => {
            format!("fill {} with secret `{name}`", loc(locator))
        }
    }
}

/// Marker prefix of the one command `replay` builds itself instead of sending to agent-browser.
const SELECT_BY_REF: &str = "\u{1}select";

/// The value an action types or navigates to, if any.
fn text_of(a: &Recorded) -> String {
    match a {
        Recorded::Fill { text, .. } | Recorded::TypeText { text, .. } => text.clone(),
        Recorded::Select { value, .. } => value.clone(),
        Recorded::Open { url } => url.clone(),
        Recorded::Wait { text, .. } => text.clone().unwrap_or_default(),
        _ => String::new(),
    }
}

/// The ref of the nth element with this tag in the snapshot, or `None`.
fn nth_ref(snapshot: &str, tag: &str, nth: usize) -> Option<String> {
    snapshot_entries(snapshot)
        .into_iter()
        .filter(|e| e.role == tag)
        .nth(nth)
        .map(|e| e.ref_id)
}

/// Reads and writes recordings under `<root>/.qaspec/cache/<env>`.
#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
    mode: CacheMode,
}

impl Cache {
    pub fn new(root: &Path, env: &str, mode: CacheMode) -> Self {
        Cache {
            dir: root.join(".qaspec").join("cache").join(env),
            mode,
        }
    }

    pub fn mode(&self) -> CacheMode {
        self.mode
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    /// The recording for this key, if any. A malformed file is ignored (the agent just runs).
    pub fn load(&self, key: &str) -> Option<Recording> {
        if self.mode == CacheMode::Off {
            return None;
        }
        let raw = std::fs::read_to_string(self.path(key)).ok()?;
        match serde_json::from_str::<Recording>(&raw) {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("warning: ignoring unreadable cache entry {key}: {e}");
                None
            }
        }
    }

    /// Stores a recording. Called only after the goal and all of the step's expectations passed.
    pub fn store(&self, r: &Recording) -> Result<()> {
        if self.mode == CacheMode::Off {
            return Ok(());
        }
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path(&r.key);
        std::fs::write(&path, serde_json::to_string_pretty(r)?)?;
        // Same as the saved sign-in state: a recording is part of what the run knows.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// Runs a recording: one `batch` through stdin, so typed secrets never appear in argv.
    /// Returns the error of the first command that failed, so the caller can fall back to the
    /// agent from wherever the page ended up.
    pub fn replay(
        &self,
        r: &Recording,
        browser: &mut Browser,
        resolve_url: &dyn Fn(&str) -> String,
        resolve_value: &dyn Fn(&str) -> Option<String>,
        secret: &mut dyn FnMut(&str) -> Result<String>,
    ) -> Result<Vec<String>> {
        let mut labels = Vec::new();
        for raw in &r.actions {
            // Action values are stored with the goal's placeholders; this run resolves them.
            let a = match raw {
                Recorded::Fill { locator, text } => Recorded::Fill {
                    locator: locator.clone(),
                    text: Templater::default().resolve(text, resolve_value),
                },
                Recorded::TypeText { locator, text } => Recorded::TypeText {
                    locator: locator.clone(),
                    text: Templater::default().resolve(text, resolve_value),
                },
                Recorded::Select { locator, value } => Recorded::Select {
                    locator: locator.clone(),
                    value: Templater::default().resolve(value, resolve_value),
                },
                Recorded::Open { url } => Recorded::Open {
                    url: Templater::default().resolve(url, resolve_value),
                },
                Recorded::Wait { text, ms } => Recorded::Wait {
                    text: text
                        .as_ref()
                        .map(|t| Templater::default().resolve(t, resolve_value)),
                    ms: *ms,
                },
                other => other.clone(),
            };
            if Templater::is_unresolved(&text_of(&a)) {
                bail!(
                    "replay of `{}` stopped: a placeholder has no value in this run",
                    label(raw)
                );
            }
            let filled;
            let a = match &a {
                Recorded::FillSecret { locator, name } => {
                    // The value is read NOW and typed through stdin, so it is in neither argv nor
                    // the cache file, and every log of this run redacts it.
                    let value = secret(name)?;
                    browser.add_redaction(&value);
                    filled = Recorded::Fill {
                        locator: locator.clone(),
                        text: value,
                    };
                    &filled
                }
                other => other,
            };
            let cmds = action_commands(a, resolve_url);
            if cmds.is_empty() {
                labels.push(label(a));
                continue;
            }
            // `select` needs the live ref of the <select>: take a snapshot and match the element
            // by its tag and position, the same way the locator was built.
            if cmds
                .iter()
                .all(|c| c.first().is_some_and(|x| x == SELECT_BY_REF))
            {
                for c in &cmds {
                    let nth: usize = c[2].parse().unwrap_or(0);
                    let snap = browser.snapshot(false)?;
                    let ref_id = [c[1].clone()]
                        .iter()
                        .find_map(|tag| nth_ref(&snap, tag, nth))
                        .ok_or_else(|| {
                            anyhow!("replay of `{}` failed: no {} on the page", label(a), c[1])
                        })?;
                    browser.run(&["select", &format!("@{ref_id}"), &c[3]])?;
                }
                labels.push(label(a));
                continue;
            }
            // One candidate per element kind: the first one agent-browser can locate is the
            // recorded element, the rest are fallbacks for a UI built with another tag.
            let mut last = None;
            let mut ok = false;
            for c in &cmds {
                match browser.batch_stdin(std::slice::from_ref(c)) {
                    Ok(_) => {
                        ok = true;
                        break;
                    }
                    Err(e) => last = Some(e),
                }
            }
            if !ok {
                bail!(
                    "replay of `{}` failed: {}",
                    label(a),
                    last.map(|e| e.to_string()).unwrap_or_default()
                );
            }
            labels.push(label(a));
        }
        browser.wait_load();
        Ok(labels)
    }
}

/// Values that a recording must not freeze: the placeholders of the goal it belongs to, mapped to
/// what they resolved to in the run that recorded it.
///
/// A goal like `add a todo "${params.todo} ${run.id}"` must replay with THIS run's values, not with
/// the ones of the run that recorded it, otherwise the goal text of the run and the page would
/// disagree (`expect.storage.local('lastTodo')` failed exactly like that). So the recorded action
/// values keep the placeholders and are interpolated again at replay time.
#[derive(Debug, Clone, Default)]
pub struct Templater {
    pairs: Vec<(String, String)>,
}

impl Templater {
    /// Builds the substitutions from a raw goal template, given the run's current values.
    pub fn from_goal(raw: &str, resolve: &dyn Fn(&str) -> Option<String>) -> Self {
        let mut pairs = Vec::new();
        for m in regex::Regex::new(r"\$\{\s*([A-Za-z0-9_.\-]+)\s*\}")
            .unwrap()
            .captures_iter(raw)
        {
            let key = m.get(1).unwrap().as_str();
            if let Some(v) = resolve(key) {
                if !v.is_empty() {
                    // Longest values first: `params.todo` must win over a shorter coincidence.
                    pairs.push((v, format!("${{{key}}}")));
                }
            }
        }
        pairs.sort_by_key(|p| std::cmp::Reverse(p.0.len()));
        pairs.dedup_by(|a, b| a.0 == b.0);
        Templater { pairs }
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Puts the placeholders back into a value the agent typed or navigated to.
    pub fn restore(&self, value: &str) -> String {
        let mut out = value.to_string();
        for (v, ph) in &self.pairs {
            if out.contains(v.as_str()) {
                out = out.replace(v.as_str(), ph);
            }
        }
        out
    }

    /// Resolves the placeholders of a recorded value for this run.
    pub fn resolve(&self, value: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
        let mut err = None;
        let out = regex::Regex::new(r"\$\{\s*([A-Za-z0-9_.\-]+)\s*\}")
            .unwrap()
            .replace_all(value, |c: &regex::Captures| match resolve(&c[1]) {
                Some(v) if !v.is_empty() => v,
                _ => {
                    err = Some(c[0].to_string());
                    String::new()
                }
            })
            .to_string();
        match err {
            // A placeholder the current run cannot resolve (a capture that did not happen) means
            // the recording does not apply here: the caller falls back to the agent.
            Some(p) => format!("\u{0}unresolved {p}"),
            None => out,
        }
    }

    /// Whether a resolved value left an unresolved placeholder in it.
    pub fn is_unresolved(value: &str) -> bool {
        value.starts_with('\u{0}')
    }
}

/// Replaces the goal's own values in a recorded action so the recording stays valid for the next
/// run. Returns `None` when an action cannot be templated (e.g. an action typed a value that looks
/// like a placeholder but is not one), which simply drops it from the recording.
pub fn template_actions(acts: &[Recorded], t: &Templater) -> Vec<Recorded> {
    acts.iter()
        .map(|a| match a {
            Recorded::Fill { locator, text } => Recorded::Fill {
                locator: locator.clone(),
                text: t.restore(text),
            },
            Recorded::TypeText { locator, text } => Recorded::TypeText {
                locator: locator.clone(),
                text: t.restore(text),
            },
            Recorded::Select { locator, value } => Recorded::Select {
                locator: locator.clone(),
                value: t.restore(value),
            },
            Recorded::Open { url } => Recorded::Open {
                url: t.restore(url),
            },
            Recorded::Wait { text, ms } => Recorded::Wait {
                text: text.as_ref().map(|x| t.restore(x)),
                ms: *ms,
            },
            other => other.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAP: &str = r#"- navigation
  - StaticText "Todo demo"
  - link "Home" [ref=e1]
  - link "Todos" [ref=e2]
- main
  - heading "Todos" [level=1, ref=e5]
  - list
    - listitem [level=1]
      - StaticText "Buy milk"
  - LabelText
    - StaticText "New todo"
    - textbox "New todo" [ref=e7]
  - button "Add" [ref=e6]
  - button "Add" [ref=e8]
"#;

    #[test]
    fn parses_snapshot_lines() {
        let e = snapshot_entries(SNAP);
        let by = |id: &str| e.iter().find(|x| x.ref_id == id).cloned().unwrap();
        assert_eq!(by("e2").role, "link");
        assert_eq!(by("e2").name, "Todos");
        assert_eq!(by("e7").role, "textbox");
        assert_eq!(by("e7").name, "New todo");
        assert_eq!(by("e5").name, "Todos");
        // A nameless element takes its name from the attributes when there is one.
        assert_eq!(
            locator_of("- textbox [placeholder=\"Search\", ref=e4]", "e4"),
            Some(Locator {
                role: "textbox".into(),
                name: "Search".into(),
                nth: 0
            })
        );
        assert_eq!(locator_of(SNAP, "e99"), None);
    }

    #[test]
    fn ambiguous_locator_gets_nth() {
        // Two buttons named "Add": the second one is only reachable by index.
        assert_eq!(
            locator_of(SNAP, "e6"),
            Some(Locator {
                role: "button".into(),
                name: "Add".into(),
                nth: 0
            })
        );
        assert_eq!(
            locator_of(SNAP, "e8"),
            Some(Locator {
                role: "button".into(),
                name: "Add".into(),
                nth: 1
            })
        );
        assert_eq!(
            locator_of(SNAP, "e2"),
            Some(Locator {
                role: "link".into(),
                name: "Todos".into(),
                nth: 0
            })
        );
    }

    #[test]
    fn roles_map_to_tags_agent_browser_finds() {
        assert_eq!(role_candidates("link"), vec!["a"]);
        assert_eq!(role_candidates("button")[0], "button");
        assert_eq!(role_candidates("textbox")[0], "input");
        assert_eq!(role_candidates("heading")[0], "h1");
    }

    #[test]
    fn commands_use_find_not_refs() {
        let cmds = action_commands(
            &Recorded::Click {
                locator: Locator {
                    role: "link".into(),
                    name: "Todos".into(),
                    nth: 0,
                },
            },
            &|u: &str| format!("http://x{u}"),
        );
        assert_eq!(cmds[0], ["find", "role", "a", "click", "--name", "Todos"]);
        assert!(cmds.iter().all(|c| !c.iter().any(|a| a.starts_with("@e"))));

        let cmds = action_commands(
            &Recorded::Fill {
                locator: Locator {
                    role: "textbox".into(),
                    name: "New todo".into(),
                    nth: 0,
                },
                text: "hello".into(),
            },
            &|u: &str| u.into(),
        );
        assert_eq!(
            cmds[0],
            ["find", "role", "input", "fill", "hello", "--name", "New todo"]
        );

        // An ambiguous element still tries its name first, then falls back to its position.
        let cmds = action_commands(
            &Recorded::Click {
                locator: Locator {
                    role: "button".into(),
                    name: "Add".into(),
                    nth: 1,
                },
            },
            &|u: &str| u.into(),
        );
        assert_eq!(
            cmds[0],
            ["find", "role", "button", "click", "--name", "Add"]
        );
        assert!(
            cmds.contains(&vec![
                "find".to_string(),
                "nth".to_string(),
                "1".to_string(),
                "button".to_string(),
                "click".to_string()
            ]),
            "an ambiguous element can be addressed by position: {cmds:?}"
        );
        // A unique element also gets the role-only last resort: agent-browser 0.27's
        // `find role <x> --name <n>` silently finds nothing on some tags (e.g. `input`).
        let cmds = action_commands(
            &Recorded::Fill {
                locator: Locator {
                    role: "textbox".into(),
                    name: "Email".into(),
                    nth: 0,
                },
                text: "qa@example.com".into(),
            },
            &|u: &str| u.into(),
        );
        assert_eq!(
            cmds[0],
            [
                "find",
                "role",
                "input",
                "fill",
                "qa@example.com",
                "--name",
                "Email"
            ]
        );
        assert!(
            cmds.contains(&vec![
                "find".to_string(),
                "label".to_string(),
                "Email".to_string(),
                "fill".to_string(),
                "qa@example.com".to_string()
            ]),
            "an input is also reachable through its <label>: {cmds:?}"
        );
        assert!(cmds.contains(&vec![
            "find".to_string(),
            "role".to_string(),
            "input".to_string(),
            "fill".to_string(),
            "qa@example.com".to_string()
        ]));

        let cmds = action_commands(
            &Recorded::Open {
                url: "/todos".into(),
            },
            &|u: &str| format!("http://app{u}"),
        );
        assert_eq!(cmds[0], ["open", "http://app/todos"]);
    }

    #[test]
    fn keys_change_with_every_component() {
        let base = || KeyParts {
            project: "app".into(),
            identity: "qa".into(),
            file: "specs/a.qa.ts".into(),
            suite: "todos".into(),
            step: "add".into(),
            goal: "add a todo".into(),
            fingerprint: "abc".into(),
        };
        let k = compute_key(&base());
        assert_eq!(k.len(), 16);
        assert_eq!(k, compute_key(&base()), "same inputs, same key");
        for f in [
            |p: &mut KeyParts| p.project = "admin".into(),
            |p: &mut KeyParts| p.identity = "viewer".into(),
            |p: &mut KeyParts| p.file = "specs/b.qa.ts".into(),
            |p: &mut KeyParts| p.suite = "other".into(),
            |p: &mut KeyParts| p.step = "remove".into(),
            |p: &mut KeyParts| p.goal = "add a todo \u{1f600}".into(),
            |p: &mut KeyParts| p.fingerprint = "def".into(),
        ] {
            let mut other = base();
            f(&mut other);
            assert_ne!(k, compute_key(&other));
        }
        // A field separator prevents "ab"+"c" from colliding with "a"+"bc".
        assert_ne!(
            compute_key(&KeyParts {
                identity: "ab".into(),
                goal: "c".into(),
                ..base()
            }),
            compute_key(&KeyParts {
                identity: "a".into(),
                goal: "bc".into(),
                ..base()
            })
        );
    }

    #[test]
    fn fingerprint_follows_the_ui_not_the_text() {
        let a = fingerprint("http://x/app/todos?a=1", SNAP);
        assert_eq!(
            a,
            fingerprint("http://y:9/app/todos?a=1", SNAP),
            "host is not part of it"
        );
        // Extra static text on the page does not change the fingerprint.
        let with_text = format!("{SNAP}  - StaticText \"You have 5 todos.\"\n");
        assert_eq!(a, fingerprint("http://x/app/todos?a=1", &with_text));
        // A different path, or a different control, does.
        assert_ne!(a, fingerprint("http://x/app/", SNAP));
        assert_ne!(
            a,
            fingerprint(
                "http://x/app/todos?a=1",
                &SNAP.replace("button \"Add\" [ref=e6]", "button \"Save\" [ref=e6]")
            )
        );
    }

    fn recording(key: &str) -> Recording {
        Recording {
            schema_version: schema(),
            key: key.into(),
            project: "app".into(),
            identity: "qa".into(),
            file: "specs/a.qa.ts".into(),
            suite: "s".into(),
            step: "st".into(),
            goal: "add a todo".into(),
            start_path: "/todos".into(),
            fingerprint: "ff".into(),
            actions: vec![
                Recorded::Click {
                    locator: Locator {
                        role: "link".into(),
                        name: "Todos".into(),
                        nth: 0,
                    },
                },
                Recorded::Fill {
                    locator: Locator {
                        role: "textbox".into(),
                        name: "New todo".into(),
                        nth: 0,
                    },
                    text: "buy milk".into(),
                },
                Recorded::FillSecret {
                    locator: Locator {
                        role: "textbox".into(),
                        name: "Password".into(),
                        nth: 0,
                    },
                    name: "password".into(),
                },
                Recorded::Press {
                    key: "Enter".into(),
                },
                Recorded::Back,
            ],
            labels: vec![],
        }
    }

    #[test]
    fn record_and_replay_files_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let c = Cache::new(dir.path(), "local", CacheMode::Auto);
        assert!(c.dir().ends_with(".qaspec/cache/local"));
        let key = compute_key(&KeyParts {
            project: "app".into(),
            identity: "qa".into(),
            file: "specs/a.qa.ts".into(),
            suite: "s".into(),
            step: "st".into(),
            goal: "add a todo".into(),
            fingerprint: "ff".into(),
        });
        let r = recording(&key);
        assert!(c.load(&key).is_none(), "nothing recorded yet");
        c.store(&r).unwrap();
        let back = c.load(&key).expect("recording is there");
        assert_eq!(back.actions, r.actions);
        assert_eq!(back.goal, r.goal);
        assert_eq!(back.start_path, "/todos");
        assert_eq!(back.schema_version, SCHEMA);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(c.path(&key))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o777,
                0o600,
                "a cache file is private like the saved state"
            );
        }
        // No secret value anywhere in the file.
        let raw = std::fs::read_to_string(c.path(&key)).unwrap();
        assert!(raw.contains("\"password\""), "the secret name is stored");
        assert!(!raw.contains("s3cret"), "the secret value never is");
    }

    #[test]
    fn off_mode_never_reads_or_writes() {
        let dir = tempfile::tempdir().unwrap();
        let c = Cache::new(dir.path(), "local", CacheMode::Off);
        c.store(&recording("k")).unwrap();
        assert!(c.load("k").is_none());
        assert!(!c.path("k").exists());
    }

    #[test]
    fn broken_recording_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let c = Cache::new(dir.path(), "local", CacheMode::Auto);
        std::fs::create_dir_all(c.dir()).unwrap();
        std::fs::write(c.path("bad"), "{not json").unwrap();
        assert!(c.load("bad").is_none());
    }

    #[test]
    fn placeholders_survive_a_recording() {
        let resolve = |k: &str| match k {
            "params.todo" => Some("Write qaspec docs".to_string()),
            "run.id" => Some("e1b331d".to_string()),
            _ => None,
        };
        let t = Templater::from_goal(
            "add a new todo with the text \"${params.todo} ${run.id}\"",
            &resolve,
        );
        assert!(!t.is_empty());
        // What the agent typed in the recording run goes back to placeholders.
        assert_eq!(
            t.restore("add a new todo with the text \"Write qaspec docs e1b331d\""),
            "add a new todo with the text \"${params.todo} ${run.id}\""
        );
        // And the next run resolves them to ITS values.
        assert_eq!(
            t.resolve("${params.todo} ${run.id}", &|k| match k {
                "params.todo" => Some("Write qaspec docs".to_string()),
                "run.id" => Some("beef123".to_string()),
                _ => None,
            }),
            "Write qaspec docs beef123"
        );
        // A value that is not a placeholder is untouched.
        assert_eq!(t.restore("Sign in"), "Sign in");
        // A placeholder with no value in this run stops the replay (the agent takes over).
        let bad = t.resolve("${params.gone}", &|_| None);
        assert!(Templater::is_unresolved(&bad));
    }

    #[test]
    fn templating_covers_the_actions_that_carry_values() {
        let resolve = |k: &str| (k == "run.id").then(|| "abc123".to_string());
        let t = Templater::from_goal("do it with ${run.id}", &resolve);
        let loc = Locator {
            role: "textbox".into(),
            name: "New todo".into(),
            nth: 0,
        };
        let acts = vec![
            Recorded::Fill {
                locator: loc.clone(),
                text: "note abc123".into(),
            },
            Recorded::TypeText {
                locator: Some(loc.clone()),
                text: "abc123".into(),
            },
            Recorded::Select {
                locator: loc.clone(),
                value: "abc123".into(),
            },
            Recorded::Open {
                url: "/x/abc123".into(),
            },
            Recorded::Wait {
                text: Some("done abc123".into()),
                ms: None,
            },
            Recorded::Click {
                locator: loc.clone(),
            },
            Recorded::Press {
                key: "Enter".into(),
            },
        ];
        let out = template_actions(&acts, &t);
        assert_eq!(
            out[0],
            Recorded::Fill {
                locator: loc.clone(),
                text: "note ${run.id}".into()
            }
        );
        assert_eq!(
            out[1],
            Recorded::TypeText {
                locator: Some(loc.clone()),
                text: "${run.id}".into()
            }
        );
        assert_eq!(
            out[2],
            Recorded::Select {
                locator: loc.clone(),
                value: "${run.id}".into()
            }
        );
        assert_eq!(
            out[3],
            Recorded::Open {
                url: "/x/${run.id}".into()
            }
        );
        assert_eq!(
            out[4],
            Recorded::Wait {
                text: Some("done ${run.id}".into()),
                ms: None
            }
        );
        assert_eq!(out[5], acts[5], "a click carries no value");
        assert_eq!(out[6], acts[6], "a key press carries no value");
    }

    #[test]
    fn mode_parsing_and_labels() {
        assert_eq!("auto".parse::<CacheMode>().unwrap(), CacheMode::Auto);
        assert_eq!("strict".parse::<CacheMode>().unwrap(), CacheMode::Strict);
        assert_eq!("off".parse::<CacheMode>().unwrap(), CacheMode::Off);
        assert!("nope".parse::<CacheMode>().is_err());
        assert_eq!(
            label(&Recorded::FillSecret {
                locator: Locator {
                    role: "textbox".into(),
                    name: "Password".into(),
                    nth: 0
                },
                name: "password".into()
            }),
            "fill <textbox \"Password\"> with secret `password`"
        );
    }
}
