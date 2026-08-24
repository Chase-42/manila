<script lang="ts">
  import { onMount } from 'svelte';
  import { listCategories } from '$lib/categories';
  import type { CategoryRow } from '$lib/categories';
  import CategorySection from '$lib/components/CategorySection.svelte';

  let categories = $state<CategoryRow[]>([]);
  let loading = $state(true);

  let flow = $derived(categories.filter((c) => c.kind === 'flow'));
  let sinking = $derived(categories.filter((c) => c.kind === 'sinking'));

  async function load() {
    loading = true;
    try {
      categories = await listCategories();
    } catch {
      // No Tauri backend in pnpm dev
    } finally {
      loading = false;
    }
  }

  onMount(load);
</script>

<div class="page">
  <h1 class="page-title">Categories</h1>

  {#if loading}
    <p class="loading">Loading...</p>
  {:else}
    <CategorySection kind="flow" label="Flow" items={flow} onChanged={load} />
    <CategorySection kind="sinking" label="Sinking" items={sinking} onChanged={load} />
  {/if}
</div>

<style>
  .page {
    padding: 32px 40px;
    max-width: 600px;
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
</style>
