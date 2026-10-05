# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- The website is now a Svelte 5 + Vite app in `site/`, deployed from the `gh-pages` branch. It renders
  the real output and `report.json` of a run (an interactive report viewer) and imports the example
  specs from the repo. The old static `docs/` page is gone, and it never got deployed.
- README and website show screenshots of a real run, the report, `qaspec check` and the agent's
  annotated view (`assets/screenshots/`).
- Website redesign: a neutral palette with four surface levels, one green accent for the primary action
  and passing states, and text colors that meet WCAG AA on every surface. IBM Plex Sans and JetBrains
  Mono are self-hosted. Decorative cards and the window-chrome colors are gone.
- Mobile: a menu button, 44px touch targets, no horizontal scroll from 375px up, tables that become
  stacked rows, and a sticky "qaspec vs" switch that compares one tool at a time.
- New "How it compares" tables (writing tests, what a check can see, running them) against scripted
  E2E, e2e by TesterArmy and agent-browser alone, on the website and in the README.
- New screenshots of the three things the agent does (sign in, reach a goal, catch a page that looks
  fine) and of the back-office app. Images ship as WebP with PNG fallback.
- The demo app in `tests/fixtures/app` has a plain stylesheet, so its screenshots look like a real app.

### Fixed
- The agent's `click` and `hover` scroll the target into view first. agent-browser 0.27 clicks at
  viewport coordinates, so off-screen elements were missed with no error.

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
