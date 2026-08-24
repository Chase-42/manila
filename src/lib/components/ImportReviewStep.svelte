<script lang="ts">
  import type { UncertainMatch } from "$lib/types/import";
  import { formatCents } from "$lib/money";

  let {
    uncertain,
    decisions = $bindable(new Map<string, boolean>()),
  }: {
    uncertain: UncertainMatch[];
    decisions?: Map<string, boolean>;
  } = $props();

  function toggle(sourceId: string) {
    const next = new Map(decisions);
    next.set(sourceId, !(next.get(sourceId) ?? false));
    decisions = next;
  }

  function isDuplicate(sourceId: string): boolean {
    return decisions.get(sourceId) ?? false;
  }

  // Strip the format prefix (e.g. "csv|acct-id|" or "ofx|acct-id|") for display.
  function hintFromSourceId(sourceId: string): string {
    const parts = sourceId.split("|");
    return parts.length >= 3 ? parts.slice(2).join("|") : sourceId;
  }
</script>

<div class="review-wrapper">
  <p class="review-hint">
    {uncertain.length} possible {uncertain.length === 1 ? "duplicate" : "duplicates"} found.
    Review each match and mark any that are true duplicates.
  </p>

  <div class="table-scroll">
    <table>
      <thead>
        <tr class="col-names">
          <th>Date</th>
          <th>Description</th>
          <th class="right">Amount</th>
          <th>Matched existing</th>
          <th class="center">Decision</th>
        </tr>
      </thead>
      <tbody>
        {#each uncertain as match}
          {@const dup = isDuplicate(match.candidate_source_id)}
          <tr class:dup>
            <td class="mono">{match.candidate_date}</td>
            <td>{match.candidate_description}</td>
            <td class="mono right">{formatCents(match.candidate_amount_cents)}</td>
            <td class="hint">{hintFromSourceId(match.existing_source_id)}</td>
            <td class="center">
              <button
                class="toggle"
                class:active={dup}
                onclick={() => toggle(match.candidate_source_id)}
              >
                {dup ? "Duplicate" : "New"}
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<style>
  .review-wrapper {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .review-hint {
    font-size: 12px;
    color: var(--muted-foreground);
    margin: 0;
  }

  .table-scroll {
    overflow-x: auto;
    border: 1px solid var(--border);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 11px;
  }

  .col-names th {
    padding: 5px 8px;
    background: var(--surface-raised);
    border-bottom: 1px solid var(--border);
    color: var(--muted-foreground);
    font-weight: 500;
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .col-names th.right {
    text-align: right;
  }

  .col-names th.center {
    text-align: center;
  }

  tbody tr:hover td {
    background: var(--surface-hover);
  }

  tbody tr.dup td {
    color: var(--muted-foreground);
  }

  tbody td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text);
    white-space: nowrap;
  }

  tbody td.mono {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }

  tbody td.right {
    text-align: right;
  }

  tbody td.center {
    text-align: center;
  }

  tbody td.hint {
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    font-size: 10px;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .toggle {
    background: none;
    border: 1px solid var(--border);
    color: var(--muted-foreground);
    font-size: 10px;
    font-family: var(--font-display);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    padding: 3px 8px;
    cursor: pointer;
    white-space: nowrap;
  }

  .toggle:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .toggle.active {
    border-color: var(--destructive);
    color: var(--destructive);
  }
</style>
