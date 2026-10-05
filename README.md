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

```
$ qaspec run
▶ specs/todos.qa.ts › todos journey  [app as qa]
  ✓ app/qa: reused saved session
  ✓ go to the list (10.4s)
  ✓ add a todo (7.9s)
  ✓ home shows the count (4.1s)

✓ PASSED — steps: 3 passed, 0 failed, 0 blocked, 0 skipped · 1 suites · 22.7s · 1 browser session · 55 browser calls · 8 model calls
```

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

## Development

```bash
cargo test                                   # unit tests (parser, config, planner, matching)
python3 tests/fixtures/app/server.py 18731 & python3 tests/fixtures/app/server.py 18732 admin &
cd tests/fixtures/app && QASPEC_FIXTURE_PASSWORD=s3cret-pass QASPEC_LLM_KEY=... qaspec run
```

The fixture's `app-smoke` suite deliberately fails one step (`/broken` raises a console error and a 500).

## License

MIT
