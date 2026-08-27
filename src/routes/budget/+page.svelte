<script lang="ts">
  import { onMount } from 'svelte';
  import { ChevronRight, ChevronDown, ArrowLeftRight } from '@lucide/svelte';
  import { getBudgetMonth, setAllocation } from '$lib/budget';
  import { formatCents } from '$lib/money';
  import type { BudgetMonthView, BudgetGroupView, BudgetCategoryRow } from '$lib/budget';
  import ReallocateDialog from '$lib/components/ReallocateDialog.svelte';

  const today = new Date();
  const CURRENT_MONTH = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}`;

  let selectedMonth = $state(CURRENT_MONTH);
  let view = $state<BudgetMonthView | null>(null);
  let loading = $state(false);
  let allocationInputs = $state<Record<string, string>>({});
  let collapsedGroups = $state(new Set<string>());

  let reallocOpen = $state(false);
  let reallocInitialFrom = $state<string | undefined>(undefined);
  let reallocInitialTo = $state<string | undefined>(undefined);

  function toMonthLabel(m: string): string {
    const [y, mo] = m.split('-').map(Number);
    return new Date(y, mo - 1, 1).toLocaleDateString('en-US', { month: 'long', year: 'numeric' });
  }

  function shiftMonth(m: string, delta: number): string {
    const [y, mo] = m.split('-').map(Number);
    const d = new Date(y, mo - 1 + delta, 1);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`;
  }

  function buildInputMap(v: BudgetMonthView): Record<string, string> {
    const cats = [
      ...v.flow_groups.flatMap(g => g.categories),
      ...v.flow_ungrouped,
      ...v.sinking_groups.flatMap(g => g.categories),
      ...v.sinking_ungrouped,
    ];
    return Object.fromEntries(
      cats.map(c => [c.category_id, c.allocated_cents > 0 ? (c.allocated_cents / 100).toFixed(2) : ''])
    );
  }

  async function load() {
    loading = true;
    try {
      view = await getBudgetMonth(selectedMonth);
      allocationInputs = buildInputMap(view);
    } catch { /* no Tauri in pnpm dev */ }
    finally { loading = false; }
  }

  onMount(load);

  function navigate(delta: number) {
    selectedMonth = shiftMonth(selectedMonth, delta);
    void load();
  }

  function toggleGroup(id: string) {
    const next = new Set(collapsedGroups);
    if (next.has(id)) { next.delete(id); } else { next.add(id); }
    collapsedGroups = next;
  }

  function parseCents(val: string): number {
    const n = parseFloat(val);
    return isNaN(n) || n < 0 ? 0 : Math.round(n * 100);
  }

  async function saveAllocation(categoryId: string) {
    const cents = parseCents(allocationInputs[categoryId] ?? '');
    try {
      await setAllocation(categoryId, selectedMonth, cents);
      await load();
    } catch { /* no backend */ }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') (e.currentTarget as HTMLElement).blur();
  }

  function openReallocFrom(categoryId: string) {
    reallocInitialFrom = categoryId;
    reallocInitialTo = undefined;
    reallocOpen = true;
  }

  function openReallocTo(categoryId: string) {
    reallocInitialFrom = undefined;
    reallocInitialTo = categoryId;
    reallocOpen = true;
  }

  let totalIncome = $derived(view?.income_rows.reduce((s, r) => s + r.actual_cents, 0) ?? 0);

  let totalSpent = $derived(
    (view?.flow_groups.reduce((s, g) => s + g.total_spent_cents, 0) ?? 0) +
    (view?.flow_ungrouped.reduce((s, c) => s + c.spent_cents, 0) ?? 0) +
    (view?.sinking_groups.reduce((s, g) => s + g.total_spent_cents, 0) ?? 0) +
    (view?.sinking_ungrouped.reduce((s, c) => s + c.spent_cents, 0) ?? 0)
  );

  let sinkingAll = $derived([
    ...(view?.sinking_groups.flatMap(g => g.categories) ?? []),
    ...(view?.sinking_ungrouped ?? []),
  ]);

  let allExpenseCategories = $derived([
    ...(view?.flow_groups.flatMap(g => g.categories) ?? []),
    ...(view?.flow_ungrouped ?? []),
    ...(view?.sinking_groups.flatMap(g => g.categories) ?? []),
    ...(view?.sinking_ungrouped ?? []),
  ]);

  let barMax = $derived(Math.max(totalIncome, totalSpent, 1));
</script>

{#snippet expenseCatRow(cat: BudgetCategoryRow)}
  {@const remaining = cat.allocated_cents - cat.spent_cents}
  <div class="expense-row">
    <span class="cat-name cat-indent">{cat.category_name}</span>
    <input
      class="alloc-input"
      type="text"
      inputmode="decimal"
      placeholder="0"
      value={allocationInputs[cat.category_id] ?? ''}
      oninput={(e) => { allocationInputs[cat.category_id] = (e.currentTarget as HTMLInputElement).value; }}
      onblur={() => saveAllocation(cat.category_id)}
      onkeydown={onKey}
      aria-label={`Budget for ${cat.category_name}`}
    />
    <span class="mono-cell">{formatCents(cat.spent_cents)}</span>
    <div class="remaining-cell">
      <span class="mono-cell" class:positive={remaining >= 0} class:negative={remaining < 0}>
        {formatCents(remaining)}
      </span>
      {#if cat.kind === 'flow' && remaining < 0}
        <button class="cover-btn" onclick={() => openReallocTo(cat.category_id)} title="Cover overspend">Cover?</button>
      {/if}
    </div>
    <button class="move-btn" onclick={() => openReallocFrom(cat.category_id)} title="Move money from this category">
      <ArrowLeftRight size={13} />
    </button>
  </div>
{/snippet}

{#snippet expenseGroupBlock(group: BudgetGroupView)}
  {@const collapsed = collapsedGroups.has(group.group_id)}
  <div class="group-block">
    <button class="expense-row group-btn" onclick={() => toggleGroup(group.group_id)}>
      <span class="group-label">
        {#if collapsed}<ChevronRight size={13} />{:else}<ChevronDown size={13} />{/if}
        {group.group_name}
      </span>
      <span class="mono-cell muted">{formatCents(group.total_allocated_cents)}</span>
      <span class="mono-cell muted">{formatCents(group.total_spent_cents)}</span>
      <span class="mono-cell" class:positive={group.remaining_cents >= 0} class:negative={group.remaining_cents < 0}>
        {formatCents(group.remaining_cents)}
      </span>
      <span></span>
    </button>
    {#if !collapsed}
      {#each group.categories as cat (cat.category_id)}
        {@render expenseCatRow(cat)}
      {/each}
    {/if}
  </div>
{/snippet}

{#snippet sinkingCatRow(cat: BudgetCategoryRow)}
  {@const balance = cat.allocated_cents - cat.spent_cents}
  <div class="sinking-row">
    <span class="cat-name">{cat.category_name}</span>
    <span class="mono-cell" class:positive={balance > 0}>{formatCents(balance)}</span>
    <input
      class="alloc-input"
      type="text"
      inputmode="decimal"
      placeholder="0"
      value={allocationInputs[cat.category_id] ?? ''}
      oninput={(e) => { allocationInputs[cat.category_id] = (e.currentTarget as HTMLInputElement).value; }}
      onblur={() => saveAllocation(cat.category_id)}
      onkeydown={onKey}
      aria-label={`Monthly contribution for ${cat.category_name}`}
    />
    <button class="move-btn move-btn-sinking" onclick={() => openReallocFrom(cat.category_id)} title="Move money from this category">
      <ArrowLeftRight size={13} />
    </button>
  </div>
{/snippet}

{#snippet incomeSection()}
  <section class="section">
    <div class="col-header income-grid">
      <span class="section-label">Income</span>
      <span class="col-label-right">Actual</span>
    </div>
    {#each view!.income_rows as row (row.income_category_id)}
      <div class="income-row">
        <span class="cat-name">{row.name}</span>
        <span class="mono-cell">{formatCents(row.actual_cents)}</span>
      </div>
    {/each}
  </section>
{/snippet}

{#snippet expenseSection()}
  <section class="section">
    <div class="col-header expense-grid">
      <span class="section-label">Expenses</span>
      <span class="col-label-right">Budget</span>
      <span class="col-label-right">Actual</span>
      <span class="col-label-right">Remaining</span>
      <span></span>
    </div>
    {#each view!.flow_groups as group (group.group_id)}
      {@render expenseGroupBlock(group)}
    {/each}
    {#if view!.flow_ungrouped.length > 0}
      <div class="group-block">
        <div class="expense-row group-ungrouped">
          <span class="group-label-plain">Other</span>
          <span></span><span></span><span></span><span></span>
        </div>
        {#each view!.flow_ungrouped as cat (cat.category_id)}
          {@render expenseCatRow(cat)}
        {/each}
      </div>
    {/if}
  </section>
{/snippet}

{#snippet sinkingSection()}
  <section class="section">
    <div class="col-header sinking-grid">
      <span class="section-label">Sinking Funds</span>
      <span class="col-label-right">Balance</span>
      <span class="col-label-right">+/mo</span>
      <span></span>
    </div>
    {#each sinkingAll as cat (cat.category_id)}
      {@render sinkingCatRow(cat)}
    {/each}
  </section>
{/snippet}

{#snippet reallocationLogSection()}
  {#if view!.reallocation_log.length > 0}
    <section class="section">
      <div class="col-header realloc-log-header">
        <span class="section-label">Reallocations this month</span>
      </div>
      {#each view!.reallocation_log as entry (entry.id)}
        <div class="realloc-log-row">
          <span class="realloc-names">{entry.from_name} → {entry.to_name}</span>
          <span class="mono-cell">{formatCents(entry.amount_cents)}</span>
        </div>
      {/each}
    </section>
  {/if}
{/snippet}

{#snippet sidebarContent()}
  {@const lta = view?.left_to_allocate_cents ?? 0}
  <div class="lta-card">
    <span class="lta-amount" class:positive={lta >= 0} class:negative={lta < 0}>
      {formatCents(Math.abs(lta))}
    </span>
    <span class="section-label">{lta < 0 ? 'Over-allocated' : 'Left to budget'}</span>
  </div>
  <div class="summary-section">
    <div class="bar-row">
      <span class="bar-label">Income</span>
      <div class="bar-track">
        <div class="bar-fill bar-income" style="width:{Math.round((totalIncome / barMax) * 100)}%"></div>
      </div>
      <span class="bar-amount">{formatCents(totalIncome)}</span>
    </div>
    <div class="bar-row">
      <span class="bar-label">Expenses</span>
      <div class="bar-track">
        <div class="bar-fill bar-expense" style="width:{Math.round((totalSpent / barMax) * 100)}%"></div>
      </div>
      <span class="bar-amount">{formatCents(totalSpent)}</span>
    </div>
  </div>
{/snippet}

<div class="page">
  <header class="page-header">
    <button class="nav-btn" onclick={() => navigate(-1)} aria-label="Previous month">&#8249;</button>
    <h1 class="month-title" class:is-current={selectedMonth === CURRENT_MONTH}>
      {toMonthLabel(selectedMonth)}
    </h1>
    <button class="nav-btn" onclick={() => navigate(1)} aria-label="Next month">&#8250;</button>
  </header>

  {#if loading}
    <div class="state-msg"><span class="muted">Loading...</span></div>
  {:else if !view}
    <div class="state-msg"><span class="muted">No data available.</span></div>
  {:else}
    <div class="layout">
      <main class="main-col">
        {#if view.income_rows.length > 0}{@render incomeSection()}{/if}
        {#if view.flow_groups.length > 0 || view.flow_ungrouped.length > 0}{@render expenseSection()}{/if}
        {#if sinkingAll.length > 0}{@render sinkingSection()}{/if}
        {@render reallocationLogSection()}
      </main>
      <aside class="sidebar">
        {@render sidebarContent()}
      </aside>
    </div>
  {/if}
</div>

<ReallocateDialog
  bind:open={reallocOpen}
  categories={allExpenseCategories}
  initialFrom={reallocInitialFrom}
  initialTo={reallocInitialTo}
  month={selectedMonth}
  onsuccess={() => { void load(); }}
/>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .page-header {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 20px;
    padding: 14px 32px;
    border-bottom: 1px solid var(--border);
    background: var(--card);
    flex-shrink: 0;
  }

  .nav-btn {
    font-size: 24px;
    line-height: 1;
    color: var(--muted-foreground);
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 2px 8px;
    transition: color 0.12s;
  }

  .nav-btn:hover { color: var(--foreground); }

  .month-title {
    font-size: 18px;
    font-weight: 700;
    color: var(--foreground);
    margin: 0;
    min-width: 200px;
    text-align: center;
  }

  .month-title.is-current { color: var(--primary); }

  .layout {
    display: flex;
    flex: 1;
    overflow: hidden;
    min-height: 0;
  }

  .main-col {
    flex: 1;
    overflow-y: auto;
    min-width: 0;
  }

  .sidebar {
    width: 224px;
    flex-shrink: 0;
    border-left: 1px solid var(--border);
    overflow-y: auto;
    padding: 28px 20px;
    display: flex;
    flex-direction: column;
    gap: 32px;
  }

  .state-msg {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    padding: 48px;
  }

  .muted {
    font-size: 13px;
    color: var(--muted-foreground);
  }

  /* Sections */

  .section {
    padding: 18px 28px;
    border-bottom: 1px solid var(--border);
  }

  .section:last-child { border-bottom: none; }

  /* Grid templates shared between headers and rows */

  .income-grid,
  .income-row {
    display: grid;
    grid-template-columns: 1fr 88px;
    align-items: center;
    gap: 4px;
  }

  .expense-grid,
  .expense-row {
    display: grid;
    grid-template-columns: 1fr 88px 72px 88px 28px;
    align-items: center;
    gap: 4px;
  }

  .sinking-grid,
  .sinking-row {
    display: grid;
    grid-template-columns: 1fr 88px 80px 28px;
    align-items: center;
    gap: 4px;
  }

  /* Column headers */

  .col-header {
    margin-bottom: 6px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border);
  }

  /* Shared label style: section titles and the sidebar "Left to budget" label */
  .section-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted-foreground);
    text-transform: uppercase;
    letter-spacing: 0.07em;
  }

  .col-label-right {
    font-size: 10px;
    font-weight: 600;
    color: var(--muted-foreground);
    text-transform: uppercase;
    letter-spacing: 0.07em;
    text-align: right;
  }

  /* Data rows */

  .income-row { padding: 5px 0; }
  .expense-row { padding: 5px 0; }
  .sinking-row { padding: 5px 0; }

  .cat-name {
    font-size: 13px;
    font-weight: 400;
    color: var(--foreground);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .cat-indent { padding-left: 20px; }

  /* Amount cells */

  .mono-cell {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 13px;
    text-align: right;
    color: var(--muted-foreground);
    white-space: nowrap;
  }

  .mono-cell.positive { color: var(--primary); }
  .mono-cell.negative { color: var(--destructive); }
  .mono-cell.muted { color: var(--muted-foreground); }

  /* Remaining cell: amount + optional Cover? */

  .remaining-cell {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
  }

  /* Cover? affordance for overspent flow categories */

  .cover-btn {
    font-size: 10px;
    font-weight: 600;
    color: var(--destructive);
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    white-space: nowrap;
    opacity: 0.7;
    transition: opacity 0.12s;
  }

  .cover-btn:hover { opacity: 1; }

  /* Move button (ArrowLeftRight icon) */

  .move-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--muted-foreground);
    padding: 0;
    opacity: 0;
    transition: opacity 0.12s, color 0.12s;
  }

  .move-btn-sinking {
    opacity: 0;
  }

  .expense-row:hover .move-btn,
  .sinking-row:hover .move-btn {
    opacity: 1;
  }

  .move-btn:hover { color: var(--foreground); }

  /* Allocation input */

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
    width: 100%;
    text-align: right;
    padding: 1px 2px;
  }

  .alloc-input:focus { border-bottom-color: var(--primary); }
  .alloc-input::placeholder { color: var(--muted-foreground); font-weight: 400; }

  /* Group blocks */

  .group-block { margin-bottom: 2px; }

  .group-btn {
    background: transparent;
    border: none;
    cursor: pointer;
    width: 100%;
    padding: 6px 0;
    font-size: 13px;
    color: var(--foreground);
    border-radius: 0;
  }

  .group-btn:hover { background: color-mix(in oklch, var(--muted) 60%, transparent); }

  .group-label {
    display: flex;
    align-items: center;
    gap: 4px;
    font-weight: 600;
    color: var(--foreground);
  }

  .group-label-plain {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted-foreground);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .group-ungrouped { padding: 6px 0 2px; }

  /* Reallocation log */

  .realloc-log-header {
    display: block;
  }

  .realloc-log-row {
    display: grid;
    grid-template-columns: 1fr 88px;
    align-items: center;
    gap: 4px;
    padding: 5px 0;
  }

  .realloc-names {
    font-size: 13px;
    color: var(--muted-foreground);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Sidebar */

  .lta-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .lta-amount {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 30px;
    font-weight: 700;
    line-height: 1;
    color: var(--muted-foreground);
  }

  .lta-amount.positive { color: var(--primary); }
  .lta-amount.negative { color: var(--destructive); }

  .summary-section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .bar-row {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .bar-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--muted-foreground);
    text-transform: uppercase;
    letter-spacing: 0.07em;
  }

  .bar-track {
    height: 5px;
    background: var(--muted);
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    transition: width 0.25s ease;
  }

  .bar-income { background: var(--chart-1); }
  .bar-expense { background: var(--chart-2); }

  .bar-amount {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
    color: var(--foreground);
  }
</style>
