# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- **Replay cache for goals.** A goal that passed with all of its expectations passing is recorded as
  the sequence of browser actions the agent performed, in `.qaspec/cache/<env>/<sha>.json` (mode 600).
  The next run replays it with zero model calls; `fill_secret` is stored as the secret *name* only and
  its value is read at replay time and typed through agent-browser's stdin, so it is never in argv and
  never in the file. Refs are never stored: each action keeps a semantic locator (role + accessible
  name, plus an index when the page has several) and is replayed with `find`, with `label`, `nth` and
  the bare tag as fallbacks for the tags where agent-browser 0.27's `find --name` finds nothing.
  The key hashes project, identity, spec file, suite, step, the interpolated goal text (`${run.id}`
  back as a placeholder) and a fingerprint of the page the goal starts from (URL path + the
  interactive roles and names); recorded values keep their placeholders, so a replay uses this run's
  `${params.x}`, `${run.id}` and captures.
- `qaspec run --cache auto|strict|off` (default `auto`). `auto` replays and re-records, self-healing
  from the current page when a replayed action cannot be located; `strict` makes a missing or stale
  recording a failure (`CACHE_MISSING` / `CACHE_REPLAY_FAILED`) with zero model calls, for CI.
- Goal items in the JSON report carry `"cache": "replayed" | "recorded" | "agent"`, the terminal marks
  those steps with `(replayed)`, and the run summary counts goals replayed and recorded.
- Mobile web: a suite can run on an emulated phone or tablet. `suite('x', { device: 'Pixel 7' })` in
  the spec, `device` / `viewport` in `[projects.<p>]`, or a default in `[browser]`; precedence is
  suite > project > browser and `qaspec check` prints what each suite will run on. The device is
  applied before the suite's first step (on the tab that suite drives, since agent-browser applies
  emulation per target) and fully undone after the suite, so a run mixing a phone suite with a
  desktop one works. The agent is told `Viewport: iPhone 14, 390x844, mobile user agent`, so it
  scrolls and opens menus as on a phone. Unknown option errors keep the `file:line:col` style.
  `--set browser.device=<name>` and `--set projects.<p>.device=<name>` too.
- `tests/fixtures/app/specs/app-mobile.qa.ts`: a deterministic mobile suite (no model call) that
  proves the emulation reaches the page: CSS size, pixel ratio and the mobile user agent.
- `tests/fixtures/app/specs/app-desktop-after-mobile.qa.ts`: a desktop suite that asserts a suite
  without a `device` still runs at desktop size, with the desktop user agent, still signed in.
- The demo app in `tests/fixtures/app` serves `<meta name="viewport">`, so its mobile layout is the
  real one instead of the 980px fallback browsers use when the tag is missing.
- `qaspec run --jobs N`: opt-in parallel runs (default 1, unchanged). The planned suites are split
  into N independent groups balanced by number of steps; each worker gets its own `Runner`, its own
  agent-browser session (`qaspec-<run>-w0`, `-w1`, ...), its own LLM client and a `std::thread` (no
  async runtime). Suites stay in the same worker when they share a `(project, identity)`, when their
  projects are connected by `depends_on` or by a step that switches `project:` and use the same
  identity, or when one captures a value that a later suite uses as `${name}`; each worker keeps plan
  order, and qaspec asserts that two workers never write the state file of the same identity.
- Memory guard for `--jobs`: reads `MemAvailable` from `/proc/meminfo`, assumes 1536 MiB per browser
  session and keeps 2 GiB free, lowering `--jobs` to what fits and printing a warning.
  `--force` skips the check; systems without `/proc/meminfo` skip it with a warning.
- With more than one session, each suite's terminal lines are buffered and printed when the suite
  ends, prefixed with `[w<i>]`, so workers never interleave mid-line. The report keeps every suite in
  plan order, the summary says "N browser sessions", and the JSON report gains a `browserSessions`
  array (`browserSession` is still the first session, so `report-1` readers keep working).
- With `--jobs`, every suite that sets a `device` runs in the first worker and last in it:
  agent-browser 0.27 cannot undo the mobile user agent, so two workers can each run one safely,
  but a desktop suite after a device suite in the same worker could not.

### Changed
- The website is now a Svelte 5 + Vite app in `site/`, deployed from the `gh-pages` branch. It renders
  the real output and `report.json` of a run (an interactive report viewer) and imports the example
  specs from the repo. The old static `docs/` page is gone, and it never got deployed.
- README and website show screenshots of a real run, the report, `qaspec check` and the agent's
  annotated view (`assets/screenshots/`).
- Website redesign: a neutral palette with four surface levels, one green accent for the primary action
  and passing states, and text colors that meet WCAG AA on every surface. IBM Plex Sans and JetBrains
  Mono are self-hosted. Decorative cards and the window-chrome colors are gone.
- Mobile: 20px page gutters (sections had lost their side padding), a menu button, 44px touch targets, no horizontal scroll from 375px up, tables that become
  stacked rows, and a sticky "qaspec vs" switch that compares one tool at a time.
- A yes/no comparison table of qaspec, regular E2E and agentic E2E (TesterArmy), on the website and
- A yes/no comparison table of qaspec, regular E2E and agentic E2E (TesterArmy), on the website and
  in the README. It includes the rows where qaspec loses: no native mobile apps (mobile *web* is
  covered by device emulation). On phones it stays a three-column table with short headers.
- The detailed comparison now says that e2e sessions last one run and are never reused across runs.
- New "How it compares" tables (writing tests, what a check can see, running them) against scripted
  E2E, e2e by TesterArmy and agent-browser alone, on the website and in the README.
- New screenshots of the three things the agent does (sign in, reach a goal, catch a page that looks
  fine) and of the back-office app. Images ship as WebP with PNG fallback.
- The demo app in `tests/fixtures/app` has a plain stylesheet, so its screenshots look like a real app.
- README: the comparison tables and the roadmap no longer say qaspec has no replay cache; there is a
  new "Replay cache" section describing the key, the locators, secrets, self-healing and the modes.

### Fixed
- The agent's `click` and `hover` scroll the target into view first. agent-browser 0.27 clicks at
  viewport coordinates, so off-screen elements were missed with no error.
- A device's mobile user agent no longer leaks into later suites. agent-browser 0.27 cannot undo
  `set device`: there is no desktop device name, the user-agent override survives `set viewport`,
  and the only real undo (`--user-agent ""`) **relaunches the browser, closing every tab and
  dropping every cookie**. qaspec therefore plans suites that set a `device` last and prints a note
  in `qaspec run` and `qaspec check`, instead of restoring the user agent and breaking the session.
  `viewport`-only suites still restore the window size and can run anywhere.
  See `tests/fixtures/app/specs/app-desktop-after-mobile.qa.ts`.

## [0.1.0] - 2026-10-05

First release.

### Added
- `qaspec run`, `check`, `init`, `state list|clear` CLI (single Rust binary).
- `*.qa.ts` spec format: TypeScript syntax parsed (never executed) into suites and ordered steps,
  with `goal`, `expect(...)` (LLM judge), deterministic `expect.console`, `expect.errors`,
  `expect.network`, `expect.url`, `expect.state`, `expect.visible`, `expect.storage.local`, and
  `capture` / `capture.state` / `capture.url`. Errors report `file:line:col`.
- Ordered steps that share browser state, with `start`, `onFail` (`stop`/`continue`/`abort`) and `needs`.
- Per-step signal windows: console errors, uncaught page errors and network requests are checked
  only for what happened during the step.
- One agent-browser session (one Chromium) per run, with one tab per project; `--keep-open` reuses it
  across runs.
- `qaspec.toml`: projects, per-environment `base_url`, `health`, `depends_on`, `locale`, identities with
  secrets (`file`/`env`/`value`), `login` spec or agent sign-in, `valid_if`, params with env overrides,
  `--set`, `QASPEC_ENV`, `QASPEC_PARAM_*`.
- Saved sign-in state per identity in `.qaspec/state/<env>/` (mode 600, filtered by host), reused
  until `valid_if` fails.
- Agent loop over any OpenAI-compatible endpoint with tool calls; `fill_secret` types secrets through
  agent-browser's stdin batch mode and redacts them from logs and reports.
- Verdicts `passed` / `failed` / `blocked` / `skipped` with exit codes 0/1/2/3, a JSON report
  (`report-1`) and JUnit XML.
- Several identities per project in one run: the planner groups suites by identity, and a switch saves
  the current state, clears cookies, restores the other projects' identities and loads the new one.
- A suite can move between projects step by step; each project keeps its own tab, so returning to a
  project continues where its tab was.
- Login suites are reported as `setup` in the JSON report and are excluded from step totals.
- Example `examples/shop` (two dependent projects) and a runnable fixture app in `tests/fixtures/app`.
- Planning analysis and roadmap in `plan/`.

### Fixed during validation
- Large agent-browser outputs (hundreds of KB of network requests on real apps) no longer deadlock the
  pipe: stdout and stderr are drained in threads.
- `open` on apps whose `load` event never settles (agent-browser times out after about 25s) counts as
  arrived when the tab is already on the target host.
- `expect.visible` reads the page's visible text. `snapshot -c` drops static text in agent-browser 0.27.
- The judge gets the visible text plus a larger snapshot, so long chat answers are not truncated away.
- Older tool results in the agent transcript are compacted, so token use does not grow with every action.

### Known limitations
- No replay cache yet: every `goal` calls the model.
- Captures are shared across files in run order, but `needs` between files is not implemented.
- agent-browser 0.27 `errors --clear` is a no-op and logs are shared by tabs; qaspec works around it
  with offsets, so the per-step windows assume one active tab at a time.

[Unreleased]: https://github.com/jaivial/qaspec/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jaivial/qaspec/releases/tag/v0.1.0
