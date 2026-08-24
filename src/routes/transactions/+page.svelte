<script lang="ts">
  import { onMount } from 'svelte';
  import { listTransactions } from '$lib/transactions';
  import { formatCents } from '$lib/money';
  import type { TransactionRow } from '$lib/types/transaction';
  import TransactionDetail from '$lib/components/TransactionDetail.svelte';

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
    <h1>Transactions</h1>
  </header>

  {#if loading}
    <div class="empty-state">
      <p>Loading...</p>
    </div>
  {:else if transactions.length === 0}
    <div class="empty-state">
      <p>No transactions yet. Import a file from Accounts to get started.</p>
    </div>
  {:else}
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Date</th>
            <th>Account</th>
            <th>Description</th>
            <th class="amount-col">Amount</th>
            <th class="reviewed-col">Reviewed</th>
          </tr>
        </thead>
        <tbody>
          {#each transactions as tx (tx.id)}
            <tr onclick={() => openDetail(tx)}>
              <td class="date">{tx.date}</td>
              <td class="account">{tx.account_name}</td>
              <td class="description">{tx.description}</td>
              <td class="amount" class:negative={tx.amount_cents < 0}>
                {formatCents(tx.amount_cents)}
              </td>
              <td class="reviewed-cell">
                {#if tx.reviewed}
                  <span class="reviewed-dot" aria-label="Reviewed"></span>
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
    display: flex;
    align-items: center;
    padding: 16px 24px;
    background: var(--surface-raised);
    border-bottom: 1px solid var(--border);
  }

  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    letter-spacing: 0.04em;
  }

  .empty-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .empty-state p {
    color: var(--faint);
    font-size: 13px;
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
    background: var(--surface-raised);
    z-index: 1;
  }

  th {
    padding: 8px 12px;
    text-align: left;
    color: var(--muted-foreground);
    font-weight: 500;
    font-size: 11px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    border-bottom: 1px solid var(--border);
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

  .date {
    font-family: var(--font-mono);
    color: var(--muted-foreground);
    white-space: nowrap;
  }

  .account {
    color: var(--muted-foreground);
    font-size: 11px;
  }

  .description {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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

  .reviewed-col {
    text-align: center;
    width: 72px;
  }

  .reviewed-cell {
    text-align: center;
  }

  .reviewed-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    background: var(--accent);
  }
</style>
