<script lang="ts">
  import { onMount } from 'svelte';
  import { listTransactions, searchTransactions, exportTransactionsCsv } from '$lib/transactions';
  import { Button } from '$lib/components/ui/button';
  import { formatCents } from '$lib/money';
  import type { TransactionRow } from '$lib/generated/TransactionRow';
  import TransactionDetail from '$lib/components/TransactionDetail.svelte';
  import { ArrowLeftRight } from '@lucide/svelte';

  let transactions = $state<TransactionRow[]>([]);
  let displayTransactions = $state<TransactionRow[]>([]);
  let loading = $state(true);
  let selected = $state<TransactionRow | undefined>(undefined);
  let detailOpen = $state(false);
  let query = $state('');
  let exportStatus = $state<string | null>(null);
  let exportTimer: ReturnType<typeof setTimeout> | undefined;

  async function handleExport() {
    try {
      exportStatus = `Saved: ${await exportTransactionsCsv()}`;
    } catch (e) {
      exportStatus = e instanceof Error ? e.message : String(e);
    }
    clearTimeout(exportTimer);
    exportTimer = setTimeout(() => {
      exportStatus = null;
    }, 4000);
  }

  async function load() {
    loading = true;
    try {
      transactions = await listTransactions();
      displayTransactions = transactions;
    } catch {
      // No Tauri backend in pnpm dev
    } finally {
      loading = false;
    }
  }

  onMount(load);

  $effect(() => {
    const q = query.trim();
    const t = setTimeout(async () => {
      if (q === '') {
        displayTransactions = transactions;
      } else {
        try {
          displayTransactions = await searchTransactions(q);
        } catch {
          // No Tauri backend in pnpm dev
        }
      }
    }, 300);
    return () => clearTimeout(t);
  });

  function openDetail(tx: TransactionRow) {
    selected = tx;
    detailOpen = true;
  }

  async function onSaved() {
    detailOpen = false;
    await load();
  }

  function formatDate(iso: string): string {
    const [year, month, day] = iso.split('-').map(Number);
    return new Date(year, month - 1, day).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
    });
  }

  function categoryLabel(name: string | null): string {
    return name ?? 'Uncategorized';
  }

  function categoryAssigned(name: string | null): boolean {
    return name !== null;
  }
</script>

{#snippet subtitle()}
  {#if !loading}
    {#if query.trim()}
      <p class="subtitle">{displayTransactions.length} result{displayTransactions.length === 1 ? '' : 's'} for "{query}"</p>
    {:else}
      <p class="subtitle">{transactions.length} transaction{transactions.length === 1 ? '' : 's'}</p>
    {/if}
  {/if}
{/snippet}

{#snippet table()}
  <div class="table-wrap">
    <table class="data-table">
      <thead>
        <tr>
          <th class="date-col">Date</th>
          <th>Merchant</th>
          <th>Category</th>
          <th class="amount-col">Amount</th>
          <th class="reviewed-col">Reviewed</th>
        </tr>
      </thead>
      <tbody>
        {#each displayTransactions as tx (tx.id)}
          <tr onclick={() => openDetail(tx)}>
            <td class="date">{formatDate(tx.date)}</td>
            <td class="merchant">
              <span class="merchant-name">{tx.description}</span>
              <span class="merchant-account">{tx.account_name}</span>
            </td>
            <td class="category-cell">
              <span class="category-chip" class:assigned={categoryAssigned(tx.category_name)}>
                {categoryLabel(tx.category_name)}
              </span>
            </td>
            <td
              class="amount"
              class:negative={tx.amount_cents < 0}
              class:positive={tx.amount_cents > 0}
            >
              {tx.amount_cents > 0 ? '+' : ''}{formatCents(tx.amount_cents)}
            </td>
            <td class="reviewed-cell">
              {#if tx.reviewed}
                <span class="reviewed-badge">&#10003;</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/snippet}

{#if selected}
  <TransactionDetail
    transaction={selected}
    bind:open={detailOpen}
    {onSaved}
  />
{/if}

<div class="page">
  <header class="page-header">
    <div class="header-top">
      <div>
        <h1 class="heading">Transactions</h1>
        {@render subtitle()}
      </div>
      <div class="header-actions">
        <Button variant="outline" size="sm" onclick={handleExport}>Export CSV</Button>
        {#if exportStatus}
          <span class="export-status">{exportStatus}</span>
        {/if}
      </div>
    </div>
    <input
      type="search"
      class="search-input"
      placeholder="Search transactions..."
      bind:value={query}
    />
  </header>

  {#if loading}
    <div class="empty-state">
      <p>Loading...</p>
    </div>
  {:else if transactions.length === 0}
    <div class="empty-state">
      <div class="empty-icon">
        <ArrowLeftRight size={36} />
      </div>
      <h2 class="empty-heading">No transactions yet</h2>
      <p class="empty-body">Import a file from an account to get started.</p>
      <a href="/accounts" class="cta-link">Go to Accounts</a>
    </div>
  {:else if displayTransactions.length === 0}
    <div class="empty-state">
      <p class="no-results">No results for "<span class="query-text">{query}</span>"</p>
    </div>
  {:else}
    {@render table()}
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .page-header {
    padding: 28px 32px 16px;
    background: var(--card);
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .header-top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
  }

  .heading {
    font-size: 24px;
    font-weight: 700;
    color: var(--foreground);
    margin: 0 0 6px;
    letter-spacing: 0.02em;
  }

  .subtitle {
    margin: 0;
    font-size: 12px;
    color: var(--muted-foreground);
  }

  .search-input {
    width: 100%;
    padding: 8px 12px;
    background: var(--input);
    border: 1px solid var(--border);
    color: var(--foreground);
    font-size: 13px;
    font-family: var(--font-sans);
    outline: none;
  }

  .search-input::placeholder {
    color: var(--muted-foreground);
  }

  .search-input:focus {
    border-color: var(--primary);
  }

  .table-wrap {
    flex: 1;
    overflow: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }

  thead {
    position: sticky;
    top: 0;
    background: var(--card);
    z-index: 1;
  }

  td {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text);
  }

  tr:hover td {
    background: var(--surface-hover);
    cursor: pointer;
  }

  .date-col {
    width: 72px;
  }

  .date {
    font-family: var(--font-mono);
    color: var(--muted-foreground);
    white-space: nowrap;
    font-size: 12px;
  }

  .merchant {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-width: 360px;
  }

  .merchant-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .merchant-account {
    font-size: 11px;
    color: var(--muted-foreground);
  }

  .category-chip {
    display: inline-block;
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    background: var(--muted);
    color: var(--muted-foreground);
    padding: 2px 7px;
  }

  .category-chip.assigned {
    background: var(--primary);
    color: var(--primary-foreground);
  }

  .amount-col {
    text-align: right;
  }

  .amount {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    text-align: right;
    white-space: nowrap;
  }

  .amount.negative {
    color: var(--destructive);
  }

  .amount.positive {
    color: var(--primary);
  }

  .reviewed-col {
    text-align: center;
    width: 72px;
  }

  .reviewed-cell {
    text-align: center;
  }

  .reviewed-badge {
    display: inline-block;
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.06em;
    background: var(--muted);
    color: var(--muted-foreground);
    padding: 2px 7px;
  }

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--muted-foreground);
  }

  .empty-icon {
    color: var(--muted-foreground);
    opacity: 0.4;
  }

  .empty-body {
    margin: 0;
    font-size: 13px;
  }

  .cta-link {
    margin-top: 4px;
    font-size: 13px;
    color: var(--primary);
    text-decoration: none;
  }

  .cta-link:hover {
    text-decoration: underline;
  }

  .no-results {
    font-size: 14px;
    margin: 0;
  }

  .query-text {
    color: var(--foreground);
    font-weight: 500;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }

  .export-status {
    font-size: 12px;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
  }
</style>
