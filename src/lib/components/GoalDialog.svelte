<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Button } from "$lib/components/ui/button";
  import { createGoal, updateGoal } from "$lib/goals";
  import { parseCentsFromString } from "$lib/budget";
  import type { GoalWithProgress } from "$lib/goals";
  import type { CategoryRow } from "$lib/categories";

  let {
    goal = undefined,
    categories,
    open = $bindable(false),
    ondone,
  }: {
    goal?: GoalWithProgress;
    categories: CategoryRow[];
    open?: boolean;
    ondone: () => void;
  } = $props();

  const isEdit = $derived(goal !== undefined);

  const sinkingCategories = $derived(categories.filter((c) => c.kind === "sinking"));

  let name = $state("");
  let amountStr = $state("");
  let categoryId = $state<string>("");
  let targetDate = $state("");
  let error = $state<string | null>(null);
  let saving = $state(false);

  function initFields(g: GoalWithProgress | undefined) {
    if (!g) {
      name = "";
      amountStr = "";
      categoryId = "";
      targetDate = "";
      error = null;
      return;
    }
    name = g.name;
    amountStr = (g.target_amount_cents / 100).toFixed(2);
    categoryId = g.category_id ?? "";
    targetDate = g.target_date ?? "";
    error = null;
  }

  $effect(() => {
    if (open) initFields(goal);
  });

  function validateForm(trimmedName: string, cents: number): string | null {
    if (!trimmedName) return "Name is required.";
    if (cents <= 0) return "Target amount must be greater than zero.";
    return null;
  }

  async function executeSave(trimmedName: string, cents: number): Promise<void> {
    const catId = categoryId || null;
    const date = targetDate || null;
    if (isEdit) {
      await updateGoal(goal!.id, trimmedName, cents, catId, date);
    } else {
      await createGoal(trimmedName, cents, catId, date);
    }
    open = false;
    ondone();
  }

  async function handleSubmit(e: Event) {
    e.preventDefault();
    const trimmedName = name.trim();
    const cents = parseCentsFromString(amountStr);
    const validError = validateForm(trimmedName, cents);
    if (validError) {
      error = validError;
      return;
    }
    error = null;
    saving = true;
    try {
      await executeSave(trimmedName, cents);
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
      <Dialog.Title>{isEdit ? "Edit goal" : "New goal"}</Dialog.Title>
    </Dialog.Header>

    <form onsubmit={handleSubmit} class="form">
      {#if error}
        <p class="error">{error}</p>
      {/if}

      <div class="field">
        <Label for="goal-name">Name</Label>
        <Input id="goal-name" bind:value={name} placeholder="e.g. Car maintenance" required />
      </div>

      <div class="field">
        <Label for="goal-amount">Target amount</Label>
        <Input
          id="goal-amount"
          bind:value={amountStr}
          placeholder="0.00"
          inputmode="decimal"
        />
      </div>

      <div class="field">
        <Label for="goal-category">Linked sinking category</Label>
        <select id="goal-category" bind:value={categoryId} class="select-input">
          <option value="">None (standalone)</option>
          {#each sinkingCategories as cat (cat.id)}
            <option value={cat.id}>{cat.name}</option>
          {/each}
        </select>
      </div>

      <div class="field">
        <Label for="goal-date">Target date (optional)</Label>
        <Input id="goal-date" type="date" bind:value={targetDate} />
      </div>

      <Dialog.Footer>
        <Button type="button" variant="ghost" onclick={() => (open = false)}>Cancel</Button>
        <Button type="submit" disabled={saving}>
          {saving ? "Saving..." : isEdit ? "Save changes" : "Create goal"}
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
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .select-input {
    width: 100%;
    background: var(--input);
    border: 1px solid var(--border);
    color: var(--foreground);
    font-size: 14px;
    padding: 8px 10px;
    outline: none;
  }

  .select-input:focus {
    border-color: var(--ring);
  }

  .error {
    font-size: 12px;
    color: var(--destructive);
    margin: 0;
  }
</style>
