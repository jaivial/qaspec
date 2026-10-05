<script>
  /**
   * Yes/no grid. rows: [label, ...values] where a value is true, false, or [bool, note].
   * Stays a real table on every width: three narrow columns fit on a phone.
   */
  let { caption, columns, rows, summary = 6 } = $props();
  let all = $state(false);
  const shown = $derived(all || rows.length <= summary + 1 ? rows : rows.slice(0, summary));
  // columns: [full, short] pairs or strings; the short label is used on phones.
  const col = (c) => (Array.isArray(c) ? c : [c, c]);
  const parts = (v) => (Array.isArray(v) ? v : [v, null]);
</script>

<div class="checks">
  <table>
    <caption class="sr-only">{caption}</caption>
    <thead>
      <tr>
        <th scope="col"><span class="sr-only">Capability</span></th>
        {#each columns as c, i}
          {@const [full, short] = col(c)}
          <th scope="col" class:ours={i === 0}><span class="full">{full}</span><span class="short" aria-hidden="true">{short}</span></th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each shown as [label, ...vals]}
        <tr>
          <th scope="row">{label}</th>
          {#each vals as v, i}
            {@const [yes, note] = parts(v)}
            <td class:ours={i === 0}>
              <span class="mark" class:yes title={note ?? ''}>
                {#if yes}
                  <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"><path d="M4.5 10.5l3.5 3.5 7.5-8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
                {:else}
                  <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"><path d="M6 6l8 8m0-8l-8 8" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round"/></svg>
                {/if}
                <span class="sr-only">{yes ? 'Yes' : 'No'}</span>
              </span>
              {#if note}<span class="note">{note}</span>{/if}
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
  {#if rows.length > summary + 1}
    <button type="button" class="more" aria-expanded={all} onclick={() => (all = !all)}>
      {all ? 'Show fewer rows' : `Show all ${rows.length} rows`}
    </button>
  {/if}
</div>

<style>
  .checks { border-radius: var(--r-md); box-shadow: 0 0 0 1px var(--border); background: var(--surface); overflow: clip; }
  .more { display: block; width: 100%; min-height: 44px; border: 0; border-top: 1px solid var(--border); background: var(--raised); color: var(--link); font: 500 14px var(--sans); cursor: pointer; }
  .more:hover { background: var(--overlay); }
  table { width: 100%; border-collapse: collapse; font-size: 14px; table-layout: fixed; }
  thead th { position: sticky; top: 56px; z-index: 2; background: var(--raised); color: var(--text); font-weight: 500; padding: 12px 10px; text-align: center; border-bottom: 1px solid var(--border); vertical-align: bottom; }
  thead th:first-child { width: 44%; text-align: left; }
  thead th.ours { color: var(--accent); }
  tbody th { text-align: left; font-weight: 400; color: var(--text); padding: 12px 14px; }
  td { text-align: center; padding: 10px 8px; vertical-align: middle; }
  td.ours { background: var(--hl); }
  tbody tr + tr > * { border-top: 1px solid var(--border); }
  .mark { display: inline-flex; color: var(--text-3); }
  .mark.yes { color: var(--accent); }
  .note { display: block; color: var(--text-3); font-size: 13px; line-height: 1.35; margin-top: 2px; }
  .short { display: none; }

  @media (max-width: 640px) {
    table { font-size: 14px; }
    thead th { padding: 10px 0; font-size: 12px; width: 56px; }
    thead th:first-child { width: auto; padding-left: 12px; }
    .full { display: none; }
    .short { display: inline; }
    tbody th { padding: 11px 6px 11px 12px; line-height: 1.4; }
    td { padding: 9px 2px; }
    .note { display: none; }
  }
</style>
