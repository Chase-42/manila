<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import { Label } from "$lib/components/ui/label";
  import { Button } from "$lib/components/ui/button";
  import { reallocate, parseCentsFromString, validateReallocation } from "$lib/budget";

  type CategoryOption = { category_id: string; category_name: string };

  let {
    open = $bindable(false),
    categories,
    initialFrom = undefined,
    initialTo = undefined,
    month,
    onsuccess,
  }: {
    open?: boolean;
    categories: CategoryOption[];
    initialFrom?: string;
    initialTo?: string;
    month: string;
    onsuccess: () => void;
  } = $props();

  // One-shot initializations from props; parent uses {#key} to remount on each open.
  let fromId = $state(initialFrom ?? "");
  let toId = $state(initialTo ?? "");
  let amountStr = $state("");
  let error = $state<string | null>(null);
  let saving = $state(false);

  async function handleSubmit(e: Event) {
    e.preventDefault();
    const msg = validateReallocation(fromId, toId, amountStr);
    if (msg) { error = msg; return; }
    error = null;
    saving = true;
    try {
      await reallocate(fromId, toId, month, parseCentsFromString(amountStr));
      open = false;
      onsuccess();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Move money</Dialog.Title>
      <Dialog.Description>Transfer an allocation from one category to another.</Dialog.Description>
    </Dialog.Header>

    <form onsubmit={handleSubmit} class="form">
      <div class="field">
        <Label for="realloc-from">From</Label>
        <select id="realloc-from" class="field-select" bind:value={fromId}>
          <option value="">Select category</option>
          {#each categories as cat (cat.category_id)}
            <option value={cat.category_id}>{cat.category_name}</option>
          {/each}
        </select>
      </div>

      <div class="field">
        <Label for="realloc-to">To</Label>
        <select id="realloc-to" class="field-select" bind:value={toId}>
          <option value="">Select category</option>
          {#each categories as cat (cat.category_id)}
            <option value={cat.category_id}>{cat.category_name}</option>
          {/each}
        </select>
      </div>

      <div class="field">
        <Label for="realloc-amount">Amount</Label>
        <input
          id="realloc-amount"
          class="field-input"
          type="text"
          inputmode="decimal"
          placeholder="0.00"
          bind:value={amountStr}
        />
      </div>

      {#if error}
        <p class="error-msg">{error}</p>
      {/if}

      <Dialog.Footer>
        <Button type="button" variant="ghost" onclick={() => (open = false)}>Cancel</Button>
        <Button type="submit" disabled={saving}>
          {saving ? "Moving..." : "Move money"}
        </Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-top: 4px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field-select,
  .field-input {
    font-family: inherit;
    font-size: 13px;
    color: var(--foreground);
    background: var(--background);
    border: 1px solid var(--border);
    outline: none;
    padding: 6px 8px;
    width: 100%;
  }

  .field-select:focus,
  .field-input:focus {
    border-color: var(--primary);
  }

  .field-input {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }

  .error-msg {
    font-size: 12px;
    color: var(--destructive);
    margin: 0;
  }
</style>
