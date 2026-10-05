<script>
  /** Renders captured `qaspec` output with the colors the CLI uses. */
  let { text, title = 'terminal', height = null } = $props();

  const classify = (line) => {
    const t = line.trimStart();
    if (t.startsWith('$ ')) return 'cmd';
    if (t.startsWith('▶')) return 'suite';
    if (/^✓ PASSED/.test(t)) return 'ok strong';
    if (/^✗ FAILED/.test(t)) return 'bad strong';
    if (/^⊘ BLOCKED/.test(t)) return 'warn strong';
    if (t.startsWith('✓')) return 'ok';
    if (t.startsWith('✗')) return 'bad';
    if (t.startsWith('⊘')) return 'warn';
    if (t.startsWith('○') || t.startsWith('↺') || t.startsWith('→')) return 'muted';
    if (t.startsWith('ok —')) return 'ok';
    if (t.startsWith('warning')) return 'warn';
    return '';
  };
  const lines = $derived(text.replace(/\n+$/, '').split('\n'));
</script>

<div class="term" style:height={height}>
  <div class="bar"><span></span><span></span><span></span><em>{title}</em></div>
  <pre>{#each lines as line}<div class={classify(line)}>{line || ' '}</div>{/each}</pre>
</div>

<style>
  .term { background: #07090b; border-radius: var(--r-md); overflow: hidden; box-shadow: 0 0 0 1px var(--border), 0 8px 24px rgb(0 0 0 / 0.35); display: flex; flex-direction: column; min-width: 0; }
  .bar { display: flex; align-items: center; gap: 7px; padding: 10px 14px; background: var(--surface); border-bottom: 1px solid var(--border); }
  .bar span { width: 10px; height: 10px; border-radius: 50%; background: var(--overlay); box-shadow: inset 0 0 0 1px var(--border-strong); }
  .bar em { font: 12px var(--mono); font-style: normal; color: var(--text-3); margin-left: 6px; }
  pre { margin: 0; padding: 14px 16px; font: 12.5px/1.65 var(--mono); overflow: auto; flex: 1; -webkit-overflow-scrolling: touch; }
  @media (max-width: 640px) { pre { font-size: 11.5px; padding: 12px; } }
  pre div { white-space: pre-wrap; overflow-wrap: anywhere; }
  .cmd { color: var(--text); }
  .suite { color: var(--link); font-weight: 500; margin-top: 4px; }
  .ok { color: var(--accent); } .bad { color: var(--bad); } .warn { color: var(--warn); } .muted { color: var(--text-3); }
  .strong { font-weight: 500; margin-top: 4px; }
</style>
