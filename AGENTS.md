# Using qaspec from a coding agent

Loop: write a spec, validate it, run it, read only what failed.

1. Create: `qaspec init` once, then `qaspec new <name>` for each spec (writes `specs/<name>.qa.ts`; see README for `suite`, `step`, `goal`, `expect`).
2. Validate without a browser: `qaspec check` or `qaspec check --json` (exit code 0 means specs and config are valid; the JSON lists suites, steps and warnings).
3. Run: `qaspec run --format json -q > run.json`, or `--format ndjson` for one event per step plus a final `run` line (emitted when the run ends). stdout is the full JSON report, the text summary goes to stderr. Exit code 0 passed, 1 failed, other values blocked or setup problems.
4. Read failures: `qaspec report --todo` for a checkbox list with the agent's cause, or
   `qaspec report --failed` (text) / `qaspec report --failed --json`. Use `--step <text>` to look at one step.
   For live automation output, use `qaspec run --format ndjson`; one step event is emitted as soon as
   that step finishes, followed by the final `run` event.
5. Fix the app or the spec and rerun. A passing goal is replayed from the cache without model calls; use `--cache strict` in CI.

Never put passwords in specs: use secrets in qaspec.toml. The model only sees secret names.

With `--format json|ndjson` or `--json`, a failure that is not a test result (missing config, invalid spec) prints `{"ok":false,"error":"...","exitCode":3}` on stdout and exits 3.
