<script>
  /** Interactive view of a qaspec `report.json` (schema report-1). */
  let { report, expandFirstFailure = true } = $props();

  const icon = { passed: '✓', failed: '✗', blocked: '⊘', skipped: '○' };
  const fmt = (ms) => (ms / 1000).toFixed(1) + 's';
  const suites = $derived(report.suites.filter((s) => !s.setup));
  const firstFail = $derived(
    expandFirstFailure ? suites.flatMap((s) => s.steps.map((st) => `${s.suite}/${st.name}`))
      .find((k) => {
        const [su, ...rest] = k.split('/');
        const st = suites.find((x) => x.suite === su)?.steps.find((y) => y.name === rest.join('/'));
        return st && st.status === 'failed';
      }) : null
  );
  let open = $state({});
  $effect(() => { if (firstFail && open[firstFail] === undefined) open[firstFail] = true; });
  const toggle = (k) => (open[k] = !open[k]);
</script>

<div class="report">
  <header>
    <span class="badge {report.status}">{icon[report.status]} {report.status.toUpperCase()}</span>
    <span>{report.steps.passed} passed · {report.steps.failed} failed · {report.steps.blocked} blocked · {report.steps.skipped} skipped</span>
    <span class="muted">{fmt(report.durationMs)} · 1 browser session · {report.browserCalls} browser calls · {report.modelCalls} model calls</span>
  </header>
  {#each suites as s}
    <section>
      <h4><span class={s.status}>{icon[s.status]}</span> {s.suite} <small>{s.file} · {s.project}{s.identity ? ` as ${s.identity}` : ''}</small></h4>
      {#each s.steps as st}
        {@const k = `${s.suite}/${st.name}`}
        <button class="step" onclick={() => toggle(k)} aria-expanded={!!open[k]}>
          <span class={st.status}>{icon[st.status]}</span>
          <span class="name">{st.name}</span>
          {#if st.signals}
            <span class="sig" title="console errors · page errors · requests (failed)">
              <i class:bad={st.signals.consoleErrors}>console {st.signals.consoleErrors}</i>
              <i class:bad={st.signals.pageErrors}>errors {st.signals.pageErrors}</i>
              <i class:bad={st.signals.failedRequests}>net {st.signals.requests}{st.signals.failedRequests ? ` (${st.signals.failedRequests} failed)` : ''}</i>
            </span>
          {/if}
          <span class="muted dur">{fmt(st.durationMs)}</span>
        </button>
        {#if open[k]}
          <div class="detail">
            {#each st.items as it}
              <div class="item"><span class={it.status}>{icon[it.status]}</span> <code>{it.label}</code>{#if it.detail}<div class="ev">{it.detail}</div>{/if}</div>
            {/each}
            {#if st.actions.length}
              <div class="actions"><b>agent actions</b>{#each st.actions as a}<div>› {a}</div>{/each}</div>
            {/if}
          </div>
        {/if}
      {/each}
    </section>
  {/each}
</div>

<style>
  .report { background: var(--panel); border: 1px solid var(--line); border-radius: 10px; overflow: hidden; font-size: 14px; }
  header { display: flex; flex-wrap: wrap; gap: 6px 16px; align-items: center; padding: 12px 16px; border-bottom: 1px solid var(--line); background: var(--panel2); }
  .badge { font-weight: 700; padding: 2px 10px; border-radius: 999px; border: 1px solid currentColor; }
  section { padding: 10px 8px 12px; border-bottom: 1px solid var(--line); }
  section:last-child { border-bottom: 0; }
  h4 { margin: 4px 8px 8px; font-size: 15px; } h4 small { color: var(--muted); font-weight: 400; font-family: var(--mono); font-size: 12px; margin-left: 6px; }
  .step { all: unset; box-sizing: border-box; display: flex; align-items: center; gap: 10px; width: 100%; padding: 6px 10px; border-radius: 6px; cursor: pointer; }
  .step:hover, .step:focus-visible { background: rgba(110,118,129,.15); }
  .name { flex: 1; }
  .sig { display: flex; gap: 6px; } .sig i { font-style: normal; font: 11px var(--mono); color: var(--muted); border: 1px solid var(--line); border-radius: 4px; padding: 0 6px; }
  .sig i.bad { color: var(--bad); border-color: rgba(248,81,73,.5); }
  .dur { font: 12px var(--mono); min-width: 44px; text-align: right; }
  .detail { margin: 2px 10px 8px 34px; padding: 10px 12px; border-left: 2px solid var(--line); background: rgba(1,4,9,.4); border-radius: 0 6px 6px 0; }
  .item { margin: 4px 0; } .ev { color: var(--muted); font-size: 13px; margin: 2px 0 6px 20px; }
  .actions { margin-top: 8px; font: 12px var(--mono); color: var(--muted); } .actions b { color: var(--text); font: 600 12px var(--mono); }
  .passed { color: var(--accent); } .failed { color: var(--bad); } .blocked { color: var(--warn); } .skipped { color: var(--muted); }
  .muted { color: var(--muted); }
  @media (max-width: 700px) { .sig { display: none; } }
</style>
