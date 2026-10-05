<script>
  /**
   * Comparison table. On narrow screens each row becomes a stacked block with the tool name
   * shown next to every value, so nothing needs horizontal scrolling.
   * cells: 'yes' | 'no' | 'partial' | any text. A cell can be [value, note].
   */
  let { caption, columns, rows, highlight = 0, versus = 1 } = $props();
  const kind = (v) => (v === 'yes' || v === 'no' || v === 'partial' ? v : 'text');
  const label = { yes: 'Yes', no: 'No', partial: 'Partly' };
</script>

{#snippet cell(c)}
  {@const [v, note] = Array.isArray(c) ? c : [c, null]}
  {#if kind(v) === 'text'}
    <span>{v}</span>
  {:else}
    <span class="mark {v}">
      {#if v === 'yes'}<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
      {:else if v === 'no'}<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M4.5 4.5l7 7m0-7l-7 7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
      {:else}<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M4 8h8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>{/if}
      {label[v]}
    </span>
  {/if}
  {#if note}<span class="note">{note}</span>{/if}
{/snippet}

<div class="cmp">
  <table>
    <caption class="sr-only">{caption}</caption>
    <thead>
      <tr><th scope="col"><span class="sr-only">Capability</span></th>{#each columns as c, i}<th scope="col" class:hl={i === highlight}>{c}</th>{/each}</tr>
    </thead>
    <tbody>
      {#each rows as [feature, ...cells]}
        <tr>
          <th scope="row">{feature}</th>
          {#each cells as c, i}<td class:hl={i === highlight} class:vs={i === versus} data-col={columns[i]}>{@render cell(c)}</td>{/each}
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .cmp { border-radius: var(--r-md); box-shadow: 0 0 0 1px var(--border); overflow: hidden; background: var(--surface); }
  table { width: 100%; border-collapse: collapse; font-size: 14px; }
  thead th { position: sticky; top: 0; background: var(--raised); color: var(--text); font-weight: 500; text-align: left; padding: 12px 14px; border-bottom: 1px solid var(--border); }
  thead th.hl { color: var(--accent); }
  tbody th { text-align: left; font-weight: 400; color: var(--text-2); padding: 12px 14px; width: 26%; }
  td { padding: 12px 14px; vertical-align: top; color: var(--text); }
  td.hl { background: rgb(255 255 255 / 0.025); }
  tbody tr + tr > * { border-top: 1px solid var(--border); }
  .mark { display: inline-flex; align-items: center; gap: 6px; font-weight: 500; }
  .mark svg { flex: none; }
  .mark.yes { color: var(--accent); } .mark.no { color: var(--text-3); } .mark.partial { color: var(--warn); }
  .note { display: block; color: var(--text-3); font-size: 13px; margin-top: 2px; }

  @media (max-width: 720px) {
    thead { display: none; }
    table, tbody, tr, th, td { display: block; width: 100%; }
    tbody tr { padding: 14px 16px; }
    tbody tr + tr > * { border-top: 0; }
    tbody tr + tr { border-top: 1px solid var(--border); }
    tbody th { padding: 0 0 8px; color: var(--text); font-weight: 500; width: auto; }
    td { display: none; grid-template-columns: 104px 1fr; gap: 10px; padding: 5px 0; }
    td.hl, td.vs { display: grid; }
    td::before { content: attr(data-col); color: var(--text-3); font-size: 13px; }
    td.hl { background: none; }
    td.hl::before { color: var(--accent); }
  }
</style>
