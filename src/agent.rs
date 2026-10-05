//! The agent: an LLM drives agent-browser through tools to reach a goal, and judges
//! natural-language expectations against the screen and the step's technical signals.

use crate::browser::{truncate, Browser, Signals};
use crate::cache::{self, Recorded};
use crate::llm::{extract_json, Llm};
use anyhow::Result;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Passed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub verdict: Verdict,
    pub reason: String,
    pub actions: Vec<String>,
    /// Browser actions of this goal, with semantic locators, ready to be cached (see `cache`).
    pub recorded: Vec<Recorded>,
}

pub struct Context<'a> {
    pub project: &'a str,
    pub base_url: &'a str,
    pub locale: Option<&'a str>,
    pub username: Option<&'a str>,
    /// Secret names available to `fill_secret` (values resolved lazily by `secret`).
    pub secret_names: Vec<String>,
    /// Device emulation of this suite, when there is one: `iPhone 14, 390x844, touch`.
    pub device: Option<String>,
    pub history: &'a [String],
}

const SYSTEM: &str = "You are qaspec, a meticulous QA engineer testing a web app through a real browser.\n\
You see the page as an accessibility snapshot where interactive elements have refs like e3; use them as `ref`.\n\
Refs change after the page changes: when unsure, call `snapshot` again.\n\
Besides the screen you can read the browser console, page errors and network requests, and evaluate read-only JavaScript.\n\
Work step by step with the tools. Never invent results. When the goal is reached call `done` with verdict `passed`; \
if the app prevents it (a bug, an error, missing feature) call `done` with `failed` and say why; if something outside the \
product blocks you (environment down, missing data, credentials rejected) use `blocked`.\n\
Passwords and other secrets are never shown to you: use `fill_secret` with the secret name.";

fn tools(secret_names: &[String]) -> Value {
    let f = |name: &str, desc: &str, props: Value, req: &[&str]| {
        json!({"type":"function","function":{"name":name,"description":desc,
            "parameters":{"type":"object","properties":props,"required":req}}})
    };
    let mut t = vec![
        f(
            "snapshot",
            "Get the current accessibility snapshot of the page (with refs).",
            json!({"full":{"type":"boolean","description":"Include non-interactive text (default true)."}}),
            &[],
        ),
        f(
            "click",
            "Click an element by ref.",
            json!({"ref":{"type":"string"}}),
            &["ref"],
        ),
        f(
            "fill",
            "Clear an input and type text into it.",
            json!({"ref":{"type":"string"},"text":{"type":"string"}}),
            &["ref", "text"],
        ),
        f(
            "type_text",
            "Type text with real keystrokes into the focused element or a ref.",
            json!({"ref":{"type":"string"},"text":{"type":"string"}}),
            &["text"],
        ),
        f(
            "press",
            "Press a key, e.g. Enter, Tab, Escape, Control+a.",
            json!({"key":{"type":"string"}}),
            &["key"],
        ),
        f(
            "select",
            "Select an option of a <select> by value or label.",
            json!({"ref":{"type":"string"},"value":{"type":"string"}}),
            &["ref", "value"],
        ),
        f(
            "hover",
            "Hover an element.",
            json!({"ref":{"type":"string"}}),
            &["ref"],
        ),
        f(
            "scroll",
            "Scroll the page.",
            json!({"direction":{"type":"string","enum":["up","down","left","right"]},"pixels":{"type":"integer"}}),
            &["direction"],
        ),
        f(
            "open",
            "Navigate to a URL or a path of the app (e.g. /settings).",
            json!({"url":{"type":"string"}}),
            &["url"],
        ),
        f("back", "Go back in history.", json!({}), &[]),
        f(
            "wait",
            "Wait for text to appear on the page, or a number of milliseconds.",
            json!({"text":{"type":"string"},"ms":{"type":"integer"}}),
            &[],
        ),
        f(
            "read_console",
            "Console messages and uncaught page errors since the step started.",
            json!({}),
            &[],
        ),
        f(
            "read_network",
            "Network requests since the step started (method, url, status).",
            json!({"filter":{"type":"string","description":"Substring to filter URLs."}}),
            &[],
        ),
        f(
            "eval",
            "Evaluate a read-only JavaScript expression in the page and return its JSON value.",
            json!({"js":{"type":"string"}}),
            &["js"],
        ),
        f(
            "done",
            "Finish the goal with a verdict.",
            json!({"verdict":{"type":"string","enum":["passed","failed","blocked"]},"reason":{"type":"string"}}),
            &["verdict", "reason"],
        ),
    ];
    if !secret_names.is_empty() {
        t.push(f(
            "fill_secret",
            "Fill an input with a secret (e.g. the password) without seeing it.",
            json!({"ref":{"type":"string"},"name":{"type":"string","enum":secret_names}}),
            &["ref", "name"],
        ));
    }
    Value::Array(t)
}

/// Shrinks old tool results (page snapshots) so the transcript does not grow with every action.
/// The last `keep` tool results stay intact.
fn compact_history(msgs: &mut [Value], keep: usize) {
    let tool_idx: Vec<usize> = msgs
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"] == "tool")
        .map(|(i, _)| i)
        .collect();
    if tool_idx.len() <= keep {
        return;
    }
    for &i in &tool_idx[..tool_idx.len() - keep] {
        if let Some(c) = msgs[i]["content"].as_str() {
            if c.chars().count() > 600 {
                let head: String = c.chars().take(300).collect();
                msgs[i]["content"] = json!(format!(
                    "{head}\n…[older output elided; call `snapshot` for the current page]"
                ));
            }
        }
    }
}

fn norm_ref(r: &str) -> String {
    let r = r.trim().trim_start_matches('@').trim_start_matches("ref=");
    format!("@{r}")
}

fn context_text(ctx: &Context) -> String {
    let mut s = format!("Project: {} (base URL {}).", ctx.project, ctx.base_url);
    if let Some(d) = &ctx.device {
        s.push_str(&format!(" Viewport: {d}."));
        s.push_str(if d.ends_with(", touch") {
            " It is a phone or tablet layout: scroll instead of hovering, open hamburger or menu \
             buttons to reach what is not visible, and never rely on hover-only controls."
        } else {
            " The window is smaller than a desktop one: scroll to see what is below the fold \
             before concluding that something is missing."
        });
    }
    if let Some(l) = ctx.locale {
        s.push_str(&format!(" The UI language is {l}."));
    }
    if let Some(u) = ctx.username {
        s.push_str(&format!(" Signed-in / login username: {u}."));
    }
    if !ctx.secret_names.is_empty() {
        s.push_str(&format!(
            " Secrets available via fill_secret: {}.",
            ctx.secret_names.join(", ")
        ));
    }
    if !ctx.history.is_empty() {
        s.push_str("\nEarlier steps in this test (the browser is still in the state they left):\n");
        for h in ctx.history {
            s.push_str(&format!("- {h}\n"));
        }
    }
    s
}

pub fn signals_summary(s: &Signals) -> String {
    let mut out = String::new();
    if s.console_errors.is_empty() && s.page_errors.is_empty() {
        out.push_str("Console errors: none. ");
    } else {
        for e in s.console_errors.iter().take(8) {
            out.push_str(&format!("console.error: {}\n", truncate(e, 200)));
        }
        for e in s.page_errors.iter().take(8) {
            out.push_str(&format!("page error: {}\n", truncate(e, 200)));
        }
    }
    let failed: Vec<_> = s
        .requests
        .iter()
        .filter(|r| r.status.is_some_and(|c| c >= 400))
        .collect();
    out.push_str(&format!(
        "Network: {} requests, {} failed.",
        s.requests.len(),
        failed.len()
    ));
    for r in failed.iter().take(8) {
        out.push_str(&format!(
            "\n  {} {} -> {}",
            r.method,
            truncate(&r.url, 150),
            r.status.unwrap_or(0)
        ));
    }
    out
}

/// Drives the browser until the goal is reached or the turn budget is spent.
#[allow(clippy::too_many_arguments)]
pub fn run_goal(
    llm: &mut Llm,
    model: &str,
    max_turns: usize,
    browser: &mut Browser,
    ctx: &Context,
    goal: &str,
    secret: &mut dyn FnMut(&str) -> Result<String>,
    resolve_url: &dyn Fn(&str) -> String,
) -> Result<Outcome> {
    let snap = browser.snapshot(false)?;
    let url = browser.url().unwrap_or_default();
    let tools = tools(&ctx.secret_names);
    let mut msgs = vec![
        json!({"role":"system","content":SYSTEM}),
        json!({"role":"user","content":format!(
            "{}\n\nGOAL: {goal}\n\nCurrent URL: {url}\nCurrent page:\n{}",
            context_text(ctx), truncate(&snap, 30000))}),
    ];
    let mut actions = Vec::new();
    let mut recorded: Vec<Recorded> = Vec::new();
    let mut last_calls: Vec<String> = Vec::new();
    // Snapshot the refs in `actions` were read from, so they can be turned into semantic
    // locators (agent-browser refs are ephemeral and mean nothing in another run).
    let mut known = snap.clone();
    for turn in 0..max_turns {
        compact_history(&mut msgs, 2);
        let reply = llm.chat(model, &msgs, Some(&tools))?;
        msgs.push(reply.message.clone());
        if reply.tool_calls.is_empty() {
            msgs.push(json!({"role":"user","content":"Use the tools. Call `done` when finished."}));
            continue;
        }
        for tc in &reply.tool_calls {
            let sig = format!("{} {}", tc.name, tc.args);
            if tc.name == "done" {
                let v = match tc.args["verdict"].as_str() {
                    Some("passed") => Verdict::Passed,
                    Some("blocked") => Verdict::Blocked,
                    _ => Verdict::Failed,
                };
                return Ok(Outcome {
                    verdict: v,
                    reason: tc.args["reason"].as_str().unwrap_or_default().to_string(),
                    actions,
                    recorded,
                });
            }
            // The locator is read from the page as the agent sees it, before the action runs.
            let loc = if let Some(r) = tc.args["ref"].as_str() {
                let id = norm_ref(r);
                if !known.contains(&id) {
                    known = browser.snapshot(false).unwrap_or_default();
                }
                cache::locator_of(&known, &id)
            } else {
                None
            };
            let rec = recordable(&tc.name, &tc.args, loc);
            let result = exec_tool(browser, &tc.name, &tc.args, secret, resolve_url);
            // Navigation invalidates every ref: the next one is read from a fresh snapshot.
            if matches!(tc.name.as_str(), "open" | "back") {
                known.clear();
            }
            let text = match &result {
                Ok(t) => t.clone(),
                Err(e) => format!("ERROR: {e}"),
            };
            // Only successful actions are recorded: a recording must replay the same way.
            if result.is_ok() {
                if let Some(r) = rec {
                    recorded.push(r);
                }
            }
            let shown = if tc.name == "fill_secret" {
                format!("fill_secret {} {}", tc.args["ref"], tc.args["name"])
            } else {
                browser.redact(&sig)
            };
            actions.push(format!(
                "{}{}",
                truncate(&shown, 160),
                if result.is_err() { " (error)" } else { "" }
            ));
            msgs.push(json!({"role":"tool","tool_call_id":tc.id,"content":browser.redact(&truncate(&text, 30000))}));
            last_calls.push(sig);
        }
        // Loop guard: the same call three times in a row.
        if last_calls.len() >= 3
            && last_calls[last_calls.len() - 3..]
                .windows(2)
                .all(|w| w[0] == w[1])
        {
            msgs.push(json!({"role":"user","content":"You repeated the same action three times. Change approach, or call `done`."}));
            last_calls.clear();
        }
        if turn + 2 == max_turns {
            msgs.push(json!({"role":"user","content":"Turn budget almost exhausted: call `done` now with your verdict."}));
        }
    }
    Ok(Outcome {
        verdict: Verdict::Blocked,
        reason: format!("STEP_BUDGET_EXHAUSTED after {max_turns} model turns"),
        actions,
        recorded,
    })
}

/// The cacheable form of a tool call, or `None` when it changes nothing in the browser (reading the
/// page, judging) or when its target could not be located semantically.
fn recordable(name: &str, a: &Value, loc: Option<cache::Locator>) -> Option<Recorded> {
    let s = |k: &str| a[k].as_str().unwrap_or_default().to_string();
    let need = |l: Option<cache::Locator>| l;
    Some(match name {
        "click" => Recorded::Click {
            locator: need(loc)?,
        },
        "hover" => Recorded::Hover {
            locator: need(loc)?,
        },
        "fill" => Recorded::Fill {
            locator: need(loc)?,
            text: s("text"),
        },
        "type_text" => Recorded::TypeText {
            locator: loc,
            text: s("text"),
        },
        "select" => Recorded::Select {
            locator: need(loc)?,
            value: s("value"),
        },
        "fill_secret" => Recorded::FillSecret {
            locator: need(loc)?,
            name: s("name"),
        },
        "press" => Recorded::Press { key: s("key") },
        "scroll" => Recorded::Scroll {
            direction: s("direction"),
            pixels: a["pixels"].as_i64().unwrap_or(600),
        },
        "open" => Recorded::Open { url: s("url") },
        "back" => Recorded::Back,
        "wait" => Recorded::Wait {
            text: a["text"].as_str().map(str::to_string),
            ms: a["ms"].as_u64(),
        },
        _ => return None,
    })
}

fn exec_tool(
    b: &mut Browser,
    name: &str,
    a: &Value,
    secret: &mut dyn FnMut(&str) -> Result<String>,
    resolve_url: &dyn Fn(&str) -> String,
) -> Result<String> {
    let s = |k: &str| a[k].as_str().unwrap_or_default().to_string();
    let after = |b: &mut Browser| -> Result<String> {
        b.wait_load();
        let url = b.url().unwrap_or_default();
        Ok(format!("OK. URL: {url}\nPage now:\n{}", b.snapshot(false)?))
    };
    match name {
        "snapshot" => {
            let full = a["full"].as_bool().unwrap_or(true);
            Ok(format!(
                "URL: {}\n{}",
                b.url().unwrap_or_default(),
                b.snapshot(!full)?
            ))
        }
        "click" => {
            // agent-browser 0.27 clicks at viewport coordinates without scrolling first, so an
            // off-screen element is missed silently: bring it into view before acting.
            let _ = b.run(&["scrollintoview", &norm_ref(&s("ref"))]);
            b.run(&["click", &norm_ref(&s("ref"))])?;
            after(b)
        }
        "fill" => {
            b.run(&["fill", &norm_ref(&s("ref")), &s("text")])?;
            Ok("OK".into())
        }
        "type_text" => {
            if a["ref"].is_string() {
                b.run(&["type", &norm_ref(&s("ref")), &s("text")])?;
            } else {
                b.run(&["keyboard", "type", &s("text")])?;
            }
            Ok("OK".into())
        }
        "press" => {
            b.run(&["press", &s("key")])?;
            after(b)
        }
        "select" => {
            b.run(&["select", &norm_ref(&s("ref")), &s("value")])?;
            after(b)
        }
        "hover" => {
            let _ = b.run(&["scrollintoview", &norm_ref(&s("ref"))]);
            b.run(&["hover", &norm_ref(&s("ref"))])?;
            after(b)
        }
        "scroll" => {
            let px = a["pixels"].as_i64().unwrap_or(600).to_string();
            b.run(&["scroll", &s("direction"), &px])?;
            Ok(format!("OK\n{}", b.snapshot(false)?))
        }
        "open" => {
            b.open(&resolve_url(&s("url")))?;
            after(b)
        }
        "back" => {
            b.run(&["back"])?;
            after(b)
        }
        "wait" => {
            if a["text"].is_string() {
                b.run(&["wait", "--text", &s("text")])?;
            } else {
                let ms = a["ms"]
                    .as_i64()
                    .unwrap_or(1000)
                    .clamp(0, 10_000)
                    .to_string();
                b.run(&["wait", &ms])?;
            }
            Ok("OK".into())
        }
        "read_console" => {
            let sig = b.signals()?;
            Ok(format!(
                "console errors since the step started: {:?}\nuncaught page errors: {:?}",
                sig.console_errors
                    .iter()
                    .map(|x| truncate(x, 300))
                    .collect::<Vec<_>>(),
                sig.page_errors
                    .iter()
                    .map(|x| truncate(x, 300))
                    .collect::<Vec<_>>()
            ))
        }
        "read_network" => {
            let sig = b.signals()?;
            let f = s("filter");
            let lines: Vec<String> = sig
                .requests
                .iter()
                .filter(|r| f.is_empty() || r.url.contains(&f))
                .take(60)
                .map(|r| {
                    format!(
                        "{} {} -> {} ({})",
                        r.method,
                        truncate(&r.url, 160),
                        r.status.map(|x| x.to_string()).unwrap_or("pending".into()),
                        r.resource_type
                    )
                })
                .collect();
            Ok(if lines.is_empty() {
                "no requests".into()
            } else {
                lines.join("\n")
            })
        }
        "eval" => Ok(truncate(&b.eval(&s("js"))?.to_string(), 6000)),
        "fill_secret" => {
            let v = secret(&s("name"))?;
            b.add_redaction(&v);
            b.batch_stdin(&[vec!["fill".into(), norm_ref(&s("ref")), v]])?;
            Ok("OK (secret filled)".into())
        }
        other => anyhow::bail!("unknown tool `{other}`"),
    }
}

/// Judges a natural-language expectation against the current screen and the step signals.
pub fn judge(
    llm: &mut Llm,
    model: &str,
    browser: &mut Browser,
    ctx: &Context,
    expectation: &str,
    signals: &Signals,
) -> Result<Outcome> {
    let snap = browser.snapshot(false)?;
    let text = browser.text().unwrap_or_default();
    let url = browser.url().unwrap_or_default();
    let title = browser.title().unwrap_or_default();
    let prompt = format!(
        "{}\n\nDecide whether this expectation holds RIGHT NOW:\nEXPECTATION: {expectation}\n\n\
         URL: {url}\nTitle: {title}\nTechnical signals during this step:\n{}\n\nVisible text of the page:\n{}\n\nPage (accessibility snapshot):\n{}\n\n\
         Answer ONLY with JSON: {{\"verdict\":\"passed\"|\"failed\"|\"inconclusive\",\"evidence\":\"quote the text/element/signal that proves it\"}}. \
         Interpret the expectation the way an experienced QA engineer would: judge its intent, not exact wording \
         (e.g. a chat input is a composer; text in another language that means the same counts). \
         Use passed only with concrete evidence; failed when the evidence contradicts it.",
        context_text(ctx),
        signals_summary(signals),
        truncate(&text, 12000),
        truncate(&snap, 40000)
    );
    let msgs = vec![
        json!({"role":"system","content":"You are a strict QA judge. You only answer with a JSON object."}),
        json!({"role":"user","content":browser.redact(&prompt)}),
    ];
    for _ in 0..2 {
        let r = llm.chat(model, &msgs, None)?;
        if let Some(v) = extract_json(&r.content) {
            let verdict = match v["verdict"].as_str() {
                Some("passed") => Verdict::Passed,
                Some("failed") => Verdict::Failed,
                _ => Verdict::Failed,
            };
            let mut reason = v["evidence"].as_str().unwrap_or_default().to_string();
            if v["verdict"].as_str() == Some("inconclusive") {
                reason = format!("ASSERTION_INCONCLUSIVE: {reason}");
            }
            return Ok(Outcome {
                verdict,
                reason,
                actions: vec![],
                recorded: vec![],
            });
        }
    }
    Ok(Outcome {
        verdict: Verdict::Blocked,
        reason: "MODEL_OUTPUT_INVALID: judge did not answer JSON".into(),
        actions: vec![],
        recorded: vec![],
    })
}

/// Extracts a value described in natural language from the screen.
pub fn extract(
    llm: &mut Llm,
    model: &str,
    browser: &mut Browser,
    description: &str,
) -> Result<Option<String>> {
    let snap = browser.snapshot(false)?;
    let prompt = format!(
        "From this page, extract: {description}\n\nPage:\n{}\n\nAnswer ONLY with JSON: {{\"found\":true|false,\"value\":\"...\"}}",
        truncate(&snap, 40000)
    );
    let msgs = vec![
        json!({"role":"system","content":"You extract data from web pages. Answer only JSON."}),
        json!({"role":"user","content":browser.redact(&prompt)}),
    ];
    let r = llm.chat(model, &msgs, None)?;
    Ok(extract_json(&r.content).and_then(|v| {
        if v["found"] == json!(true) {
            v["value"]
                .as_str()
                .map(|s| s.to_string())
                .or(Some(v["value"].to_string()))
        } else {
            None
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_and_tools() {
        assert_eq!(norm_ref("e3"), "@e3");
        assert_eq!(norm_ref("@e3"), "@e3");
        assert_eq!(norm_ref("ref=e3"), "@e3");
        let t = tools(&[]);
        assert!(!t
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["function"]["name"] == "fill_secret"));
        let t = tools(&["password".into()]);
        assert!(t
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["function"]["name"] == "fill_secret"));
    }

    #[test]
    fn the_agent_is_told_the_viewport() {
        let ctx = Context {
            project: "app",
            base_url: "http://x",
            locale: None,
            username: None,
            secret_names: vec![],
            device: Some("iPhone 14, 390x844, touch".into()),
            history: &[],
        };
        let t = context_text(&ctx);
        assert!(t.contains("Viewport: iPhone 14, 390x844, touch."), "{t}");
        assert!(
            t.contains("scroll instead of hovering"),
            "the agent is told to behave as on a phone: {t}"
        );
        let narrow = Context {
            device: Some("390x844".into()),
            ..ctx
        };
        let t = context_text(&narrow);
        assert!(t.contains("Viewport: 390x844."), "{t}");
        assert!(
            t.contains("below the fold"),
            "a narrow window is not a phone: {t}"
        );
        let desktop = Context {
            device: None,
            ..narrow
        };
        assert!(!context_text(&desktop).contains("Viewport"));
    }
}
