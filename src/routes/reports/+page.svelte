<script lang="ts">
  import { onMount } from 'svelte';
  import { getSpendingByCategory, getMonthlySpendTrend, lastNMonths, formatMonthLabel } from '$lib/reports';
  import { formatCents } from '$lib/money';
  import type { CategorySpendReport } from '$lib/generated/CategorySpendReport';
  import type { MonthlySpendTrend } from '$lib/generated/MonthlySpendTrend';

  const months = lastNMonths(13);
  let selectedMonth = $state(months[0]);
  let categoryRows = $state<CategorySpendReport[]>([]);
  let trendRows = $state<MonthlySpendTrend[]>([]);
  let categoryLoading = $state(true);
  let trendLoading = $state(true);

  async function loadCategorySpend() {
    categoryLoading = true;
    try {
      categoryRows = await getSpendingByCategory(selectedMonth);
    } catch {
      // No Tauri backend in pnpm dev
    } finally {
      categoryLoading = false;
    }
  }

  async function loadTrend() {
    trendLoading = true;
    try {
      trendRows = await getMonthlySpendTrend(12);
    } catch {
      // No Tauri backend in pnpm dev
    } finally {
      trendLoading = false;
    }
  }

  onMount(() => {
    loadCategorySpend();
    loadTrend();
  });
</script>

<div class="page">
  <header class="page-header">
    <h1 class="heading">Reports</h1>
    <p class="subtitle">Spending by category and monthly trends</p>
  </header>

  <div class="reports-body">
    <section class="report-section">
      <div class="section-head">
        <h2 class="section-title">Spending by Category</h2>
        <select
          class="month-picker"
          bind:value={selectedMonth}
          onchange={loadCategorySpend}
        >
          {#each months as m (m)}
            <option value={m}>{formatMonthLabel(m)}</option>
          {/each}
        </select>
      </div>

      {#if categoryLoading}
        <div class="empty-state"><p>Loading...</p></div>
      {:else if categoryRows.length === 0}
        <div class="empty-state"><p class="muted">No spending recorded for {formatMonthLabel(selectedMonth)}.</p></div>
      {:else}
        <div class="table-wrap">
          <table class="data-table">
            <thead>
              <tr>
                <th>Category</th>
                <th>Type</th>
                <th class="amount-col">Spent</th>
              </tr>
            </thead>
            <tbody>
              {#each categoryRows as row (row.category_id)}
                <tr>
                  <td class="category-name">{row.category_name}</td>
                  <td><span class="kind-chip" class:sinking={row.kind === 'sinking'}>{row.kind}</span></td>
                  <td class="amount">{formatCents(-row.spent_cents)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>

    <section class="report-section">
      <h2 class="section-title">Monthly Trend</h2>

      {#if trendLoading}
        <div class="empty-state"><p>Loading...</p></div>
      {:else}
        <div class="table-wrap">
          <table class="data-table">
            <thead>
              <tr>
                <th>Month</th>
                <th class="amount-col">Total Spent</th>
              </tr>
            </thead>
            <tbody>
              {#each trendRows as row (row.month)}
                <tr>
                  <td class="month-label">{formatMonthLabel(row.month)}</td>
                  <td class="amount">{formatCents(-row.total_spent_cents)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
  </div>
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

  .reports-body {
    flex: 1;
    overflow: auto;
    padding: 28px 32px;
    display: flex;
    flex-direction: column;
    gap: 40px;
  }

  .report-section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--foreground);
    margin: 0;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .month-picker {
    background: var(--input);
    border: 1px solid var(--border);
    color: var(--foreground);
    font-size: 12px;
    font-family: var(--font-sans);
    padding: 5px 10px;
    outline: none;
    cursor: pointer;
  }

  .month-picker:focus {
    border-color: var(--primary);
  }

  .table-wrap {
    border: 1px solid var(--border);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }

  thead {
    background: var(--card);
  }

  th {
    padding: 8px 12px;
    text-align: left;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    border-bottom: 1px solid var(--border);
  }

  td {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text);
  }

  tr:last-child td {
    border-bottom: none;
  }

  .amount-col {
    text-align: right;
  }

  .amount {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    text-align: right;
    white-space: nowrap;
    color: var(--destructive);
  }

  .category-name {
    font-weight: 500;
    color: var(--foreground);
  }

  .month-label {
    font-family: var(--font-mono);
    color: var(--muted-foreground);
  }

  .kind-chip {
    display: inline-block;
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    background: var(--muted);
    color: var(--muted-foreground);
    padding: 2px 7px;
  }

  .kind-chip.sinking {
    background: var(--primary);
    color: var(--primary-foreground);
  }

  .muted {
    color: var(--muted-foreground);
    font-size: 13px;
    margin: 0;
  }
</style>
