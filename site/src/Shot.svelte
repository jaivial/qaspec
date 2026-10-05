<script>
  import Terminal from './lib/Terminal.svelte';
  import Report from './lib/Report.svelte';
  import Code from './lib/Code.svelte';
  import Compare from './lib/Compare.svelte';
  import Checks from './lib/Checks.svelte';
  import { checkCols, checkRows, columns, writing, seeing, running } from './data/compare.js';
  import run from './data/run.txt?raw';
  import check from './data/check.txt?raw';
  import report from './data/report.json';
  import spec from '../../tests/fixtures/app/specs/app-agent.qa.ts?raw';

  let { name } = $props();

  // Short screenshot version of the detailed table.
  const rows = [writing[0], seeing[0], seeing[1], seeing[2], running[0], running[1], running[3]];
</script>

<div class="shot" id="shot">
  {#if name === 'run'}
    <Terminal title="qaspec run" text={'$ qaspec run specs/app-agent.qa.ts specs/app-smoke.qa.ts specs/admin-overview.qa.ts\n' + run} />
  {:else if name === 'check'}
    <Terminal title="qaspec check" text={'$ qaspec check\n' + check} />
  {:else if name === 'report'}
    <Report {report} />
  {:else if name === 'checks'}
    <Checks caption="qaspec, regular E2E and agentic E2E" columns={checkCols.map((c) => (Array.isArray(c) ? c[0] : c))} rows={checkRows} />
  {:else if name === 'compare'}
    <Compare caption="qaspec compared" {columns} {rows} />
  {:else if name === 'spec'}
    <Code code={spec} title="specs/app-agent.qa.ts" />
  {/if}
</div>

<style>
  .shot { width: 1000px; padding: 24px; background: var(--bg); }
</style>
