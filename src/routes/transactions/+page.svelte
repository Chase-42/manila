<script lang="ts">
  import { onMount } from 'svelte';
  import { listTransactions } from '$lib/transactions';
  import { formatCents } from '$lib/money';
  import type { TransactionRow } from '$lib/generated/TransactionRow';
  import TransactionDetail from '$lib/components/TransactionDetail.svelte';
  import { ArrowLeftRight } from '@lucide/svelte';

  let transactions = $state<TransactionRow[]>([]);
  let loading = $state(true);
  let selected = $state<TransactionRow | undefined>(undefined);
  let detailOpen = $state(false);

  async function load() {
    loading = true;
    try {
      transactions = await listTransactions();
    } catch {
      // No Tauri backend in pnpm dev
    } finally {
      loading = false;
    }
  }

  onMount(load);

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

{#if selected}
  <TransactionDetail
    transaction={selected}
    bind:open={detailOpen}
    {onSaved}
  />
{/if}

<div class="page">
  <header class="page-header">
    <h1 class="heading">Transactions</h1>
    {#if !loading}
      <p class="subtitle">{transactions.length} transaction{transactions.length === 1 ? '' : 's'}</p>
    {/if}
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
  {:else}
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
          {#each transactions as tx (tx.id)}
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
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .page-header {
    padding: 28px 32px 20px;
    background: var(--card);
    border-bottom: 1px solid var(--border);
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

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
  }

  .empty-icon {
    color: var(--muted-foreground);
    margin-bottom: 4px;
  }

  .empty-heading {
    font-size: 16px;
    font-weight: 600;
    color: var(--foreground);
    margin: 0;
  }

  .empty-body {
    font-size: 13px;
    color: var(--muted-foreground);
    margin: 0;
  }

  .cta-link {
    display: inline-block;
    margin-top: 8px;
    padding: 8px 18px;
    background: var(--primary);
    color: var(--primary-foreground);
    text-decoration: none;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.03em;
  }

  .cta-link:hover {
    opacity: 0.9;
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
</style>
