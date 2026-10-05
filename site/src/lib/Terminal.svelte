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
  .term { background: #010409; border: 1px solid var(--line); border-radius: 10px; overflow: hidden; box-shadow: 0 10px 40px rgba(0,0,0,.45); display: flex; flex-direction: column; }
  .bar { display: flex; align-items: center; gap: 7px; padding: 10px 14px; background: var(--panel); border-bottom: 1px solid var(--line); }
  .bar span { width: 11px; height: 11px; border-radius: 50%; background: #484f58; }
  .bar span:nth-child(1) { background: #ff5f57; } .bar span:nth-child(2) { background: #febc2e; } .bar span:nth-child(3) { background: #28c840; }
  .bar em { font-style: normal; color: var(--muted); font: 12px var(--mono); margin-left: 8px; }
  pre { margin: 0; padding: 16px 18px; font: 13px/1.6 var(--mono); overflow: auto; flex: 1; }
  pre div { white-space: pre-wrap; word-break: break-word; }
  .cmd { color: var(--text); font-weight: 600; }
  .suite { color: var(--accent2); font-weight: 600; margin-top: 4px; }
  .ok { color: var(--accent); } .bad { color: var(--bad); } .warn { color: var(--warn); } .muted { color: var(--muted); }
  .strong { font-weight: 700; margin-top: 4px; }
</style>
