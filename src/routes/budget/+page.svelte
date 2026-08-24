<script lang="ts">
  import { onMount } from 'svelte';
  import { listAccounts } from '$lib/accounts';
  import { listTransactions } from '$lib/transactions';
  import { formatCents } from '$lib/money';
  import type { Account } from '$lib/types/account';
  import type { TransactionRow } from '$lib/types/transaction';
  import { Wallet } from '@lucide/svelte';

  const now = new Date();
  const currentMonth = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`;
  const monthLabel = now.toLocaleDateString('en-US', { month: 'long', year: 'numeric' });

  let accounts = $state<Account[]>([]);
  let transactions = $state<TransactionRow[]>([]);
  let loading = $state(true);

  let thisMonthTxns = $derived(transactions.filter((t) => t.date.startsWith(currentMonth)));
  let totalSpentCents = $derived(
    thisMonthTxns
      .filter((t) => t.amount_cents < 0)
      .reduce((sum, t) => sum + Math.abs(t.amount_cents), 0)
  );

  onMount(async () => {
    try {
      [accounts, transactions] = await Promise.all([listAccounts(), listTransactions()]);
    } catch {
      // No Tauri backend in pnpm dev
    } finally {
      loading = false;
    }
  });
</script>

<div class="page">
  <header class="page-header">
    <h1 class="month">{monthLabel}</h1>
    {#if !loading && thisMonthTxns.length > 0}
      <p class="summary">
        <span class="summary-count">{thisMonthTxns.length} transaction{thisMonthTxns.length === 1 ? '' : 's'}</span>
        <span class="dot">·</span>
        <span class="summary-amount">{formatCents(totalSpentCents)}</span>
        <span class="summary-label">spent this month</span>
      </p>
    {/if}
  </header>

  {#if loading}
    <div class="empty-state">
      <p class="loading-text">Loading...</p>
    </div>
  {:else if accounts.length === 0}
    <div class="empty-state">
      <div class="empty-icon">
        <Wallet size={36} />
      </div>
      <h2 class="empty-heading">No accounts yet</h2>
      <p class="empty-body">Add an account and import transactions to get started.</p>
      <a href="/accounts" class="cta-link">Set up accounts</a>
    </div>
  {:else}
    <div class="content">
      <section class="section">
        <h2 class="section-title">Accounts</h2>
        <div class="account-grid">
          {#each accounts as account (account.id)}
            <div class="account-card">
              <div class="card-main">
                <span class="account-name">{account.name}</span>
                <span class="account-institution">{account.institution}</span>
              </div>
              <div class="card-footer">
                <span class="type-chip">{account.subtype || account.account_type}</span>
                <span class="currency">{account.currency}</span>
              </div>
            </div>
          {/each}
        </div>
      </section>
    </div>
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: auto;
  }

  .page-header {
    padding: 28px 32px 20px;
    border-bottom: 1px solid var(--border);
    background: var(--card);
  }

  .month {
    font-size: 24px;
    font-weight: 700;
    color: var(--foreground);
    margin: 0 0 6px;
    letter-spacing: 0.02em;
  }

  .summary {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: 12px;
  }

  .summary-count,
  .dot,
  .summary-label {
    color: var(--muted-foreground);
  }

  .summary-amount {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    color: var(--destructive);
    font-weight: 500;
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

  .empty-body,
  .loading-text {
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

  .content {
    flex: 1;
    overflow: auto;
  }

  .section {
    padding: 24px 32px;
  }

  .section-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted-foreground);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    margin: 0 0 14px;
  }

  .account-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 12px;
  }

  .account-card {
    background: var(--card);
    border: 1px solid var(--border);
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .card-main {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .account-name {
    font-size: 14px;
    font-weight: 600;
    color: var(--foreground);
  }

  .account-institution {
    font-size: 12px;
    color: var(--muted-foreground);
  }

  .card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .type-chip {
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    background: var(--muted);
    color: var(--muted-foreground);
    padding: 2px 7px;
  }

  .currency {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--muted-foreground);
  }
</style>
