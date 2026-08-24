<script lang="ts">
  import { createCategory, updateCategory } from '$lib/categories';
  import type { CategoryRow } from '$lib/categories';

  let { items, kind, label, onChanged }: {
    items: CategoryRow[];
    kind: 'flow' | 'sinking';
    label: string;
    onChanged: () => void;
  } = $props();

  let addName = $state('');
  let addError = $state('');
  let editingId = $state<string | null>(null);
  let editingName = $state('');
  let editingError = $state('');

  function errMsg(e: unknown): string {
    return e instanceof Error ? e.message : String(e);
  }

  async function add() {
    const name = addName.trim();
    if (!name) return;
    try {
      await createCategory(name, kind);
      addName = '';
      addError = '';
      onChanged();
    } catch (e) {
      addError = errMsg(e);
    }
  }

  function startEdit(cat: CategoryRow) {
    editingId = cat.id;
    editingName = cat.name;
    editingError = '';
  }

  function cancelEdit() {
    editingId = null;
    editingName = '';
    editingError = '';
  }

  async function commitEdit() {
    const id = editingId;
    if (!id) return;
    const name = editingName.trim();
    if (!name) { editingError = 'Name cannot be blank'; return; }
    try {
      await updateCategory(id, name);
      cancelEdit();
      onChanged();
    } catch (e) {
      editingError = errMsg(e);
    }
  }

  function onEditKey(e: KeyboardEvent) {
    if (e.key === 'Enter') commitEdit();
    if (e.key === 'Escape') cancelEdit();
  }

  function focusOnMount(node: HTMLElement) {
    node.focus();
  }
</script>

<section class="section">
  <h2 class="section-title">{label}</h2>
  <ul class="list">
    {#each items as cat (cat.id)}
      <li class="item">
        {#if editingId === cat.id}
          <input
            class="edit-input"
            use:focusOnMount
            bind:value={editingName}
            onblur={commitEdit}
            onkeydown={onEditKey}
          />
          {#if editingError}
            <span class="error">{editingError}</span>
          {/if}
        {:else}
          <button class="name-btn" onclick={() => startEdit(cat)}>{cat.name}</button>
        {/if}
      </li>
    {/each}
  </ul>

  <form class="add-form" onsubmit={(e) => { e.preventDefault(); add(); }}>
    <input class="add-input" placeholder="New category name" bind:value={addName} />
    <button class="add-btn" type="submit">Add</button>
  </form>
  {#if addError}
    <p class="error">{addError}</p>
  {/if}
</section>

<style>
  .section {
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

  .list {
    list-style: none;
    margin: 0 0 12px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .name-btn {
    background: none;
    border: none;
    padding: 6px 8px;
    color: var(--text);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
    width: 100%;
  }

  .name-btn:hover {
    background: var(--surface-hover);
    color: var(--accent);
  }

  .edit-input {
    flex: 1;
    background: var(--surface-raised);
    border: 1px solid var(--accent);
    color: var(--text);
    font-size: 13px;
    padding: 5px 8px;
    outline: none;
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
