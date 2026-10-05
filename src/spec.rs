//! Parser for `*.qa.ts` specs.
//!
//! Specs use TypeScript syntax but are never executed: this module tokenizes a small subset
//! (call chains with literal arguments and arrow-function bodies) and maps it to a [`SpecFile`].
//! Anything outside the subset is an error with its line and column.

use anyhow::{bail, Result};
use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Num(f64),
    Punct(char),
    Arrow,
    Eof,
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    line: usize,
    col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

fn lex(src: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line, mut col) = (0usize, 1usize, 1usize);
    macro_rules! adv {
        () => {{
            if chars[i] == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            i += 1;
        }};
    }
    while i < chars.len() {
        let c = chars[i];
        let (l, cl) = (line, col);
        if c.is_whitespace() {
            adv!();
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                adv!();
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            adv!();
            adv!();
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                adv!();
            }
            if i >= chars.len() {
                bail!("{l}:{cl}: unterminated block comment");
            }
            adv!();
            adv!();
        } else if c == '\'' || c == '"' || c == '`' {
            let q = c;
            adv!();
            let mut s = String::new();
            loop {
                if i >= chars.len() {
                    bail!("{l}:{cl}: unterminated string");
                }
                let ch = chars[i];
                if ch == q {
                    adv!();
                    break;
                }
                if ch == '\n' && q != '`' {
                    bail!("{l}:{cl}: unterminated string");
                }
                if ch == '\\' {
                    adv!();
                    if i >= chars.len() {
                        bail!("{l}:{cl}: unterminated string");
                    }
                    let e = chars[i];
                    s.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '0' => '\0',
                        other => other,
                    });
                    adv!();
                    continue;
                }
                s.push(ch);
                adv!();
            }
            out.push(Token {
                tok: Tok::Str(s),
                line: l,
                col: cl,
            });
        } else if c.is_ascii_digit()
            || (c == '-' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let mut s = String::new();
            s.push(c);
            adv!();
            while i < chars.len()
                && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '_')
            {
                if chars[i] != '_' {
                    s.push(chars[i]);
                }
                adv!();
            }
            let n: f64 = s
                .parse()
                .map_err(|_| anyhow::anyhow!("{l}:{cl}: invalid number `{s}`"))?;
            out.push(Token {
                tok: Tok::Num(n),
                line: l,
                col: cl,
            });
        } else if c.is_alphabetic() || c == '_' || c == '$' {
            let mut s = String::new();
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                s.push(chars[i]);
                adv!();
            }
            out.push(Token {
                tok: Tok::Ident(s),
                line: l,
                col: cl,
            });
        } else if c == '=' && chars.get(i + 1) == Some(&'>') {
            adv!();
            adv!();
            out.push(Token {
                tok: Tok::Arrow,
                line: l,
                col: cl,
            });
        } else if "(){}[],.:;*=".contains(c) {
            adv!();
            out.push(Token {
                tok: Tok::Punct(c),
                line: l,
                col: cl,
            });
        } else {
            bail!("{l}:{cl}: unexpected character `{c}` (qaspec specs only allow DSL calls with literal arguments)");
        }
    }
    out.push(Token {
        tok: Tok::Eof,
        line,
        col,
    });
    Ok(out)
}

/// A literal value in a spec.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
    Obj(Vec<(String, Value)>),
    Arr(Vec<Value>),
    Func(Vec<Stmt>),
}

impl Value {
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Value::Str(s) => serde_json::Value::String(s.clone()),
            Value::Num(n) => serde_json::json!(n),
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Null | Value::Func(_) => serde_json::Value::Null,
            Value::Arr(a) => serde_json::Value::Array(a.iter().map(|v| v.to_json()).collect()),
            Value::Obj(o) => {
                serde_json::Value::Object(o.iter().map(|(k, v)| (k.clone(), v.to_json())).collect())
            }
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Value::Str(_) => "string",
            Value::Num(_) => "number",
            Value::Bool(_) => "boolean",
            Value::Null => "null",
            Value::Obj(_) => "object",
            Value::Arr(_) => "array",
            Value::Func(_) => "function",
        }
    }
}

/// One segment of a call chain: `a.b.c(args)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Seg {
    pub path: Vec<String>,
    pub args: Vec<Value>,
}

/// A statement: a call chain such as `expect.network('GET /x').status(200)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub segs: Vec<Seg>,
    pub pos: Pos,
}

const KEYWORDS: &[&str] = &[
    "if", "else", "for", "while", "do", "const", "let", "var", "function", "return", "await",
    "async", "try", "catch", "switch", "class", "new", "throw", "export",
];

struct Parser {
    toks: Vec<Token>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.toks[self.i]
    }
    fn next(&mut self) -> Token {
        let t = self.toks[self.i].clone();
        if self.i < self.toks.len() - 1 {
            self.i += 1;
        }
        t
    }
    fn pos(&self) -> Pos {
        let t = self.peek();
        Pos {
            line: t.line,
            col: t.col,
        }
    }
    fn is_punct(&self, c: char) -> bool {
        self.peek().tok == Tok::Punct(c)
    }
    fn expect_punct(&mut self, c: char) -> Result<()> {
        if self.is_punct(c) {
            self.next();
            Ok(())
        } else {
            bail!(
                "{}: expected `{c}`, found {}",
                self.pos(),
                describe(&self.peek().tok)
            )
        }
    }
    fn ident(&mut self) -> Result<String> {
        match self.next() {
            Token {
                tok: Tok::Ident(s), ..
            } => Ok(s),
            t => bail!(
                "{}:{}: expected a name, found {}",
                t.line,
                t.col,
                describe(&t.tok)
            ),
        }
    }

    fn program(&mut self) -> Result<Vec<Stmt>> {
        let mut out = Vec::new();
        while self.peek().tok != Tok::Eof {
            if self.peek().tok == Tok::Ident("import".into()) {
                self.skip_import()?;
                continue;
            }
            out.push(self.stmt()?);
        }
        Ok(out)
    }

    fn skip_import(&mut self) -> Result<()> {
        // import ... from 'x';   |   import 'x';
        let start = self.pos();
        self.next();
        loop {
            match self.next().tok {
                Tok::Str(_) => break,
                Tok::Eof => bail!("{start}: unterminated import"),
                _ => {}
            }
        }
        if self.is_punct(';') {
            self.next();
        }
        Ok(())
    }

    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect_punct('{')?;
        let mut out = Vec::new();
        while !self.is_punct('}') {
            if self.peek().tok == Tok::Eof {
                bail!("{}: unterminated block, expected `}}`", self.pos());
            }
            out.push(self.stmt()?);
        }
        self.next();
        Ok(out)
    }

    fn stmt(&mut self) -> Result<Stmt> {
        let pos = self.pos();
        if let Tok::Ident(k) = &self.peek().tok {
            if KEYWORDS.contains(&k.as_str()) {
                bail!("{pos}: `{k}` is not allowed: specs are declarative (only suite/step/goal/expect/capture calls)");
            }
        }
        let mut segs = Vec::new();
        loop {
            let mut path = vec![self.ident()?];
            while self.is_punct('.') {
                self.next();
                path.push(self.ident()?);
            }
            if !self.is_punct('(') {
                bail!(
                    "{}: `{}` must be called, e.g. `{}(...)`",
                    self.pos(),
                    path.join("."),
                    path.join(".")
                );
            }
            let args = self.args()?;
            segs.push(Seg { path, args });
            if self.is_punct('.') {
                self.next();
                continue;
            }
            break;
        }
        if self.is_punct(';') {
            self.next();
        }
        Ok(Stmt { segs, pos })
    }

    fn args(&mut self) -> Result<Vec<Value>> {
        self.expect_punct('(')?;
        let mut out = Vec::new();
        while !self.is_punct(')') {
            out.push(self.value()?);
            if self.is_punct(',') {
                self.next();
            } else if !self.is_punct(')') {
                bail!(
                    "{}: expected `,` or `)`, found {}",
                    self.pos(),
                    describe(&self.peek().tok)
                );
            }
        }
        self.next();
        Ok(out)
    }

    fn value(&mut self) -> Result<Value> {
        let pos = self.pos();
        match self.peek().tok.clone() {
            Tok::Str(s) => {
                self.next();
                Ok(Value::Str(s))
            }
            Tok::Num(n) => {
                self.next();
                Ok(Value::Num(n))
            }
            Tok::Ident(id) if id == "true" || id == "false" => {
                self.next();
                Ok(Value::Bool(id == "true"))
            }
            Tok::Ident(id) if id == "null" || id == "undefined" => {
                self.next();
                Ok(Value::Null)
            }
            Tok::Ident(id) if id == "async" => {
                bail!("{pos}: specs are declarative, `async` is not allowed")
            }
            Tok::Punct('(') => {
                self.next();
                self.expect_punct(')')?;
                if self.peek().tok != Tok::Arrow {
                    bail!("{}: expected `=>`", self.pos());
                }
                self.next();
                Ok(Value::Func(self.block()?))
            }
            Tok::Punct('{') => {
                self.next();
                let mut fields = Vec::new();
                while !self.is_punct('}') {
                    let key = match self.next() {
                        Token {
                            tok: Tok::Ident(s), ..
                        }
                        | Token {
                            tok: Tok::Str(s), ..
                        } => s,
                        t => bail!(
                            "{}:{}: expected an object key, found {}",
                            t.line,
                            t.col,
                            describe(&t.tok)
                        ),
                    };
                    self.expect_punct(':')?;
                    fields.push((key, self.value()?));
                    if self.is_punct(',') {
                        self.next();
                    } else if !self.is_punct('}') {
                        bail!("{}: expected `,` or `}}`", self.pos());
                    }
                }
                self.next();
                Ok(Value::Obj(fields))
            }
            Tok::Punct('[') => {
                self.next();
                let mut items = Vec::new();
                while !self.is_punct(']') {
                    items.push(self.value()?);
                    if self.is_punct(',') {
                        self.next();
                    } else if !self.is_punct(']') {
                        bail!("{}: expected `,` or `]`", self.pos());
                    }
                }
                self.next();
                Ok(Value::Arr(items))
            }
            other => bail!(
                "{pos}: expected a literal value, found {} (specs only accept literals)",
                describe(&other)
            ),
        }
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => format!("`{s}`"),
        Tok::Str(_) => "a string".into(),
        Tok::Num(n) => format!("`{n}`"),
        Tok::Punct(c) => format!("`{c}`"),
        Tok::Arrow => "`=>`".into(),
        Tok::Eof => "end of file".into(),
    }
}

// ---------------------------------------------------------------------------------------------
// Semantic model
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct SpecFile {
    pub path: String,
    pub suites: Vec<Suite>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Suite {
    pub name: String,
    pub project: Option<String>,
    pub identity: Option<String>,
    pub start: Option<String>,
    /// Device emulation for this suite (`device` or `[width, height, scale]`).
    pub device: Option<String>,
    pub viewport: Option<Viewport>,
    pub steps: Vec<Step>,
    pub pos: Pos,
}

/// A viewport in CSS pixels, with an optional device pixel ratio.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
}

impl fmt::Display for Viewport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.scale {
            Some(s) => write!(f, "{}x{} at {}x", self.width, self.height, s),
            None => write!(f, "{}x{}", self.width, self.height),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OnFail {
    /// Later steps that depend on browser state (no `start`) are skipped.
    Stop,
    /// Later steps run regardless.
    Continue,
    /// Every later step of the suite is skipped.
    Abort,
}

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub name: String,
    pub start: Option<String>,
    pub on_fail: OnFail,
    pub needs: Vec<String>,
    pub project: Option<String>,
    pub identity: Option<String>,
    pub items: Vec<Item>,
    pub pos: Pos,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Item {
    Goal {
        text: String,
        pos: Pos,
    },
    Expect {
        check: Check,
        pos: Pos,
    },
    Capture {
        name: String,
        source: CaptureSource,
        pos: Pos,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Check {
    /// Judged by the LLM over the screen and the step's signals.
    Judge {
        text: String,
    },
    ConsoleNoErrors,
    ErrorsNone,
    /// A matching request was made; with `status`, every match returned it.
    Network {
        pattern: String,
        status: Option<u16>,
        ok: bool,
    },
    NoNetworkFailures,
    Url {
        op: UrlOp,
        value: String,
        negate: bool,
    },
    State {
        js: String,
        expected: Option<serde_json::Value>,
        negate: bool,
    },
    Visible {
        text: String,
        negate: bool,
    },
    StorageLocal {
        key: String,
        expected: Option<serde_json::Value>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UrlOp {
    Contains,
    Equals,
}

impl Check {
    pub fn is_deterministic(&self) -> bool {
        !matches!(self, Check::Judge { .. })
    }

    pub fn label(&self) -> String {
        let not = |n: bool| if n { "not." } else { "" };
        match self {
            Check::Judge { text } => format!("expect('{text}')"),
            Check::ConsoleNoErrors => "expect.console.noErrors()".into(),
            Check::ErrorsNone => "expect.errors.none()".into(),
            Check::Network {
                pattern,
                status: Some(s),
                ..
            } => format!("expect.network('{pattern}').status({s})"),
            Check::Network {
                pattern, ok: true, ..
            } => format!("expect.network('{pattern}').ok()"),
            Check::Network { pattern, .. } => format!("expect.network('{pattern}').called()"),
            Check::NoNetworkFailures => "expect.network.noFailures()".into(),
            Check::Url {
                op: UrlOp::Contains,
                value,
                negate,
            } => format!("expect.url().{}toContain('{value}')", not(*negate)),
            Check::Url {
                op: UrlOp::Equals,
                value,
                negate,
            } => format!("expect.url().{}toBe('{value}')", not(*negate)),
            Check::State {
                js,
                expected: Some(v),
                negate,
            } => format!("expect.state('{js}').{}equals({v})", not(*negate)),
            Check::State { js, negate, .. } => {
                format!("expect.state('{js}').{}toBeTruthy()", not(*negate))
            }
            Check::Visible { text, negate } => format!("expect.{}visible('{text}')", not(*negate)),
            Check::StorageLocal {
                key,
                expected: Some(v),
            } => format!("expect.storage.local('{key}').equals({v})"),
            Check::StorageLocal { key, .. } => format!("expect.storage.local('{key}').exists()"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CaptureSource {
    /// Extracted by the LLM from the screen.
    Describe {
        description: String,
    },
    State {
        js: String,
    },
    Url,
}

fn str_arg(args: &[Value], i: usize, what: &str, pos: Pos) -> Result<String> {
    match args.get(i) {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(v) => bail!("{pos}: {what} must be a string, got {}", v.kind()),
        None => bail!("{pos}: {what} is required"),
    }
}

fn obj_arg(args: &[Value], i: usize, pos: Pos) -> Result<Option<&Vec<(String, Value)>>> {
    match args.get(i) {
        Some(Value::Obj(o)) => Ok(Some(o)),
        Some(Value::Func(_)) | None => Ok(None),
        Some(v) => bail!("{pos}: options must be an object, got {}", v.kind()),
    }
}

fn func_arg(args: &[Value], pos: Pos, what: &str) -> Result<Vec<Stmt>> {
    match args.last() {
        Some(Value::Func(b)) => Ok(b.clone()),
        _ => bail!("{pos}: {what} needs a body: `() => {{ ... }}` as its last argument"),
    }
}

fn opt_str(o: &[(String, Value)], key: &str, pos: Pos) -> Result<Option<String>> {
    match o.iter().find(|(k, _)| k == key) {
        None | Some((_, Value::Null)) => Ok(None),
        Some((_, Value::Str(s))) => Ok(Some(s.clone())),
        Some((_, v)) => bail!("{pos}: option `{key}` must be a string, got {}", v.kind()),
    }
}

/// `viewport: [w, h]` or `viewport: [w, h, scale]`.
fn opt_viewport(o: &[(String, Value)], pos: Pos) -> Result<Option<Viewport>> {
    let Some((_, Value::Arr(a))) = o.iter().find(|(k, _)| k == "viewport") else {
        return match o.iter().find(|(k, _)| k == "viewport") {
            None | Some((_, Value::Null)) => Ok(None),
            Some((_, v)) => bail!(
                "{pos}: option `viewport` must be an array, got {}",
                v.kind()
            ),
        };
    };
    if a.len() < 2 || a.len() > 3 {
        bail!("{pos}: `viewport` needs [width, height] or [width, height, scale]");
    }
    let mut nums = Vec::with_capacity(a.len());
    for x in a {
        match x {
            Value::Num(n) if *n > 0.0 => nums.push(*n),
            Value::Num(n) => bail!("{pos}: `viewport` values must be positive, got {n}"),
            other => bail!(
                "{pos}: `viewport` must be a list of numbers, got {}",
                other.kind()
            ),
        }
    }
    let (w, h) = (nums[0], nums[1]);
    let scale = nums.get(2).copied();
    Ok(Some(Viewport {
        width: w as u32,
        height: h as u32,
        scale,
    }))
}

fn check_keys(o: &[(String, Value)], allowed: &[&str], what: &str, pos: Pos) -> Result<()> {
    for (k, _) in o {
        if !allowed.contains(&k.as_str()) {
            bail!(
                "{pos}: unknown {what} option `{k}` (allowed: {})",
                allowed.join(", ")
            );
        }
    }
    Ok(())
}

/// Parses spec source text.
pub fn parse(path: &str, src: &str) -> Result<SpecFile> {
    let toks = lex(src).map_err(|e| anyhow::anyhow!("{path}:{e}"))?;
    let stmts = Parser { toks, i: 0 }
        .program()
        .map_err(|e| anyhow::anyhow!("{path}:{e}"))?;
    let mut suites = Vec::new();
    for st in stmts {
        let seg = &st.segs[0];
        if seg.path != ["suite"] || st.segs.len() != 1 {
            bail!(
                "{path}:{}: only `suite(...)` is allowed at the top level, found `{}`",
                st.pos,
                seg.path.join(".")
            );
        }
        suites.push(suite(&st).map_err(|e| anyhow::anyhow!("{path}:{e}"))?);
    }
    if suites.is_empty() {
        bail!("{path}: no `suite(...)` found");
    }
    let mut names = std::collections::HashSet::new();
    for s in &suites {
        if !names.insert(&s.name) {
            bail!("{path}:{}: duplicate suite name `{}`", s.pos, s.name);
        }
    }
    Ok(SpecFile {
        path: path.to_string(),
        suites,
    })
}

fn suite(st: &Stmt) -> Result<Suite> {
    let pos = st.pos;
    let args = &st.segs[0].args;
    let name = str_arg(args, 0, "suite name", pos)?;
    let opts = obj_arg(args, 1, pos)?;
    let body = func_arg(args, pos, "suite")?;
    let (mut project, mut identity, mut start) = (None, None, None);
    let (mut device, mut viewport) = (None, None);
    if let Some(o) = opts {
        check_keys(
            o,
            &["project", "as", "start", "device", "viewport"],
            "suite",
            pos,
        )?;
        project = opt_str(o, "project", pos)?;
        identity = opt_str(o, "as", pos)?;
        start = opt_str(o, "start", pos)?;
        device = opt_str(o, "device", pos)?;
        viewport = opt_viewport(o, pos)?;
    }
    let mut steps = Vec::new();
    let mut loose = Vec::new();
    for s in &body {
        if s.segs[0].path == ["step"] {
            if !loose.is_empty() {
                bail!("{}: mix of `step(...)` and loose goal/expect calls in suite `{name}`; put them in a step", s.pos);
            }
            steps.push(step(s)?);
        } else {
            if !steps.is_empty() {
                bail!("{}: mix of `step(...)` and loose goal/expect calls in suite `{name}`; put them in a step", s.pos);
            }
            loose.push(item(s)?);
        }
    }
    if !loose.is_empty() {
        steps.push(Step {
            name: name.clone(),
            start: None,
            on_fail: OnFail::Stop,
            needs: vec![],
            project: None,
            identity: None,
            items: loose,
            pos,
        });
    }
    if steps.is_empty() {
        bail!("{pos}: suite `{name}` is empty");
    }
    let mut seen: Vec<&str> = Vec::new();
    for s in &steps {
        if seen.contains(&s.name.as_str()) {
            bail!("{}: duplicate step name `{}`", s.pos, s.name);
        }
        for n in &s.needs {
            if !seen.contains(&n.as_str()) {
                bail!(
                    "{}: step `{}` needs `{n}`, which is not an earlier step of this suite",
                    s.pos,
                    s.name
                );
            }
        }
        seen.push(&s.name);
    }
    Ok(Suite {
        name,
        project,
        identity,
        start,
        device,
        viewport,
        steps,
        pos,
    })
}

fn step(st: &Stmt) -> Result<Step> {
    let pos = st.pos;
    if st.segs.len() != 1 || st.segs[0].path.len() != 1 {
        bail!("{pos}: invalid step");
    }
    let args = &st.segs[0].args;
    let name = str_arg(args, 0, "step name", pos)?;
    let opts = obj_arg(args, 1, pos)?;
    let body = func_arg(args, pos, "step")?;
    let mut s = Step {
        name,
        start: None,
        on_fail: OnFail::Stop,
        needs: vec![],
        project: None,
        identity: None,
        items: vec![],
        pos,
    };
    if let Some(o) = opts {
        check_keys(
            o,
            &["start", "onFail", "needs", "project", "as"],
            "step",
            pos,
        )?;
        s.start = opt_str(o, "start", pos)?;
        s.project = opt_str(o, "project", pos)?;
        s.identity = opt_str(o, "as", pos)?;
        if let Some(f) = opt_str(o, "onFail", pos)? {
            s.on_fail = match f.as_str() {
                "stop" => OnFail::Stop,
                "continue" => OnFail::Continue,
                "abort" => OnFail::Abort,
                other => {
                    bail!("{pos}: onFail must be 'stop', 'continue' or 'abort', got '{other}'")
                }
            };
        }
        if let Some((_, v)) = o.iter().find(|(k, _)| k == "needs") {
            match v {
                Value::Str(x) => s.needs.push(x.clone()),
                Value::Arr(a) => {
                    for x in a {
                        match x {
                            Value::Str(x) => s.needs.push(x.clone()),
                            _ => bail!("{pos}: needs must list step names"),
                        }
                    }
                }
                _ => bail!("{pos}: needs must be a step name or a list of step names"),
            }
        }
    }
    for b in &body {
        if b.segs[0].path == ["step"] {
            bail!("{}: steps cannot be nested", b.pos);
        }
        s.items.push(item(b)?);
    }
    if s.items.is_empty() {
        bail!("{pos}: step `{}` is empty", s.name);
    }
    Ok(s)
}

fn literal_json(v: &Value, pos: Pos) -> Result<serde_json::Value> {
    if matches!(v, Value::Func(_)) {
        bail!("{pos}: expected a literal value");
    }
    Ok(v.to_json())
}

fn item(st: &Stmt) -> Result<Item> {
    let pos = st.pos;
    let head = &st.segs[0];
    let path: Vec<&str> = head.path.iter().map(|s| s.as_str()).collect();
    let rest: Vec<(Vec<&str>, &Vec<Value>)> = st.segs[1..]
        .iter()
        .map(|s| (s.path.iter().map(|p| p.as_str()).collect(), &s.args))
        .collect();
    let no_rest = |what: &str| -> Result<()> {
        if !rest.is_empty() {
            bail!("{pos}: unexpected `.{}` after {what}", rest[0].0.join("."));
        }
        Ok(())
    };
    let check = |c: Check| Ok(Item::Expect { check: c, pos });
    match path.as_slice() {
        ["goal"] => {
            no_rest("goal()")?;
            Ok(Item::Goal {
                text: str_arg(&head.args, 0, "goal", pos)?,
                pos,
            })
        }
        ["expect"] => {
            no_rest("expect()")?;
            check(Check::Judge {
                text: str_arg(&head.args, 0, "expectation", pos)?,
            })
        }
        ["expect", "console", "noErrors"] => {
            no_rest("expect.console.noErrors()")?;
            check(Check::ConsoleNoErrors)
        }
        ["expect", "errors", "none"] => {
            no_rest("expect.errors.none()")?;
            check(Check::ErrorsNone)
        }
        ["expect", "network", "noFailures"] => {
            no_rest("expect.network.noFailures()")?;
            check(Check::NoNetworkFailures)
        }
        ["expect", "network"] => {
            let pattern = str_arg(&head.args, 0, "network pattern", pos)?;
            match rest.as_slice() {
                [] => check(Check::Network {
                    pattern,
                    status: None,
                    ok: false,
                }),
                [(p, _)] if p.as_slice() == ["called"] => check(Check::Network {
                    pattern,
                    status: None,
                    ok: false,
                }),
                [(p, _)] if p.as_slice() == ["ok"] => check(Check::Network {
                    pattern,
                    status: None,
                    ok: true,
                }),
                [(p, a)] if p.as_slice() == ["status"] => match a.first() {
                    Some(Value::Num(n)) if *n >= 100.0 && *n < 600.0 => check(Check::Network {
                        pattern,
                        status: Some(*n as u16),
                        ok: false,
                    }),
                    _ => bail!("{pos}: status() needs an HTTP status code"),
                },
                _ => {
                    bail!("{pos}: after expect.network(...) use .status(code), .ok() or .called()")
                }
            }
        }
        ["expect", "url"] => {
            let (negate, op, args) = match rest.as_slice() {
                [(p, a)] if p.as_slice() == ["toContain"] => (false, UrlOp::Contains, a),
                [(p, a)] if p.as_slice() == ["not", "toContain"] => (true, UrlOp::Contains, a),
                [(p, a)] if p.as_slice() == ["toBe"] => (false, UrlOp::Equals, a),
                [(p, a)] if p.as_slice() == ["not", "toBe"] => (true, UrlOp::Equals, a),
                _ => bail!("{pos}: after expect.url() use .toContain(s), .not.toContain(s), .toBe(s) or .not.toBe(s)"),
            };
            check(Check::Url {
                op,
                value: str_arg(args, 0, "url value", pos)?,
                negate,
            })
        }
        ["expect", "state"] => {
            let js = str_arg(&head.args, 0, "state expression", pos)?;
            match rest.as_slice() {
                [(p, a)] if p.as_slice() == ["equals"] || p.as_slice() == ["not", "equals"] => {
                    let v = a.first().ok_or_else(|| anyhow::anyhow!("{pos}: equals() needs a value"))?;
                    check(Check::State { js, expected: Some(literal_json(v, pos)?), negate: p[0] == "not" })
                }
                [(p, _)] if p.as_slice() == ["toBeTruthy"] => check(Check::State { js, expected: None, negate: false }),
                [(p, _)] if p.as_slice() == ["toBeFalsy"] => check(Check::State { js, expected: None, negate: true }),
                _ => bail!("{pos}: after expect.state(js) use .equals(v), .not.equals(v), .toBeTruthy() or .toBeFalsy()"),
            }
        }
        ["expect", "visible"] | ["expect", "not", "visible"] => {
            no_rest("expect.visible()")?;
            check(Check::Visible {
                text: str_arg(&head.args, 0, "visible text", pos)?,
                negate: path.len() == 3,
            })
        }
        ["expect", "storage", "local"] => {
            let key = str_arg(&head.args, 0, "storage key", pos)?;
            match rest.as_slice() {
                [(p, a)] if p.as_slice() == ["equals"] => {
                    let v = a
                        .first()
                        .ok_or_else(|| anyhow::anyhow!("{pos}: equals() needs a value"))?;
                    check(Check::StorageLocal {
                        key,
                        expected: Some(literal_json(v, pos)?),
                    })
                }
                [(p, _)] if p.as_slice() == ["exists"] => check(Check::StorageLocal {
                    key,
                    expected: None,
                }),
                _ => bail!("{pos}: after expect.storage.local(key) use .equals(v) or .exists()"),
            }
        }
        ["capture"] => {
            no_rest("capture()")?;
            let name = capture_name(&head.args, pos)?;
            Ok(Item::Capture {
                name,
                source: CaptureSource::Describe {
                    description: str_arg(&head.args, 1, "capture description", pos)?,
                },
                pos,
            })
        }
        ["capture", "state"] => {
            no_rest("capture.state()")?;
            let name = capture_name(&head.args, pos)?;
            Ok(Item::Capture {
                name,
                source: CaptureSource::State {
                    js: str_arg(&head.args, 1, "state expression", pos)?,
                },
                pos,
            })
        }
        ["capture", "url"] => {
            no_rest("capture.url()")?;
            Ok(Item::Capture {
                name: capture_name(&head.args, pos)?,
                source: CaptureSource::Url,
                pos,
            })
        }
        _ => bail!("{pos}: unknown call `{}`", head.path.join(".")),
    }
}

fn capture_name(args: &[Value], pos: Pos) -> Result<String> {
    let n = str_arg(args, 0, "capture name", pos)?;
    if n.is_empty() || !n.chars().all(|c| c.is_alphanumeric() || c == '_') {
        bail!("{pos}: capture name `{n}` must be letters, digits or `_`");
    }
    if ["params", "project", "projects", "identity", "run", "env"].contains(&n.as_str()) {
        bail!("{pos}: capture name `{n}` is reserved");
    }
    Ok(n)
}

impl Suite {
    pub fn has_llm_work(&self) -> bool {
        self.steps.iter().any(|s| {
            s.items.iter().any(|i| {
                matches!(
                    i,
                    Item::Goal { .. }
                        | Item::Expect {
                            check: Check::Judge { .. },
                            ..
                        }
                ) || matches!(
                    i,
                    Item::Capture {
                        source: CaptureSource::Describe { .. },
                        ..
                    }
                )
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
import { suite, step, goal, expect, capture } from 'qaspec';

// a comment
suite('Checkout', { project: 'shop', as: 'buyer', start: '/' }, () => {
  step('add to cart', () => {
    goal('add the "Blue mug" to the cart');
    expect('the cart badge shows 1');
    expect.console.noErrors();
    expect.network('POST /api/cart/**').status(201);
    capture('total', 'the cart total');
  });
  /* block */
  step("pay", { start: `/checkout`, onFail: 'continue', needs: ['add to cart'] }, () => {
    expect.url().not.toContain('/login');
    expect.state('window.cart.items.length').equals(1);
    expect.storage.local('cart').exists();
    expect.not.visible('Error');
    capture.state('orderId', 'window.lastOrder.id');
  });
});
"#;

    #[test]
    fn parses_device_and_viewport_options() {
        let f = parse(
            "m.qa.ts",
            "suite('m', { device: 'iPhone 14' }, () => { expect.url().toBe('/'); })\n\
             suite('v', { viewport: [390, 844, 3] }, () => { expect.url().toBe('/'); })\n\
             suite('both', { device: 'Pixel 7', viewport: [360, 640] }, () => { expect.url().toBe('/'); })\n\
             suite('none', {}, () => { expect.url().toBe('/'); })",
        )
        .unwrap();
        assert_eq!(f.suites[0].device.as_deref(), Some("iPhone 14"));
        assert_eq!(f.suites[0].viewport, None);
        let v = f.suites[1].viewport.unwrap();
        assert_eq!((v.width, v.height, v.scale), (390, 844, Some(3.0)));
        assert_eq!(v.to_string(), "390x844 at 3x");
        let v = f.suites[2].viewport.unwrap();
        assert_eq!((v.width, v.height, v.scale), (360, 640, None));
        assert_eq!(f.suites[2].device.as_deref(), Some("Pixel 7"));
        assert_eq!(f.suites[3].device, None);
        assert_eq!(f.suites[3].viewport, None);
    }

    #[test]
    fn device_and_viewport_errors() {
        let err = |src: &str| parse("e.qa.ts", src).unwrap_err().to_string();
        let one = |opts: &str| format!("suite('x', {opts}, () => {{ expect.url().toBe('/'); }})");
        assert!(
            err(&one("{ viewport: [] }")).contains("e.qa.ts:1:1: `viewport` needs [width, height]")
        );
        assert!(err(&one("{ viewport: 390 }"))
            .contains("e.qa.ts:1:1: option `viewport` must be an array"));
        assert!(err(&one("{ device: 7 }")).contains("must be a string"));
        assert!(err(&one("{ viewport: [390] }")).contains("`viewport` needs"));
        assert!(err(&one("{ viewport: [390, 844, 3, 1] }")).contains("`viewport` needs"));
        assert!(err(&one("{ viewport: [0, 844] }")).contains("must be positive"));
        assert!(err(&one("{ viewport: [390, 844, 0] }")).contains("must be positive"));
        assert!(err(&one("{ viewport: 390 }")).contains("must be an array"));
        assert!(err(&one("{ device: 7 }")).contains("must be a string"));
        assert!(err(&one("{ devce: 'x' }")).contains("unknown suite option `devce`"));
    }

    #[test]
    fn parses_sample() {
        let f = parse("a.qa.ts", SAMPLE).unwrap();
        assert_eq!(f.suites.len(), 1);
        let s = &f.suites[0];
        assert_eq!(s.project.as_deref(), Some("shop"));
        assert_eq!(s.identity.as_deref(), Some("buyer"));
        assert_eq!(s.steps.len(), 2);
        assert_eq!(s.steps[0].items.len(), 5);
        assert_eq!(s.steps[1].on_fail, OnFail::Continue);
        assert_eq!(s.steps[1].needs, vec!["add to cart"]);
        assert_eq!(s.steps[1].start.as_deref(), Some("/checkout"));
        match &s.steps[0].items[3] {
            Item::Expect {
                check: Check::Network {
                    pattern, status, ..
                },
                ..
            } => {
                assert_eq!(pattern, "POST /api/cart/**");
                assert_eq!(*status, Some(201));
            }
            other => panic!("{other:?}"),
        }
        match &s.steps[1].items[1] {
            Item::Expect {
                check: Check::State { expected, .. },
                ..
            } => assert_eq!(expected, &Some(serde_json::json!(1.0))),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            &s.steps[1].items[3],
            Item::Expect {
                check: Check::Visible { negate: true, .. },
                ..
            }
        ));
        assert!(s.has_llm_work());
    }

    #[test]
    fn loose_items_become_one_step() {
        let f = parse("b.qa.ts", "suite('x', () => { expect.errors.none(); })").unwrap();
        assert_eq!(f.suites[0].steps.len(), 1);
        assert_eq!(f.suites[0].steps[0].name, "x");
        assert!(!f.suites[0].has_llm_work());
    }

    fn err(src: &str) -> String {
        parse("e.qa.ts", src).unwrap_err().to_string()
    }

    #[test]
    fn errors_have_positions() {
        assert!(
            err("suite('x', () => { if (a) {} })").contains("e.qa.ts:1:20: `if` is not allowed")
        );
        assert!(err("suite('x', () => {\n  const a = 1\n})").contains("e.qa.ts:2:3: `const`"));
        assert!(err("suite('x', () => { goal(foo) })").contains("expected a literal"));
        assert!(
            err("suite('x', () => {\n  expect.network('x').status('a')\n})")
                .contains("e.qa.ts:2:3")
        );
        assert!(
            err("suite('x', () => { step('a', () => { goal('g') }); goal('loose') })")
                .contains("mix of")
        );
        assert!(
            err("suite('x', () => { step('a', {needs:['b']}, () => { goal('g') }) })")
                .contains("not an earlier step")
        );
        assert!(
            err("suite('x', { bogus: 1 }, () => { goal('g') })").contains("unknown suite option")
        );
        assert!(err("goal('x')").contains("only `suite(...)`"));
        assert!(err("suite('x', () => { capture('params', 'x') })").contains("reserved"));
        assert!(err("suite('x', () => { goal('a) })").contains("unterminated string"));
        assert!(err("suite('x', async () => { goal('a') })").contains("async"));
        assert!(err("").contains("no `suite"));
    }
}
