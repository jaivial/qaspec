<script>
  import Terminal from './lib/Terminal.svelte';
  import Report from './lib/Report.svelte';
  import Code from './lib/Code.svelte';
  import Compare from './lib/Compare.svelte';
  import Checks from './lib/Checks.svelte';
  import run from './data/run.txt?raw';
  import check from './data/check.txt?raw';
  import report from './data/report.json';
  import spec from '../../tests/fixtures/app/specs/app-agent.qa.ts?raw';

  let { name } = $props();

  const checkCols = ['qaspec', 'Regular E2E', 'Agentic E2E (TesterArmy)'];
  const checkRows = [
    ['Tests written without selectors', true, false, [true, 'Mixed with locators']],
    ['Survives UI changes without edits', true, false, true],
    ['Judges the screen like a person', true, false, true],
    ['The model sees console errors', true, false, false],
    ['The model sees network requests', true, false, false],
    ['Console and network checks built in', true, false, false],
    ['Ordered checks sharing browser state', true, true, true],
    ['Sign-in reused across runs', true, true, [false, 'Once per run']],
    ['One browser for the whole run', true, false, false],
    ['Dependent apps: health, order, shared data', true, false, false],
    ['Passwords never reach the model', true, [true, 'No model'], true],
    ['Runs without calling a model', [false, 'Exact checks only'], true, [true, 'From the cache']],
    ['Replay cache for unchanged UI', false, [true, 'No model'], true],
    ['Parallel workers', false, true, true],
    ['Mobile apps (iOS, Android)', false, [true, 'Appium, Detox'], true],
    ['Single binary, no Node runtime', true, false, false]
  ];
  const columns = ['qaspec', 'Scripted E2E', 'e2e (TesterArmy)', 'agent-browser alone'];
  const rows = [
    ['What you write', 'Goals and expectations', 'Locators, waits, helpers', 'Code plus agent steps', 'Prompts to a coding agent'],
    ['Judges the screen like a person', ['yes', "expect('…') with evidence"], 'no', ['yes', 'agent.assert'], ['partial', 'You read it']],
    ['Console errors per step', ['yes', 'expect.console.noErrors()'], ['partial', 'Hand-written listeners'], ['no', 'Hidden from the model'], ['partial', 'console command']],
    ['Network status per step', ['yes', "expect.network('POST /x').status(201)"], ['partial', 'waitForResponse'], ['no', 'Hidden from the model'], ['partial', 'network requests']],
    ['Browsers per run', ['1 Chromium', 'One tab per project'], ['1 context per test', 'Workers in parallel'], ['1 context per test', 'Workers in parallel'], ['1 per session', 'You manage it']],
    ['Sign in', ['Once per identity', 'Reused across runs'], 'Setup project + storageState', 'Setup test + saved session', 'state save / load by hand'],
    ['Runtime', 'One Rust binary + agent-browser', 'Node + browsers', 'Node 22.12+ + Playwright', 'agent-browser']
  ];
</script>

<div class="shot" id="shot">
  {#if name === 'run'}
    <Terminal title="qaspec run" text={'$ qaspec run specs/app-agent.qa.ts specs/app-smoke.qa.ts specs/admin-overview.qa.ts\n' + run} />
  {:else if name === 'check'}
    <Terminal title="qaspec check" text={'$ qaspec check\n' + check} />
  {:else if name === 'report'}
    <Report {report} />
  {:else if name === 'checks'}
    <Checks caption="qaspec, regular E2E and agentic E2E" columns={checkCols} rows={checkRows} />
  {:else if name === 'compare'}
    <Compare caption="qaspec compared" {columns} {rows} />
  {:else if name === 'spec'}
    <Code code={spec} title="specs/app-agent.qa.ts" />
  {/if}
</div>

<style>
  .shot { width: 1000px; padding: 24px; background: var(--bg); }
</style>
