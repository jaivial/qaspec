// Single source for the comparison tables on the site and in the screenshot views.
// Checked against e2e 0.16 and agent-browser 0.27 docs. Rows where qaspec loses stay in.

export const checkCols = ['qaspec', ['Regular E2E', 'Regular'], ['Agentic E2E (TesterArmy)', 'Agentic']];
export const checkRows = [
  ['Tests written without selectors', true, false, [true, 'Mixed with locators']],
  ['Survives UI changes without edits', true, false, true],
  ['Judges the screen like a person', true, false, true],
  ['The model sees console errors', true, false, false],
  ['The model sees network requests', true, false, false],
  ['Console and network checks built in', true, false, false],
  ['Ordered checks sharing browser state', true, true, true],
  ['Sign-in reused across runs', true, true, [false, 'Once per run']],
  ['One browser for the whole run', [true, 'Default'], false, false],
  ['Dependent apps: health, order, shared data', true, false, false],
  ['Passwords never reach the model', true, [true, 'No model'], true],
  ['Replay cache for unchanged UI', true, [true, 'No model'], true],
  ['Goals run without a model call', [true, 'From the cache'], [true, 'No model'], [true, 'From the cache']],
  ['Judged checks without a model call', false, [true, 'No model'], false],
  ['Parallel workers', [true, 'Opt-in, --jobs'], true, true],
  ['Mobile web (device emulation)', [true, 'device = …'], true, true],
  ['Native mobile apps (iOS, Android)', false, [true, 'Appium, Detox'], true],
  ['Single binary, no Node runtime', true, false, false]
];

export const columns = ['qaspec', 'Scripted E2E', 'e2e (TesterArmy)', 'agent-browser alone'];

export const writing = [
  ['What you write', 'Goals and expectations', 'Locators, waits, helpers', ['Code plus agent steps', 'agent.act / agent.assert in TypeScript'], 'Prompts to a coding agent'],
  ['Test file', ['Declarative .qa.ts', 'Parsed, never executed'], 'Executable TS/JS', 'Executable TypeScript', ['None', 'No test format']],
  ['Survives UI changes', ['yes', 'Replays, then the agent heals'], ['no', 'Selectors break'], ['yes', 'Plus a replay cache'], ['partial', 'If you re-prompt']],
  ['Many checks in order, shared state', ['yes', 'Steps in a suite'], ['yes', 'Inside one test'], ['partial', 'serial groups'], ['partial', 'Manual']],
  ['Repeatable in CI', ['yes', 'Exit codes, JSON, JUnit, --cache strict'], 'yes', 'yes', ['no', 'Interactive tool']]
];

export const seeing = [
  ['Judges the screen like a person', ['yes', "expect('…') with evidence"], 'no', ['yes', 'agent.assert'], ['partial', 'You read it']],
  ['Console errors per step', ['yes', 'expect.console.noErrors()'], ['partial', 'Hand-written listeners'], ['no', 'Hidden from the model'], ['partial', 'console command']],
  ['Network status per step', ['yes', "expect.network('POST /x').status(201)"], ['partial', 'waitForResponse'], ['no', 'Hidden from the model'], ['partial', 'network requests']],
  ['App state (JS, localStorage)', ['yes', 'expect.state, expect.storage'], ['yes', 'page.evaluate'], ['partial', 'Not to the model'], ['partial', 'eval']],
  ['Phone-sized layouts', ['yes', "device: 'iPhone 14'"], ['yes', 'Device descriptors'], ['yes', 'Viewports, devices'], ['yes', 'set device']],
  ['Secrets kept from the model', ['yes', 'Typed via stdin, redacted'], ['partial', 'No model involved'], 'yes', ['no', 'Whatever you paste']]
];

export const running = [
  ['Browsers per run', ['1 Chromium by default', 'One tab per project; --jobs N opt-in'], ['1 context per test', 'Workers in parallel'], ['1 context per test', 'Workers in parallel'], ['1 per session', 'You manage it']],
  ['Sign in', ['Once per identity', 'Saved state reused across runs'], 'Setup project + storageState', ['Setup test, once per run', 'Sessions are never reused across runs'], 'state save / load by hand'],
  ['Several apps that depend on each other', ['yes', 'depends_on, health, captures'], ['partial', 'Projects, by hand'], ['partial', 'Targets'], 'no'],
  ['Runtime', 'One Rust binary + agent-browser', 'Node + browsers', 'Node 22.12+ + Playwright', 'agent-browser'],
  ['Model calls per run', ['First run: goals and judged checks', 'Then only judged checks'], 'None', ['Fewer after the first run', 'Replay cache'], 'Every step']
];
