<script>
  /** Interactive view of a qaspec `report.json` (schema report-1). */
  let { report, expandFirstFailure = true } = $props();

  const icon = { passed: '✓', failed: '✗', blocked: '⊘', skipped: '○' };
  const fmt = (ms) => (ms / 1000).toFixed(1) + ' s';
  const suites = $derived(report.suites.filter((s) => !s.setup));
  const key = (s, st) => `${s.suite}\u0000${st.name}`;

  let open = $state({});
  $effect(() => {
    if (!expandFirstFailure) return;
    for (const s of suites)
      for (const st of s.steps)
        if (st.status === 'failed') {
          if (open[key(s, st)] === undefined) open[key(s, st)] = true;
          return;
        }
  });
</script>

<div class="report">
  <header>
    <span class="verdict {report.status}">{icon[report.status]} {report.status}</span>
    <span class="num">{report.steps.passed} passed, {report.steps.failed} failed, {report.steps.blocked} blocked</span>
    <span class="meta num">{fmt(report.durationMs)} · 1 browser session · {report.modelCalls} model calls</span>
  </header>
  <p class="hint">Select a step to see its checks, evidence and agent actions.</p>
  {#each suites as s}
    <section>
      <h4><span class="{s.status}" aria-hidden="true">{icon[s.status]}</span> {s.suite}</h4>
      <p class="where">{s.file} · {s.project}{s.identity ? ` as ${s.identity}` : ''}</p>
      <ul>
        {#each s.steps as st}
          {@const k = key(s, st)}
          <li>
            <button class="step" onclick={() => (open[k] = !open[k])} aria-expanded={!!open[k]}>
              <span class="ico {st.status}" aria-hidden="true">{icon[st.status]}</span>
              <span class="body">
                <span class="name">{st.name}<span class="sr-only"> ({st.status})</span></span>
                {#if st.signals}
                  <span class="sig num">
                    <span class:bad={st.signals.consoleErrors}>console {st.signals.consoleErrors}</span>
                    <span class:bad={st.signals.pageErrors}>errors {st.signals.pageErrors}</span>
                    <span class:bad={st.signals.failedRequests}>requests {st.signals.requests}{st.signals.failedRequests ? `, ${st.signals.failedRequests} failed` : ''}</span>
                  </span>
                {/if}
              </span>
              <span class="dur num">{fmt(st.durationMs)}</span>
              <svg class="chev" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
            </button>
            {#if open[k]}
              <div class="detail">
                {#each st.items as it}
                  <div class="item">
                    <span class={it.status} aria-hidden="true">{icon[it.status]}</span>
                    <div><code>{it.label}</code>{#if it.detail}<p class="ev">{it.detail}</p>{/if}</div>
                  </div>
                {/each}
                {#if st.actions.length}
                  <p class="actions-h">Agent actions</p>
                  <ol class="actions">{#each st.actions as a}<li>{a}</li>{/each}</ol>
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/each}
</div>

<style>
  .report { background: var(--surface); border-radius: var(--r-md); box-shadow: 0 0 0 1px var(--border); overflow: hidden; font-size: 14px; min-width: 0; }
  header { display: flex; flex-wrap: wrap; gap: 4px 14px; align-items: center; padding: 12px 16px; background: var(--raised); border-bottom: 1px solid var(--border); }
  .verdict { font-weight: 600; text-transform: capitalize; }
  .meta { color: var(--text-3); font-size: 13px; }
  .hint { margin: 0; padding: 8px 16px; font-size: 13px; color: var(--text-3); border-bottom: 1px solid var(--border); }
  section { padding: 12px 8px 8px; border-bottom: 1px solid var(--border); }
  section:last-child { border-bottom: 0; }
  h4 { margin: 0 8px; font-size: 15px; font-weight: 500; }
  .where { margin: 0 8px 6px 28px; color: var(--text-3); font: 13px var(--mono); overflow-wrap: anywhere; }
  ul { list-style: none; margin: 0; padding: 0; }
  .step {
    all: unset; box-sizing: border-box; display: flex; align-items: flex-start; gap: 10px; width: 100%;
    min-height: 44px; padding: 10px 8px; border-radius: var(--r-sm); cursor: pointer;
    transition-property: background-color; transition-duration: 150ms; transition-timing-function: var(--ease);
  }
  .step:hover { background: var(--overlay); }
  .step:hover .chev { translate: 2px 0; }
  .step:focus-visible { outline: 2px solid var(--link); outline-offset: -2px; }
  .ico { width: 14px; flex: none; line-height: 1.5; }
  .body { flex: 1; min-width: 0; display: flex; flex-wrap: wrap; gap: 2px 12px; align-items: baseline; }
  .name { color: var(--text); }
  .sig { display: flex; flex-wrap: wrap; gap: 4px 10px; color: var(--text-3); font: 13px var(--mono); }
  .sig .bad { color: var(--bad); }
  .dur { color: var(--text-3); font: 13px var(--mono); line-height: 1.75; flex: none; }
  .chev { flex: none; margin-top: 3px; color: var(--link); transition-property: rotate; transition-duration: 150ms; transition-timing-function: var(--ease); }
  .step[aria-expanded='true'] .chev { rotate: 90deg; }
  .detail { margin: 2px 8px 10px 32px; padding: 4px 0 4px 14px; border-left: 1px solid var(--border-strong); }
  .item { display: flex; gap: 8px; margin: 6px 0; } .item > div { min-width: 0; }
  .item code { overflow-wrap: anywhere; }
  .ev { color: var(--text-2); font-size: 14px; margin: 4px 0 0; overflow-wrap: anywhere; }
  .actions-h { margin: 12px 0 4px; font-size: 12px; color: var(--text-2); font-weight: 500; }
  .actions { margin: 0; padding-left: 20px; font: 12px/1.7 var(--mono); color: var(--text-3); overflow-wrap: anywhere; }
  .passed { color: var(--accent); } .failed { color: var(--bad); } .blocked { color: var(--warn); } .skipped { color: var(--text-3); }
  @media (prefers-reduced-motion: reduce) { .step, .chev { transition: none; } }
</style>
