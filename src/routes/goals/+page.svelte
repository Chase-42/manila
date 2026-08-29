<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import * as AlertDialog from '$lib/components/ui/alert-dialog';
  import GoalDialog from '$lib/components/GoalDialog.svelte';
  import { listGoalsWithProgress, deleteGoal, formatDaysUntilTarget } from '$lib/goals';
  import { listCategories } from '$lib/categories';
  import { formatCents } from '$lib/money';
  import type { GoalWithProgress } from '$lib/goals';
  import type { CategoryRow } from '$lib/categories';

  let goals = $state<GoalWithProgress[]>([]);
  let categories = $state<CategoryRow[]>([]);

  let dialogOpen = $state(false);
  let editingGoal = $state<GoalWithProgress | undefined>(undefined);
  let deleteOpen = $state(false);
  let deletingGoal = $state<GoalWithProgress | undefined>(undefined);
  let deleteError = $state<string | null>(null);

  const sinkingCategories = $derived(categories.filter((c) => c.kind === 'sinking'));

  async function load() {
    try {
      [goals, categories] = await Promise.all([listGoalsWithProgress(), listCategories()]);
    } catch {
      // non-fatal: running without Tauri in pnpm dev
    }
  }

  async function reload() {
    try {
      goals = await listGoalsWithProgress();
    } catch {
      // non-fatal
    }
  }

  onMount(load);

  function openCreate() {
    editingGoal = undefined;
    dialogOpen = true;
  }

  function openEdit(goal: GoalWithProgress) {
    editingGoal = goal;
    dialogOpen = true;
  }

  function openDelete(goal: GoalWithProgress) {
    deletingGoal = goal;
    deleteError = null;
    deleteOpen = true;
  }

  async function confirmDelete() {
    if (!deletingGoal) return;
    try {
      await deleteGoal(deletingGoal.id);
      deleteOpen = false;
      deletingGoal = undefined;
      await reload();
    } catch (e) {
      deleteError = e instanceof Error ? e.message : String(e);
    }
  }

  function categoryName(goal: GoalWithProgress): string | null {
    if (!goal.category_id) return null;
    return categories.find((c) => c.id === goal.category_id)?.name ?? null;
  }

  function progressPct(goal: GoalWithProgress): number {
    if (goal.target_amount_cents <= 0) return 0;
    const pct = Math.round((goal.current_balance_cents * 100) / goal.target_amount_cents);
    return Math.max(0, Math.min(100, pct));
  }

  function daysLabel(goal: GoalWithProgress): string | null {
    return goal.target_date ? formatDaysUntilTarget(goal.target_date) : null;
  }
</script>

<div class="goals-page">
  <div class="page-header">
    <h1 class="page-title">Goals</h1>
    <Button onclick={openCreate}>New goal</Button>
  </div>

  {#if goals.length === 0}
    <div class="empty">
      <p class="empty-text">No goals yet.</p>
      <Button onclick={openCreate}>Create your first goal</Button>
    </div>
  {:else}
    <div class="goals-list">
      {#each goals as goal (goal.id)}
        {@const reached = goal.current_balance_cents >= goal.target_amount_cents}
        {@const pct = progressPct(goal)}
        {@const catName = categoryName(goal)}
        {@const days = daysLabel(goal)}
        <div class="goal-row">
          <div class="goal-info">
            <div class="goal-top">
              <span class="goal-name">{goal.name}</span>
              {#if reached}
                <span class="reached-badge">Reached</span>
              {/if}
            </div>
            {#if catName}
              <span class="goal-category">{catName}</span>
            {/if}
            <div class="progress-bar">
              <div class="progress-fill" style="width: {pct}%"></div>
            </div>
            <div class="goal-amounts">
              <span class="goal-current">{formatCents(goal.current_balance_cents)}</span>
              <span class="goal-sep">/</span>
              <span class="goal-target">{formatCents(goal.target_amount_cents)}</span>
              {#if days}
                <span class="goal-days">{days}</span>
              {/if}
            </div>
          </div>
          <div class="goal-actions">
            <Button variant="ghost" size="sm" onclick={() => openEdit(goal)}>Edit</Button>
            <Button variant="ghost" size="sm" onclick={() => openDelete(goal)}>Delete</Button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<GoalDialog
  goal={editingGoal}
  categories={sinkingCategories}
  bind:open={dialogOpen}
  ondone={reload}
/>

<AlertDialog.Root bind:open={deleteOpen}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>Delete goal</AlertDialog.Title>
      <AlertDialog.Description>
        Delete "{deletingGoal?.name}"? This cannot be undone.
      </AlertDialog.Description>
    </AlertDialog.Header>
    {#if deleteError}
      <p class="delete-error">{deleteError}</p>
    {/if}
    <AlertDialog.Footer>
      <AlertDialog.Cancel onclick={() => (deleteOpen = false)}>Cancel</AlertDialog.Cancel>
      <AlertDialog.Action onclick={confirmDelete}>Delete</AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>

<style>
  .goals-page {
    padding: 32px;
    display: flex;
    flex-direction: column;
    gap: 24px;
    max-width: 800px;
  }

  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .page-title {
    font-family: var(--font-display);
    font-size: 20px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--foreground);
    margin: 0;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 64px 0;
  }

  .empty-text {
    font-family: var(--font-display);
    color: var(--muted-foreground);
    margin: 0;
  }

  .goals-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    border: 1px solid var(--border);
  }

  .goal-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 16px;
    background: var(--card);
    border-bottom: 1px solid var(--border);
  }

  .goal-row:last-child {
    border-bottom: none;
  }

  .goal-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .goal-top {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .goal-name {
    font-size: 14px;
    font-weight: 600;
    color: var(--foreground);
  }

  .reached-badge {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    color: var(--primary);
    border: 1px solid var(--primary);
    padding: 1px 6px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .goal-category {
    font-size: 12px;
    color: var(--muted-foreground);
  }

  .progress-bar {
    height: 4px;
    background: var(--border);
    width: 100%;
    max-width: 360px;
  }

  .progress-fill {
    height: 100%;
    background: var(--primary);
    transition: width 0.2s ease;
  }

  .goal-amounts {
    display: flex;
    align-items: baseline;
    gap: 4px;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }

  .goal-current {
    font-size: 14px;
    color: var(--foreground);
  }

  .goal-sep {
    color: var(--muted-foreground);
    font-size: 12px;
  }

  .goal-target {
    font-size: 13px;
    color: var(--muted-foreground);
  }

  .goal-days {
    font-size: 12px;
    color: var(--muted-foreground);
    margin-left: 8px;
  }

  .goal-actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }

  .delete-error {
    font-size: 12px;
    color: var(--destructive);
    margin: 0 0 8px;
  }
</style>
