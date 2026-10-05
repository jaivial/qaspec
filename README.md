# qaspec

**Agentic end-to-end specs.** Write *what* a QA engineer would check — goals and expectations —
and an LLM agent drives a real browser through [agent-browser](https://github.com/vercel-labs/agent-browser)
to do it, checking the screen **and** the console, uncaught errors, network and app state.

```ts
// specs/todos.qa.ts — TypeScript syntax, but it is never executed: qaspec reads it as a declaration.
import { suite, step, goal, expect, capture } from 'qaspec';

suite('todos journey', { project: 'app', as: 'qa', start: '/' }, () => {
  step('go to the list', () => {
    goal('open the Todos page from the navigation');
    expect('the Todos page lists at least one todo');        // judged by the LLM, with evidence
    expect.url().toContain('/todos');                        // deterministic, no model call
  });

  step('add a todo', () => {                                 // continues where the last step left off
    goal('add a todo with the text "${params.todo} ${run.id}"');
    expect('the list now contains "${params.todo} ${run.id}"');
    expect.network('POST /api/todos').status(201);
    expect.storage.local('lastTodo').equals('${params.todo} ${run.id}');
    expect.console.noErrors();                               // only errors raised during THIS step
    capture('first', 'the text of the first todo');          // reusable later as ${first}
  });

  step('home shows the count', { start: '/' }, () => {       // `start` makes a step independent
    expect('the welcome page says the user has more than 1 todo');
  });
});
```

<p align="center">
  <a href="https://jaivial.github.io/qaspec/"><img src="assets/screenshots/run.png" alt="qaspec run: an agent-driven journey passes, a page with a console error and a 500 fails, all in one browser session" width="820"></a>
</p>

**Website:** https://jaivial.github.io/qaspec/ (it includes an interactive viewer for the report of this run).

## Why

Classic E2E tests are slow to write (locators, waits, helpers) and blind to what they don't assert.
A human QA reasons about what they see, but can't read the console or the network while clicking.
Agentic frameworks such as `e2e` keep the model away from those signals by design.
qaspec sits in between: **an agent that clicks like a person and inspects like a test.**

- **One file, many checks in order.** A `suite` is a journey of `step`s that share the browser state.
  A failed step skips only the steps that depend on its state.
- **One Chromium for the whole run.** A Chromium session costs about 1.5 GB of memory, so qaspec never
  runs browsers in parallel. Each project gets its own tab, and the session is reused across suites.
- **Sign in once.** Identities sign in once (with a login spec, or the agent does it by itself). The
  state is saved under `.qaspec/state/` with mode 600 and reused across runs until it expires.
- **Config, not code.** Base URLs per environment, credentials as secrets, params, and projects that
  depend on each other all live in `qaspec.toml`.
- **Secrets stay secret.** The model only sees secret *names*. Values are typed through
  agent-browser's stdin, never argv, and redacted from every log and report.
- **Single Rust binary.** agent-browser is the only runtime dependency.

## What it looks like

These screenshots come from a real run against the demo app in [`tests/fixtures/app`](tests/fixtures/app).

| 1. Sign in once | 2. Reach the goal | 3. Catch what looks fine |
|---|---|---|
| <img src="assets/screenshots/app-login-annotated.png" alt="Sign-in page with agent-browser's numbered labels on each control" width="300"> | <img src="assets/screenshots/app-todo-added.png" alt="Todo list with the item the agent added" width="300"> | <img src="assets/screenshots/app-broken.png" alt="Reports page that renders but throws and gets a 500" width="300"> |
| agent-browser labels every control. The agent types the username and uses `fill_secret` for the password, so it never sees it. The session is saved for the next run. | For `goal('add a todo …')` the agent fills the input and presses Add. qaspec checks that `POST /api/todos` returned 201 and that no console error fired during the step. | The page looks fine, but its script throws and its API returns 500. `expect.console.noErrors()` fails the step. |

The report of that run, rendered on the website, with the failing step expanded:

<img src="assets/screenshots/report.png" alt="Report with per-step verdicts, signal counts, and the failing step showing a console error and a 500 response as evidence" width="760">

`qaspec check` validates specs, config, identities and secrets without opening a browser:

<img src="assets/screenshots/check.png" alt="qaspec check output listing suites by project and identity" width="760">

## How it compares

Regular E2E is a scripted Playwright or Cypress suite. Agentic E2E is [e2e by TesterArmy](https://github.com/tester-army/e2e).

| | qaspec | Regular E2E | Agentic E2E (TesterArmy) |
|---|:---:|:---:|:---:|
| Tests written without selectors | ✅ | ❌ | ✅ (mixed with locators) |
| Survives UI changes without edits | ✅ | ❌ | ✅ |
| Judges the screen like a person | ✅ | ❌ | ✅ |
| The model sees console errors | ✅ | ❌ | ❌ |
| The model sees network requests | ✅ | ❌ | ❌ |
| Console and network checks built in | ✅ | ❌ | ❌ |
| Ordered checks sharing browser state | ✅ | ✅ | ✅ |
| Sign-in reused across runs | ✅ | ✅ | ❌ (once per run) |
| One browser for the whole run | ✅ | ❌ | ❌ |
| Dependent apps: health, order, shared data | ✅ | ❌ | ❌ |
| Passwords never reach the model | ✅ | ✅ (no model) | ✅ |
| Runs without calling a model | ❌ (exact checks only) | ✅ | ✅ (from the cache) |
| Replay cache for unchanged UI | ❌ | ✅ (no model) | ✅ |
| Parallel workers | ❌ | ✅ | ✅ |
| Mobile apps (iOS, Android) | ❌ | ✅ (Appium, Detox) | ✅ |
| Single binary, no Node runtime | ✅ | ❌ | ❌ |

In detail, with agent-browser on its own as a fourth column:

<img src="assets/screenshots/compare.png" alt="Comparison table of qaspec, scripted E2E, e2e by TesterArmy and agent-browser alone" width="860">

| | qaspec | Scripted E2E (Playwright, Cypress) | [e2e](https://github.com/tester-army/e2e) (TesterArmy) | agent-browser alone |
|---|---|---|---|---|
| What you write | Goals and expectations | Locators, waits, helpers | TypeScript with `agent.act` / `agent.assert` | Prompts to a coding agent |
| Survives UI changes | Yes, the agent finds the control again | No, selectors break | Yes, plus a replay cache | If you re-prompt |
| Judges the screen like a person | Yes, `expect('…')` quotes evidence | No | Yes, `agent.assert` | You read it |
| Console errors per step | Yes, `expect.console.noErrors()` | Hand-written listeners | Hidden from the model | `console` command |
| Network status per step | Yes, `expect.network(…).status(…)` | `waitForResponse` | Hidden from the model | `network requests` |
| Browsers per run | 1 Chromium, one tab per project | A context per test, parallel workers | A context per test, parallel workers | One per session, by hand |
| Sign in | Once per identity, reused across runs | Setup project + `storageState` | Setup test, once per run (sessions are never reused across runs) | `state save` / `load` by hand |
| Several dependent apps | `depends_on`, health checks, captures | Projects, by hand | Targets | No |
| Runtime | One Rust binary + agent-browser | Node + browsers | Node 22.12+ + Playwright | agent-browser |
| Model calls | Every goal and judged check (no replay cache yet) | None | Fewer after the first run | Every step |

Based on the e2e 0.16 and agent-browser 0.27 docs, and on porting a real two-app suite. Corrections are welcome as issues.

## Install

```bash
npm i -g agent-browser && agent-browser install     # the browser driver (one-time)
cargo install --git https://github.com/jaivial/qaspec --tag v0.1.0
```

Prebuilt Linux and macOS binaries are attached to each [GitHub release](https://github.com/jaivial/qaspec/releases).

## Quick start

```bash
qaspec init          # writes qaspec.toml + specs/home.qa.ts and ignores .qaspec/
qaspec check         # parse specs, validate config, secrets and identities — no browser
qaspec run           # run everything in one browser session
qaspec run specs/todos.qa.ts --env dev --set params.todo="Buy bread" --headed
qaspec run --keep-open   # leave the browser up; the next run reuses it (session qaspec-<env>)
qaspec state list    # saved sign-ins; `qaspec state clear app.qa` forces a new login
```

Exit codes: `0` passed, `1` a step failed, `2` blocked (environment, credentials, LLM), `3` usage/config error.
Reports are written to `.qaspec/report.json` (`--json PATH`), and `--junit PATH` adds JUnit XML for CI.

## `qaspec.toml`

```toml
default_env = "local"                       # or --env / QASPEC_ENV

[projects.app]
specs  = "specs/app/**/*.qa.ts"             # suites in these files belong to `app`
health = "/health"                          # failing health => the project's suites are `blocked`
locale = "es-ES"                            # hint for the agent
[projects.app.env.local]
base_url = "http://localhost:3000"
[projects.app.env.dev]
base_url = "https://dev.example.com"

[projects.app.identities.qa]
username = "qa@example.com"
password = { file = "~/.config/qaspec/app-password" }  # or { env = "APP_PW" } / { value = "..." }
secrets  = { otp_seed = { env = "APP_OTP" } }          # more secrets for fill_secret
login    = "specs/_login.qa.ts"             # optional: without it the agent signs in by itself
valid_if = { url_not = "/login" }           # or { js = "!!window.currentUser" }

[projects.admin]
specs = "specs/admin/**/*.qa.ts"
depends_on = ["app"]                        # run after app; blocked if app is down
[projects.admin.env.local]
base_url = "http://localhost:4000"

[params]
todo = "Write docs"
[env.dev.params]
todo = "Write docs (dev)"

[llm]                                       # any OpenAI-compatible endpoint with tool calls
base_url = "https://api.openai.com/v1"
api_key  = { env = "OPENAI_API_KEY" }
actor    = "gpt-4.1-mini"                   # drives the browser
judge    = "gpt-4.1-mini"                   # judges expect('...') (defaults to actor)

[browser]
headed = false
# session = "my-session"                    # attach to an existing agent-browser session
```

Precedence, from lowest to highest: the file, then `[env.<env>]`, then `QASPEC_PARAM_<NAME>` /
`QASPEC_<PROJECT>_BASE_URL`, then `--set`. You can override `params.<name>`,
`projects.<p>.base_url`, `llm.base_url`, `llm.actor`, `llm.judge` and `browser.headed`.

## Spec reference

| Call | Meaning |
|---|---|
| `suite(name, { project?, as?, start? }, () => {...})` | A journey. `as` = identity, `start` = first URL (default `/`). |
| `step(name, { start?, onFail?, needs?, project?, as? }, () => {...})` | One verdict. Inherits the browser state unless `start` is given. `onFail`: `stop` (default; later state-dependent steps are skipped), `continue`, `abort`. `needs`: earlier steps that must have passed. A `project` change opens that project's tab. |
| `goal(text)` | The agent acts until the goal is reached (`passed`/`failed`/`blocked`). |
| `expect(text)` | The LLM judges the screen + this step's signals and must quote evidence. |
| `expect.console.noErrors()` | No `console.error` and no uncaught error during the step. |
| `expect.errors.none()` | No uncaught page error during the step. |
| `expect.network(pattern).status(code)` / `.ok()` / `.called()` | Requests during the step. Pattern: `[METHOD] /path` with `*` (one segment) and `**`. |
| `expect.network.noFailures()` | No request answered `>= 400`. |
| `expect.url().toContain(s)` / `.not.toContain(s)` / `.toBe(s)` | Current URL. |
| `expect.state(js).equals(v)` / `.toBeTruthy()` / `.toBeFalsy()` | A JS expression evaluated in the page. |
| `expect.visible(text)` / `expect.not.visible(text)` | Text present in the page. |
| `expect.storage.local(key).equals(v)` / `.exists()` | `localStorage`. |
| `capture(name, description)` / `capture.state(name, js)` / `capture.url(name)` | Stores a value as `${name}` for later steps and files. |

Placeholders: `${params.x}`, `${project.base_url}`, `${projects.<p>.base_url}`, `${identity.username}`,
`${run.id}` (unique per run), `${env}` and any captured `${name}`. Secrets are never interpolated.

Within a step, deterministic expectations run before the judged ones, so a failing check does not spend
a model call. Anything outside this subset (`if`, `const`, `await`, variables, etc.) is rejected with
`file:line:col`.

## How it works

```
qaspec run
 └─ agent-browser --session qaspec-<run>         one Chromium for every suite
     ├─ tab "app"    ← identity app/qa  (state: .qaspec/state/<env>/app.qa.json)
     └─ tab "admin"
```

1. Plan: suites are ordered by project dependency, then grouped by identity.
2. Each project is health-checked once. Dependencies that are down make dependents `blocked`.
3. Each identity is restored from saved state and checked with `valid_if`. If that fails, the
   identity signs in again and the new state is saved, filtered to that project's host.
4. Each step marks the signal window, navigates if it has `start`, then runs goals, expectations and captures.
5. The agent's tools map one-to-one to agent-browser commands: snapshot, click, fill, type, press,
   select, hover, scroll, open, back, wait, read_console, read_network, eval, fill_secret and done.

## Examples

- [`examples/shop`](examples/shop): a storefront and its back office. Two projects with `depends_on`,
  two identities (one with a login spec, one signed in by the agent), a checkout journey with
  captures, and a back-office suite that uses the captured order number and switches back to the store tab.
- [`tests/fixtures/app`](tests/fixtures/app): a tiny runnable app (Python stdlib) with the suites used
  to validate qaspec end to end.

## Status

`0.1.0` is the first usable release. The [`plan/`](plan/) folder holds the analysis and the roadmap:
replay cache (no model calls when the UI is unchanged), `.qa.md` specs, `explore`, HTML reports,
cross-project captures with `needs` between files, and identity switching inside one project
(implemented, not yet battle-tested).

## Website

<img src="assets/screenshots/site.png" alt="The qaspec website on desktop" width="620"> <img src="assets/screenshots/site-mobile.png" alt="The qaspec website on a phone" width="150">

The site in [`site/`](site/) is built with Svelte 5 and Vite, and deployed to GitHub Pages
(`gh-pages` branch) by `.github/workflows/pages.yml`. It renders a real `report.json` and the run output
from `site/src/data/`, and imports the example specs straight from the repo.

```bash
cd site && npm ci && npm run dev          # http://localhost:5173/qaspec/
npm run build && npm run preview          # production build
```

The README screenshots come from the site's `?shot=run|report|spec|check|checks|compare` views, captured with agent-browser
(see [`site/README.md`](site/README.md)).

## Development

```bash
cargo test                                   # unit tests (parser, config, planner, matching)
python3 tests/fixtures/app/server.py 18731 & python3 tests/fixtures/app/server.py 18732 admin &
cd tests/fixtures/app && QASPEC_FIXTURE_PASSWORD=s3cret-pass QASPEC_LLM_KEY=... qaspec run
```

The fixture's `app-smoke` suite deliberately fails one step (`/broken` raises a console error and a 500).

## License

MIT
