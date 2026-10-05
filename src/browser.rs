//! Thin typed client over the `agent-browser` CLI (`--json`). It is the only way qaspec talks to
//! the browser. One `Browser` = one agent-browser session = one Chromium for the whole run.

use crate::config::{AppliedEmulation, Emulation, Viewport};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct Browser {
    cmd: String,
    session: String,
    headed: bool,
    pub calls: u64,
    pub log: Vec<String>,
    /// Values that must never appear in logs (secrets filled during the run).
    redact: Vec<String>,
    owns_session: bool,
    /// Signal cursors: events before these offsets belong to earlier steps.
    cursor: (usize, usize, usize),
    /// Label of the active tab, to know which tabs got an emulation.
    active_tab: Option<String>,
    /// Emulation currently applied to the active tab (to restore it between suites).
    applied: Option<Emulation>,
    /// What agent-browser reported for `applied` (size and mobile flag), for the agent context.
    applied_info: Option<AppliedEmulation>,
    /// Tabs that got an emulation in this run, to put them back afterwards.
    emulated_tabs: BTreeSet<String>,
    /// The window size the browser had before qaspec emulated anything: agent-browser has no
    /// "clear emulation", so this is what "back to the browser default" means.
    desktop: Option<Viewport>,
    /// Whether a `set device` has overridden the user agent in this session. agent-browser can
    /// only put it back with `--user-agent ""` (an empty override means "use the real one").
    ua_overridden: bool,
}

#[derive(Debug, Clone)]
pub struct Signals {
    pub console_errors: Vec<String>,
    pub page_errors: Vec<String>,
    pub requests: Vec<Request>,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub resource_type: String,
}

impl Browser {
    pub fn new(cmd: &str, session: &str, headed: bool, owns_session: bool) -> Self {
        Browser {
            cmd: cmd.into(),
            session: session.into(),
            headed,
            calls: 0,
            log: vec![],
            redact: vec![],
            owns_session,
            cursor: (0, 0, 0),
            active_tab: None,
            applied: None,
            applied_info: None,
            emulated_tabs: BTreeSet::new(),
            desktop: None,
            ua_overridden: false,
        }
    }

    pub fn session(&self) -> &str {
        &self.session
    }

    pub fn add_redaction(&mut self, v: &str) {
        if !v.is_empty() && !self.redact.contains(&v.to_string()) {
            self.redact.push(v.to_string());
        }
    }

    pub fn redact(&self, s: &str) -> String {
        let mut out = s.to_string();
        for r in &self.redact {
            out = out.replace(r, "<secret>");
        }
        out
    }

    fn base(&self) -> Command {
        self.base_with(&[])
    }

    /// `base()` plus global flags that only apply to this one command.
    fn base_with(&self, flags: &[&str]) -> Command {
        let mut c = Command::new(&self.cmd);
        c.arg("--session").arg(&self.session).arg("--json");
        if self.headed {
            c.arg("--headed");
        }
        c.args(flags);
        c
    }

    /// Runs one agent-browser command and returns its `data`.
    pub fn run(&mut self, args: &[&str]) -> Result<Value> {
        self.calls += 1;
        let t = Instant::now();
        let out = self
            .base()
            .args(args)
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("cannot run `{}` (is agent-browser installed?)", self.cmd))?;
        let line = format!(
            "{} ({} ms)",
            self.redact(&args.join(" ")),
            t.elapsed().as_millis()
        );
        self.log.push(line);
        parse_response(&out.stdout, &out.stderr, &args.join(" "))
    }

    /// Runs commands through `batch` on stdin, so arguments (secrets) never appear in argv.
    pub fn batch_stdin(&mut self, cmds: &[Vec<String>]) -> Result<Vec<Value>> {
        self.calls += 1;
        let body = serde_json::to_string(cmds)?;
        let mut child = self
            .base()
            .args(["batch", "--bail"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("cannot run `{}`", self.cmd))?;
        child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("no stdin"))?
            .write_all(body.as_bytes())?;
        let out = wait_timeout(child, Duration::from_secs(90))?;
        self.log.push(format!(
            "batch [{}]",
            cmds.iter()
                .map(|c| c.first().cloned().unwrap_or_default())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| {
            anyhow!(
                "agent-browser batch: unexpected output: {}",
                self.redact(&String::from_utf8_lossy(&out.stdout))
            )
        })?;
        let arr = v
            .as_array()
            .ok_or_else(|| anyhow!("agent-browser batch: expected a JSON array"))?;
        let mut res = Vec::new();
        for r in arr {
            if r["success"] != Value::Bool(true) {
                bail!(
                    "{}",
                    self.redact(r["error"].as_str().unwrap_or("batch command failed"))
                );
            }
            res.push(r["result"].clone());
        }
        Ok(res)
    }

    /// Navigates. agent-browser waits for the `load` event and times out (~25s) on apps that
    /// keep loading; if the tab did reach the target, that timeout is not a failure.
    pub fn open(&mut self, url: &str) -> Result<()> {
        match self.run(&["open", url]) {
            Ok(_) => Ok(()),
            Err(e) if e.to_string().contains("timed out") => {
                let now = self.url().unwrap_or_default();
                if same_document(&now, url) {
                    self.log.push(format!(
                        "open {url}: load event timed out, page is there; continuing"
                    ));
                    Ok(())
                } else {
                    Err(e)
                }
            }
            Err(e) => Err(e),
        }
    }

    pub fn url(&mut self) -> Result<String> {
        Ok(self.run(&["get", "url"])?["url"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    pub fn title(&mut self) -> Result<String> {
        Ok(self.run(&["get", "title"])?["title"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// Accessibility snapshot. `interactive` limits it to controls; otherwise it includes text
    /// (`-c` is not used for the full view: in agent-browser 0.27 it drops static text).
    pub fn snapshot(&mut self, interactive: bool) -> Result<String> {
        let d = if interactive {
            self.run(&["snapshot", "-i", "-c"])?
        } else {
            self.run(&["snapshot"])?
        };
        Ok(d["snapshot"].as_str().unwrap_or_default().to_string())
    }

    pub fn eval(&mut self, js: &str) -> Result<Value> {
        Ok(self.run(&["eval", js])?["result"].clone())
    }

    /// Waits until the network is idle (agent-browser caps it; `load` can take 25s+ on some pages).
    pub fn wait_load(&mut self) {
        let _ = self.run(&["wait", "--load", "networkidle"]);
    }

    /// Opens a URL and waits for the page to settle.
    pub fn goto(&mut self, url: &str) -> Result<()> {
        self.open(url)?;
        self.wait_load();
        Ok(())
    }

    fn raw_signals(&mut self) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
        let r = self.batch_stdin(&[
            vec!["console".into()],
            vec!["errors".into()],
            vec!["network".into(), "requests".into()],
        ])?;
        let arr = |v: &Value, k: &str| v[k].as_array().cloned().unwrap_or_default();
        Ok((
            arr(&r[0], "messages"),
            arr(&r[1], "errors"),
            arr(&r[2], "requests"),
        ))
    }

    /// Starts a new signal window. agent-browser 0.27's `errors --clear` does not clear and the
    /// logs are shared by all tabs, so windows are tracked with offsets instead of clearing.
    pub fn mark_signals(&mut self) -> Result<()> {
        let (c, e, n) = self.raw_signals()?;
        self.cursor = (c.len(), e.len(), n.len());
        Ok(())
    }

    /// Console errors, page errors and requests since the last `mark_signals`.
    pub fn signals(&mut self) -> Result<Signals> {
        let (c, e, n) = self.raw_signals()?;
        // A log shorter than its cursor was reset (e.g. the daemon restarted): read it whole.
        let tail = |v: Vec<Value>, at: usize| if v.len() >= at { v[at..].to_vec() } else { v };
        let (c, e, n) = (
            tail(c, self.cursor.0),
            tail(e, self.cursor.1),
            tail(n, self.cursor.2),
        );
        let console_errors = c
            .iter()
            .filter(|m| m["type"].as_str() == Some("error"))
            .map(|m| m["text"].as_str().unwrap_or_default().to_string())
            .collect();
        let page_errors = e
            .iter()
            .map(|x| x["text"].as_str().unwrap_or_default().to_string())
            .collect();
        let requests = n
            .iter()
            .map(|r| Request {
                method: r["method"].as_str().unwrap_or("GET").to_string(),
                url: r["url"].as_str().unwrap_or_default().to_string(),
                status: r["status"].as_u64().map(|s| s as u16),
                resource_type: r["resourceType"].as_str().unwrap_or_default().to_string(),
            })
            .collect();
        Ok(Signals {
            console_errors,
            page_errors,
            requests,
        })
    }

    /// Visible text of the page body.
    pub fn text(&mut self) -> Result<String> {
        Ok(self.run(&["get", "text", "body"])?["text"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// Switches to the labelled tab, creating it if needed. Returns true if it was created.
    pub fn use_tab(&mut self, label: &str) -> Result<bool> {
        // agent-browser applies emulation per target and does not inherit it to other tabs,
        // so switching tabs drops what this client thinks is applied.
        if self.active_tab.as_deref() != Some(label) {
            self.applied = None;
            self.applied_info = None;
        }
        let tabs = self.run(&["tab", "list"])?;
        let list = tabs["tabs"].as_array().cloned().unwrap_or_default();
        if list.iter().any(|t| t["label"].as_str() == Some(label)) {
            self.run(&["tab", label])?;
            self.active_tab = Some(label.to_string());
            return Ok(false);
        }
        // Reuse the initial unlabelled blank tab for the first project.
        if let Some(t) = list
            .iter()
            .find(|t| t["label"].is_null() && t["url"].as_str() == Some("about:blank"))
        {
            if list.len() == 1 {
                let id = t["tabId"].as_str().unwrap_or("t1").to_string();
                self.run(&["tab", "new", "--label", label])?;
                let _ = self.run(&["tab", "close", &id]);
                return Ok(true);
            }
        }
        self.run(&["tab", "new", "--label", label])?;
        self.active_tab = Some(label.to_string());
        Ok(true)
    }

    /// Emulates a device (`set device`) or a viewport (`set viewport w h [scale]`) on the
    /// ACTIVE tab. agent-browser applies emulation per target and does not inherit it to new
    /// tabs, so this must run after `use_tab` for the tab the suite will drive.
    ///
    /// A device also overrides the user agent; `set viewport` only sets size and scale.
    pub fn set_emulation(&mut self, emu: &Emulation) -> Result<AppliedEmulation> {
        if self.applied.as_ref() == Some(emu) {
            if let Some(i) = self.applied_info {
                return Ok(i);
            }
        }
        let info = if let Some(d) = &emu.device {
            let d = d.clone();
            let out = self.run(&["set", "device", &d])?;
            // `set device` also overrides the user agent (see `restore_user_agent`).
            self.ua_overridden = true;
            let info = applied_of(&out);
            self.log.push(format!(
                "emulating device {d} ({}x{} at {}x, mobile: {})",
                info.width,
                info.height,
                json_num(&out, "deviceScaleFactor"),
                info.mobile
            ));
            info
        } else if let Some(v) = &emu.viewport {
            let mut args = vec![
                "set".to_string(),
                "viewport".to_string(),
                v.width.to_string(),
                v.height.to_string(),
            ];
            if let Some(s) = v.scale {
                args.push(format!("{s}"));
            }
            let argv: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let out = self.run(&argv)?;
            let info = applied_of(&out);
            self.log.push(format!(
                "emulating viewport {}x{} at {}x",
                v.width,
                v.height,
                json_num(&out, "deviceScaleFactor")
            ));
            info
        } else {
            // No device and no viewport: back to the plain browser window.
            self.log.push("emulation cleared".to_string());
            AppliedEmulation {
                width: 0,
                height: 0,
                mobile: false,
            }
        };
        self.applied = Some(emu.clone());
        self.applied_info = Some(info);
        if let Some(t) = &self.active_tab {
            self.emulated_tabs.insert(t.clone());
        }
        Ok(info)
    }

    /// The window size of the browser before any emulation, read once from the page.
    /// agent-browser has no "clear emulation" command, so this is how "the browser default"
    /// is restored between suites.
    pub fn remember_desktop_viewport(&mut self) {
        if self.desktop.is_some() {
            return;
        }
        let js = "JSON.stringify({w: innerWidth, h: innerHeight})";
        if let Ok(Value::String(s)) = self.eval(js) {
            if let Ok(v) = serde_json::from_str::<Value>(&s) {
                let (w, h) = (v["w"].as_f64(), v["h"].as_f64());
                if let (Some(w), Some(h)) = (w, h) {
                    if w >= 1.0 && h >= 1.0 {
                        self.desktop = Some(Viewport {
                            width: w as u32,
                            height: h as u32,
                            scale: None,
                        });
                    }
                }
            }
        }
    }

    /// An emulation that puts the tab back to the size the browser had before qaspec
    /// emulated anything (`None` when the size could not be read).
    pub fn desktop_emulation(&self) -> Option<Emulation> {
        self.desktop.map(|v| Emulation {
            device: None,
            viewport: Some(v),
        })
    }

    /// Puts `target` back on every tab this run emulated, so a desktop suite does not keep
    /// running on the phone size or with the phone user agent.
    ///
    /// agent-browser 0.27 has no "clear emulation" and no "clear user agent" command: `set
    /// viewport` does not undo a `set device`, and the user-agent override survives it. The
    /// only way back is the global `--user-agent ""` (an empty override means "use the real
    /// user agent"), which must run on each tab while it is the active one. Without this a
    /// suite without a `device` would still be served `... iPhone ...`.
    pub fn restore_emulation_on_all(&mut self, target: Option<&Emulation>) -> Result<()> {
        let Some(target) = target else {
            self.emulated_tabs.clear();
            return Ok(());
        };
        for label in std::mem::take(&mut self.emulated_tabs) {
            self.use_tab(&label)?;
            if self.ua_overridden {
                self.restore_user_agent()?;
            }
            self.set_emulation(target)?;
        }
        Ok(())
    }

    /// Drops the device user-agent override on the active tab, going back to the browser's own.
    /// Runs a harmless `eval` so the effect lands on the tab that is active now.
    pub fn restore_user_agent(&mut self) -> Result<()> {
        if !self.ua_overridden {
            return Ok(());
        }
        let t = Instant::now();
        let out = self
            .base_with(&["--user-agent", ""])
            .args(["eval", "1"])
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("cannot run `{}`", self.cmd))?;
        self.calls += 1;
        parse_response(&out.stdout, &out.stderr, "eval 1")?;
        self.log.push(format!(
            "user agent restored to the browser default ({} ms)",
            t.elapsed().as_millis()
        ));
        self.ua_overridden = false;
        Ok(())
    }

    pub fn state_save(&mut self, path: &str) -> Result<()> {
        self.run(&["state", "save", path]).map(|_| ())
    }

    pub fn state_load(&mut self, path: &str) -> Result<()> {
        self.run(&["state", "load", path]).map(|_| ())
    }

    pub fn close(&mut self) {
        if self.owns_session {
            let _ = self
                .base()
                .arg("close")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

/// Waits for the child with a deadline while draining stdout/stderr in threads, so a large
/// output (e.g. hundreds of KB of network requests) can never fill the pipe and deadlock.
fn wait_timeout(mut child: std::process::Child, limit: Duration) -> Result<std::process::Output> {
    use std::io::Read;
    fn drain<R: Read + Send + 'static>(r: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut buf);
            }
            buf
        })
    }
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            bail!("agent-browser timed out after {}s", limit.as_secs());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(std::process::Output {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

pub fn parse_response(stdout: &[u8], stderr: &[u8], what: &str) -> Result<Value> {
    let text = String::from_utf8_lossy(stdout);
    let v: Value = match serde_json::from_str(text.trim()) {
        Ok(v) => v,
        Err(_) => {
            let err = String::from_utf8_lossy(stderr);
            let msg = if text.trim().is_empty() {
                err.trim().to_string()
            } else {
                text.trim().to_string()
            };
            bail!(
                "agent-browser `{}`: {}",
                first_word(what),
                truncate(&msg, 300)
            );
        }
    };
    if v["success"] == Value::Bool(true) {
        Ok(v["data"].clone())
    } else {
        bail!(
            "{}",
            v["error"]
                .as_str()
                .unwrap_or("agent-browser command failed")
        )
    }
}

/// Size and mobile flag of a `set device` / `set viewport` response.
fn applied_of(v: &Value) -> AppliedEmulation {
    let num = |k: &str| v[k].as_f64().unwrap_or_default();
    AppliedEmulation {
        width: num("width") as u32,
        height: num("height") as u32,
        mobile: v["mobile"].as_bool().unwrap_or(false),
    }
}

fn json_num(v: &Value, key: &str) -> String {
    match v[key].as_f64() {
        Some(n) if n.fract() == 0.0 => format!("{}", n as i64),
        Some(n) => format!("{n}"),
        None => "?".into(),
    }
}

fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or(s)
}

/// True if `now` is the page `target` points at (ignoring a trailing slash; redirects that
/// keep the host, like `/` → `/<workspace>`, count as arrived).
fn same_document(now: &str, target: &str) -> bool {
    let host = |u: &str| {
        u.split_once("://")
            .map(|(_, r)| r.split('/').next().unwrap_or("").to_string())
    };
    match (host(now), host(target)) {
        (Some(a), Some(b)) => a == b && now != "about:blank",
        _ => now.trim_end_matches('/') == target.trim_end_matches('/'),
    }
}

pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

/// Matches a request against `METHOD /path/**` or `/path` (method optional; `*` = one segment,
/// `**` = anything). The pattern matches the URL path (and query), or the full URL if it has a scheme.
pub fn request_matches(pattern: &str, method: &str, url: &str) -> bool {
    let (m, p) = match pattern.split_once(' ') {
        Some((m, p)) if m.chars().all(|c| c.is_ascii_uppercase()) => (Some(m), p.trim()),
        _ => (None, pattern.trim()),
    };
    if let Some(m) = m {
        if !m.eq_ignore_ascii_case(method) {
            return false;
        }
    }
    let target = if p.contains("://") {
        url.to_string()
    } else {
        match url.split_once("://") {
            Some((_, rest)) => rest
                .find('/')
                .map(|i| rest[i..].to_string())
                .unwrap_or_else(|| "/".into()),
            None => url.to_string(),
        }
    };
    let target_no_query = target.split('?').next().unwrap_or(&target).to_string();
    glob_match(p, &target) || glob_match(p, &target_no_query)
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let mut re = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '*' {
            if chars.get(i + 1) == Some(&'*') {
                re.push_str(".*");
                i += 2;
                continue;
            }
            re.push_str("[^/]*");
        } else {
            re.push_str(&regex::escape(&chars[i].to_string()));
        }
        i += 1;
    }
    re.push('$');
    regex::Regex::new(&re)
        .map(|r| r.is_match(text))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching() {
        assert!(request_matches(
            "POST /api/cart/**",
            "POST",
            "https://x.com/api/cart/1/items"
        ));
        assert!(!request_matches(
            "POST /api/cart/**",
            "GET",
            "https://x.com/api/cart/1"
        ));
        assert!(request_matches(
            "/api/*",
            "GET",
            "http://localhost:3000/api/users?x=1"
        ));
        assert!(!request_matches(
            "/api/*",
            "GET",
            "http://localhost:3000/api/users/2"
        ));
        assert!(request_matches("GET /", "GET", "http://localhost:3000"));
        assert!(request_matches(
            "https://cdn.x.com/**",
            "GET",
            "https://cdn.x.com/a.js"
        ));
    }

    #[test]
    fn responses() {
        assert_eq!(
            parse_response(br#"{"success":true,"data":{"a":1},"error":null}"#, b"", "x").unwrap()
                ["a"],
            1
        );
        assert!(parse_response(
            br#"{"success":false,"data":null,"error":"Unknown ref: e9"}"#,
            b"",
            "click"
        )
        .unwrap_err()
        .to_string()
        .contains("Unknown ref"));
        assert!(parse_response(b"", b"boom", "open x")
            .unwrap_err()
            .to_string()
            .contains("boom"));
    }

    #[test]
    fn large_output_does_not_deadlock() {
        let child = std::process::Command::new("sh")
            .args(["-c", "head -c 2000000 /dev/zero"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let out = wait_timeout(child, Duration::from_secs(10)).unwrap();
        assert_eq!(out.stdout.len(), 2_000_000);
    }

    #[test]
    fn arrival() {
        assert!(same_document("https://a.com/ws/1", "https://a.com/"));
        assert!(!same_document("https://login.b.com/", "https://a.com/"));
        assert!(!same_document("about:blank", "https://a.com/"));
    }

    #[test]
    fn redaction() {
        let mut b = Browser::new("agent-browser", "s", false, false);
        b.add_redaction("hunter2");
        assert_eq!(b.redact("fill @e1 hunter2"), "fill @e1 <secret>");
    }
}
