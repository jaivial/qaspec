<script>
  import Terminal from './lib/Terminal.svelte';
  import Report from './lib/Report.svelte';
  import Code from './lib/Code.svelte';
  import Compare from './lib/Compare.svelte';
  import run from './data/run.txt?raw';
  import check from './data/check.txt?raw';
  import report from './data/report.json';
  import spec from '../../tests/fixtures/app/specs/app-agent.qa.ts?raw';

  let { name } = $props();

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
  {:else if name === 'compare'}
    <Compare caption="qaspec compared" {columns} {rows} />
  {:else if name === 'spec'}
    <Code code={spec} title="specs/app-agent.qa.ts" />
  {/if}
</div>

<style>
  .shot { width: 1000px; padding: 24px; background: var(--bg); }
</style>
