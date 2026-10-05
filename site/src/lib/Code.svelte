<script>
  /** Tiny highlighter for qaspec specs (TS subset) and TOML. */
  let { code, lang = 'ts', title = null, wrap = false } = $props();
  let copied = $state(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(code.trim());
      copied = true;
      setTimeout(() => (copied = false), 1600);
    } catch {}
  }

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
  <div class="title">
    <span>{title ?? lang}</span>
    <button type="button" class="copy" onclick={copy} aria-live="polite">{copied ? 'Copied' : 'Copy'}</button>
  </div>
  <!-- html is produced from static, escaped source -->
  <pre class:wrap>{@html html}</pre>
</div>

<style>
  .code { background: var(--surface); border-radius: var(--r-md); overflow: hidden; box-shadow: 0 0 0 1px var(--border); min-width: 0; }
  .title { font: 12px var(--mono); color: var(--text-3); padding: 4px 8px 4px 16px; border-bottom: 1px solid var(--border); background: var(--raised); display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 40px; }
  .copy { font: 12px var(--sans); color: var(--text-2); background: transparent; border: 0; border-radius: var(--r-sm); min-height: 36px; padding: 0 12px; cursor: pointer; }
  .copy:hover { background: var(--overlay); color: var(--text); }
  pre.wrap { overflow-x: hidden; white-space: pre-wrap; overflow-wrap: anywhere; }
  pre { margin: 0; padding: 14px 16px; overflow: auto; font: 12.5px/1.65 var(--mono); -webkit-overflow-scrolling: touch; }
  @media (max-width: 640px) { pre { font-size: 11.5px; padding: 12px; } }
  pre :global(.c) { color: var(--text-3); } pre :global(.s) { color: var(--code-str); } pre :global(.kw) { color: var(--code-kw); }
  pre :global(.f) { color: var(--code-fn); } pre :global(.n) { color: var(--code-num); } pre :global(.h) { color: var(--accent); }
  pre :global(.k) { color: var(--code-num); }
</style>
