<script lang="ts">
  import { onMount } from 'svelte';
  import { listCategories } from '$lib/categories';
  import type { CategoryRow } from '$lib/categories';
  import { listCategoryGroups } from '$lib/groups';
  import type { CategoryGroupRow } from '$lib/groups';
  import { listIncomeCategories, createIncomeCategory, setIncomeCategoryHidden } from '$lib/income';
  import type { IncomeCategoryItem } from '$lib/income';
  import CategorySection from '$lib/components/CategorySection.svelte';

  let categories = $state<CategoryRow[]>([]);
  let groups = $state<CategoryGroupRow[]>([]);
  let incomeCategories = $state<IncomeCategoryItem[]>([]);
  let loading = $state(true);
  let newIncomeName = $state('');
  let newIncomeError = $state('');

  let flow = $derived(categories.filter((c) => c.kind === 'flow'));
  let sinking = $derived(categories.filter((c) => c.kind === 'sinking'));

  async function load() {
    loading = true;
    try {
      [categories, groups, incomeCategories] = await Promise.all([
        listCategories(),
        listCategoryGroups(),
        listIncomeCategories(),
      ]);
    } catch {
      // No Tauri backend in pnpm dev
    } finally {
      loading = false;
    }
  }

  async function addIncomeCategory() {
    const name = newIncomeName.trim();
    if (!name) return;
    try {
      await createIncomeCategory(name);
      newIncomeName = '';
      newIncomeError = '';
      incomeCategories = await listIncomeCategories();
    } catch (e) {
      newIncomeError = e instanceof Error ? e.message : String(e);
    }
  }

  async function toggleHidden(item: IncomeCategoryItem) {
    try {
      await setIncomeCategoryHidden(item.id, !item.hidden);
      incomeCategories = await listIncomeCategories();
    } catch {
      // non-fatal
    }
  }

  onMount(load);
</script>

<div class="page">
  <h1 class="page-title">Categories</h1>

  {#if loading}
    <p class="loading">Loading...</p>
  {:else}
    <section class="income-section">
      <h2 class="section-title">Income</h2>
      <ul class="income-list">
        {#each incomeCategories as item (item.id)}
          <li class="income-item" class:hidden-row={item.hidden}>
            <span class="income-name">{item.name}</span>
            <button
              class="toggle-btn"
              onclick={() => toggleHidden(item)}
            >
              {item.hidden ? 'Show' : 'Hide'}
            </button>
          </li>
        {/each}
      </ul>
      <form class="add-form" onsubmit={(e) => { e.preventDefault(); addIncomeCategory(); }}>
        <input class="add-input" placeholder="New income category" bind:value={newIncomeName} />
        <button class="add-btn" type="submit">Add</button>
      </form>
      {#if newIncomeError}
        <p class="error">{newIncomeError}</p>
      {/if}
    </section>

    <CategorySection kind="flow" label="Flow" items={flow} {groups} onChanged={load} />
    <CategorySection kind="sinking" label="Sinking" items={sinking} {groups} onChanged={load} />
  {/if}
</div>

<style>
  .page {
    padding: 32px 40px;
    max-width: 640px;
  }

  .page-title {
    font-family: var(--font-display);
    font-size: 20px;
    font-weight: 700;
    color: var(--text);
    margin: 0 0 32px;
    letter-spacing: 0.04em;
  }

  .loading {
    color: var(--muted-foreground);
    font-size: 13px;
  }

  .income-section {
    margin-bottom: 32px;
  }

  .section-title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    margin: 0 0 12px;
    border-bottom: 1px solid var(--border);
    padding-bottom: 6px;
  }

  .income-list {
    list-style: none;
    margin: 0 0 12px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .income-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
  }

  .hidden-row .income-name {
    opacity: 0.4;
  }

  .income-name {
    flex: 1;
    font-size: 13px;
    color: var(--text);
  }

  .toggle-btn {
    background: none;
    border: 1px solid var(--border);
    color: var(--muted-foreground);
    font-size: 11px;
    padding: 3px 10px;
    cursor: pointer;
    letter-spacing: 0.04em;
  }

  .toggle-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .add-form {
    display: flex;
    gap: 8px;
  }

  .add-input {
    flex: 1;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    color: var(--text);
    font-size: 13px;
    padding: 6px 10px;
    outline: none;
  }

  .add-input:focus {
    border-color: var(--accent);
  }

  .add-btn {
    background: var(--accent);
    color: var(--accent-foreground);
    border: none;
    font-size: 12px;
    font-weight: 600;
    padding: 6px 14px;
    cursor: pointer;
    letter-spacing: 0.04em;
  }

  .add-btn:hover {
    opacity: 0.85;
  }

  .error {
    font-size: 12px;
    color: var(--destructive);
    margin: 4px 0 0;
  }
</style>
