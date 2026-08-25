<script lang="ts">
  import { onMount } from 'svelte';
  import { getBudgetMonth, setAllocation, setMonthlyTarget } from '$lib/budget';
  import { formatCents } from '$lib/money';
  import type { BudgetMonthView, BudgetCategoryRow } from '$lib/budget';
  import { BarChart2 } from '@lucide/svelte';

  const now = new Date();
  const currentMonth = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`;
  const monthLabel = now.toLocaleDateString('en-US', { month: 'long', year: 'numeric' });

  let view = $state<BudgetMonthView | null>(null);
  let loading = $state(true);
  let targetInput = $state('');
  let allocationInputs = $state<Record<string, string>>({});

  let flowCategories = $derived(view?.categories.filter((c) => c.kind === 'flow') ?? []);
  let sinkingCategories = $derived(view?.categories.filter((c) => c.kind === 'sinking') ?? []);

  // Pure helpers - not state-coupled, no invoke calls.
  function parseCents(value: string): number {
    const n = parseFloat(value);
    return isNaN(n) || n < 0 ? 0 : Math.round(n * 100);
  }

  function formatAllocInput(cat: BudgetCategoryRow): [string, string] {
    return [cat.category_id, cat.allocated_cents > 0 ? (cat.allocated_cents / 100).toFixed(2) : ''];
  }

  async function load() {
    loading = true;
    try {
      view = await getBudgetMonth(currentMonth);
      targetInput = view.monthly_target_cents > 0
        ? (view.monthly_target_cents / 100).toFixed(2)
        : '';
      allocationInputs = Object.fromEntries(view.categories.map(formatAllocInput));
    } catch {
      // no Tauri backend in pnpm dev
    } finally {
      loading = false;
    }
  }

  onMount(load);

  async function commitTarget() {
    try {
      await setMonthlyTarget(currentMonth, parseCents(targetInput));
      await load();
    } catch {
      // no backend
    }
  }

  async function commitAllocation(categoryId: string) {
    const cents = parseCents(allocationInputs[categoryId] ?? '');
    try {
      await setAllocation(categoryId, currentMonth, cents);
      await load();
    } catch {
      // no backend
    }
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Enter') (e.currentTarget as HTMLElement).blur();
  }

  function progressPercent(cat: BudgetCategoryRow): number {
    if (cat.allocated_cents === 0) return cat.spent_cents > 0 ? 100 : 0;
    return Math.min(100, Math.round((cat.spent_cents / cat.allocated_cents) * 100));
  }

  function isOverspent(cat: BudgetCategoryRow): boolean {
    return cat.spent_cents > cat.allocated_cents;
  }

  function leftToAllocateLabel(cents: number): string {
    if (cents >= 0) return `${formatCents(cents)} left to allocate`;
    return `${formatCents(Math.abs(cents))} over budget`;
  }
</script>

{#snippet flowRow(cat: BudgetCategoryRow)}
  <div class="category-row">
    <div class="row-main">
      <span class="cat-name">{cat.category_name}</span>
      <div class="progress-track">
        <div
          class="progress-fill"
          class:overspent={isOverspent(cat)}
          style="width: {progressPercent(cat)}%"
        ></div>
      </div>
    </div>
    <div class="row-amounts">
      <span class="spent-amount">{formatCents(cat.spent_cents)}</span>
      <span class="divider">/</span>
      <input
        class="alloc-input"
        type="text"
        inputmode="decimal"
        placeholder="0"
        value={allocationInputs[cat.category_id] ?? ''}
        oninput={(e) => { allocationInputs[cat.category_id] = (e.currentTarget as HTMLInputElement).value; }}
        onblur={() => commitAllocation(cat.category_id)}
        onkeydown={handleKey}
        aria-label={`Allocation for ${cat.category_name}`}
      />
    </div>
  </div>
{/snippet}

{#snippet sinkingRow(cat: BudgetCategoryRow)}
  {@const balance = cat.allocated_cents - cat.spent_cents}
  <div class="sinking-row">
    <div class="sinking-left">
      <span class="cat-name">{cat.category_name}</span>
      <span class="sinking-balance" class:positive={balance > 0} class:zero={balance === 0}>
        {formatCents(balance)}
      </span>
    </div>
    <div class="row-amounts">
      <span class="sinking-label">this month</span>
      <input
        class="alloc-input"
        type="text"
        inputmode="decimal"
        placeholder="0"
        value={allocationInputs[cat.category_id] ?? ''}
        oninput={(e) => { allocationInputs[cat.category_id] = (e.currentTarget as HTMLInputElement).value; }}
        onblur={() => commitAllocation(cat.category_id)}
        onkeydown={handleKey}
        aria-label={`Monthly contribution for ${cat.category_name}`}
      />
    </div>
  </div>
{/snippet}

{#snippet pageContent()}
  <div class="content">
    {#if flowCategories.length > 0}
      <section class="section">
        <h2 class="section-title">Monthly spending</h2>
        <div class="category-list">
          {#each flowCategories as cat (cat.category_id)}
            {@render flowRow(cat)}
          {/each}
        </div>
      </section>
    {/if}

    {#if sinkingCategories.length > 0}
      <section class="section">
        <h2 class="section-title">Sinking funds</h2>
        <div class="category-list">
          {#each sinkingCategories as cat (cat.category_id)}
            {@render sinkingRow(cat)}
          {/each}
        </div>
      </section>
    {/if}
  </div>
{/snippet}

<div class="page">
  <header class="page-header">
    <div class="header-top">
      <h1 class="month">{monthLabel}</h1>
      <div class="target-field">
        <span class="target-label">Budget</span>
        <span class="target-prefix">$</span>
        <input
          class="target-input"
          type="text"
          inputmode="decimal"
          placeholder="0"
          bind:value={targetInput}
          onblur={commitTarget}
          onkeydown={handleKey}
          aria-label="Monthly budget target"
        />
      </div>
    </div>
    {#if view && view.monthly_target_cents > 0}
      <div class="header-meta">
        <span class="lta-chip" class:over={view.left_to_allocate_cents < 0}>
          {leftToAllocateLabel(view.left_to_allocate_cents)}
        </span>
      </div>
    {/if}
  </header>

  {#if loading}
    <div class="empty-state">
      <p class="muted">Loading...</p>
    </div>
  {:else if !view || view.categories.length === 0}
    <div class="empty-state">
      <div class="empty-icon"><BarChart2 size={36} /></div>
      <h2 class="empty-heading">No categories yet</h2>
      <p class="empty-body">Add categories to start budgeting.</p>
      <a href="/categories" class="cta-link">Manage categories</a>
    </div>
  {:else}
    {@render pageContent()}
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .page-header {
    padding: 24px 32px 18px;
    border-bottom: 1px solid var(--border);
    background: var(--card);
    flex-shrink: 0;
  }

  .header-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .month {
    font-size: 22px;
    font-weight: 700;
    color: var(--foreground);
    margin: 0;
    letter-spacing: 0.02em;
  }

  .target-field {
    display: flex;
    align-items: center;
    gap: 4px;
    border: 1px solid var(--border);
    padding: 4px 10px;
    background: var(--background);
  }

  .target-label {
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    margin-right: 4px;
  }

  .target-prefix {
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--muted-foreground);
  }

  .target-input {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 14px;
    font-weight: 600;
    color: var(--foreground);
    background: transparent;
    border: none;
    outline: none;
    width: 90px;
    text-align: right;
  }

  .target-input::placeholder {
    color: var(--muted-foreground);
    font-weight: 400;
  }

  .header-meta {
    margin-top: 10px;
  }

  .lta-chip {
    display: inline-block;
    font-size: 12px;
    font-weight: 500;
    color: var(--primary);
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }

  .lta-chip.over {
    color: var(--destructive);
  }

  .muted {
    font-size: 13px;
    color: var(--muted-foreground);
    margin: 0;
  }

  .content {
    flex: 1;
    overflow: auto;
  }

  .section {
    padding: 20px 32px;
    border-bottom: 1px solid var(--border);
  }

  .section:last-child {
    border-bottom: none;
  }

  .section-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted-foreground);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    margin: 0 0 12px;
  }

  .category-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  /* Flow rows */

  .category-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .row-main {
    flex: 1;
    min-width: 0;
  }

  .cat-name {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--foreground);
    margin-bottom: 5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .progress-track {
    height: 4px;
    background: var(--muted);
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--primary);
    transition: width 0.2s ease;
  }

  .progress-fill.overspent {
    background: var(--destructive);
  }

  .row-amounts {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .spent-amount {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
    color: var(--muted-foreground);
    white-space: nowrap;
  }

  .divider {
    color: var(--muted-foreground);
    font-size: 12px;
  }

  .alloc-input {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 13px;
    font-weight: 600;
    color: var(--foreground);
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--border);
    outline: none;
    width: 72px;
    text-align: right;
    padding: 1px 0;
  }

  .alloc-input:focus {
    border-bottom-color: var(--primary);
  }

  .alloc-input::placeholder {
    color: var(--muted-foreground);
    font-weight: 400;
  }

  /* Sinking rows */

  .sinking-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .sinking-left {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .sinking-balance {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
    font-weight: 600;
    color: var(--muted-foreground);
    flex-shrink: 0;
  }

  .sinking-balance.positive {
    color: var(--primary);
  }

  .sinking-balance.zero {
    color: var(--muted-foreground);
  }

  .sinking-label {
    font-size: 11px;
    color: var(--muted-foreground);
    white-space: nowrap;
  }
</style>
