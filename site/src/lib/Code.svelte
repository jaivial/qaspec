<script>
  /** Tiny highlighter for qaspec specs (TS subset) and TOML. */
  let { code, lang = 'ts', title = null } = $props();

  const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  function highlight(src, lang) {
    const out = [];
    const re = lang === 'toml'
      ? /(#.*$)|("(?:[^"\\]|\\.)*")|(^\s*\[[^\]]+\])|(\b[A-Za-z_][\w-]*(?=\s*=))|(\b\d+\b|\btrue\b|\bfalse\b)/gm
      : /(\/\/.*$)|('(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`|"(?:[^"\\]|\\.)*")|(\b(?:import|from|const)\b)|(\b(?:suite|step|goal|expect|capture)\b)|(\b\d+\b)/gm;
    let last = 0, m;
    while ((m = re.exec(src))) {
      out.push(esc(src.slice(last, m.index)));
      const cls = lang === 'toml'
        ? ['c', 's', 'h', 'k', 'n'][[1, 2, 3, 4, 5].find((i) => m[i] !== undefined) - 1]
        : ['c', 's', 'kw', 'f', 'n'][[1, 2, 3, 4, 5].find((i) => m[i] !== undefined) - 1];
      out.push(`<span class="${cls}">${esc(m[0])}</span>`);
      last = re.lastIndex;
    }
    out.push(esc(src.slice(last)));
    return out.join('');
  }
  const html = $derived(highlight(code.trim(), lang));
</script>

<div class="code">
  {#if title}<div class="title">{title}</div>{/if}
  <!-- html is produced from static, escaped source -->
  <pre>{@html html}</pre>
</div>

<style>
  .code { background: var(--panel); border: 1px solid var(--line); border-radius: 10px; overflow: hidden; }
  .title { font: 12px var(--mono); color: var(--muted); padding: 8px 16px; border-bottom: 1px solid var(--line); background: var(--panel2); }
  pre { margin: 0; padding: 16px 18px; overflow: auto; font: 13px/1.6 var(--mono); }
  pre :global(.c) { color: var(--muted); } pre :global(.s) { color: var(--str); } pre :global(.kw) { color: var(--kw); }
  pre :global(.f) { color: var(--violet); } pre :global(.n) { color: #79c0ff; } pre :global(.h) { color: var(--accent); font-weight: 600; }
  pre :global(.k) { color: #79c0ff; }
</style>
