<script lang="ts">
  import { Target } from '@lucide/svelte';
  import { formatCents } from '$lib/money';
  import type { GoalWithProgress } from '$lib/goals';

  let { data } = $props();

  const today = new Date();
  const dateLabel = today.toLocaleDateString('en-US', {
    weekday: 'long',
    month: 'long',
    day: 'numeric',
  });

  const daily = $derived(data.home?.safe_to_spend_daily_cents ?? 0);
  const remaining = $derived(data.home?.flow_remaining_cents ?? 0);
  const positive = $derived(daily > 0);

  function goalProgressRatio(g: GoalWithProgress): number {
    return g.target_amount_cents > 0
      ? Math.floor((g.current_balance_cents * 100) / g.target_amount_cents)
      : 0;
  }

  function compareGoalProgress(a: GoalWithProgress, b: GoalWithProgress): number {
    const diff = goalProgressRatio(b) - goalProgressRatio(a);
    return diff !== 0 ? diff : a.created_at < b.created_at ? -1 : 1;
  }

  function pickFeaturedGoal(goals: GoalWithProgress[]): GoalWithProgress | null {
    if (goals.length === 0) return null;
    return [...goals].sort(compareGoalProgress)[0] ?? null;
  }

  const featured = $derived(pickFeaturedGoal(data.goals ?? []));

  function progressPct(goal: GoalWithProgress): number {
    if (goal.target_amount_cents <= 0) return 0;
    return Math.max(0, Math.min(100,
      Math.floor((goal.current_balance_cents * 100) / goal.target_amount_cents)
    ));
  }
</script>

<div class="home">
  <header class="home-header">
    <span class="date-label">{dateLabel}</span>
  </header>

  <div class="hero-card">
    <div class="hero-amount" class:muted={!positive}>
      {formatCents(daily)}
    </div>
    <div class="hero-label">per day</div>
    <div class="hero-sub">{formatCents(remaining)} left this month</div>
  </div>

  {#if featured}
    {@const reached = featured.current_balance_cents >= featured.target_amount_cents}
    {@const pct = progressPct(featured)}
    <div class="goal-card">
      <div class="goal-card-header">
        <Target size={14} />
        <span class="goal-name">{featured.name}</span>
        {#if reached}
          <span class="reached-badge">Reached</span>
        {/if}
      </div>
      <div class="goal-amounts">
        <span class="goal-current">{formatCents(featured.current_balance_cents)}</span>
        <span class="goal-sep">/</span>
        <span class="goal-target">{formatCents(featured.target_amount_cents)}</span>
      </div>
      <div class="progress-bar">
        <div class="progress-fill" style="width: {pct}%"></div>
      </div>
    </div>
  {/if}
</div>

<style>
  .home {
    padding: 32px;
    display: flex;
    flex-direction: column;
    gap: 24px;
    max-width: 960px;
  }

  .home-header {
    display: flex;
    align-items: center;
  }

  .date-label {
    font-family: var(--font-display);
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--muted-foreground);
    text-transform: uppercase;
  }

  .hero-card {
    border: 1px solid var(--border);
    padding: 40px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  .hero-amount {
    font-family: var(--font-mono);
    font-size: 64px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    line-height: 1;
    color: var(--primary);
  }

  .hero-amount.muted {
    color: var(--muted-foreground);
  }

  .hero-label {
    font-family: var(--font-mono);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
    color: var(--muted-foreground);
    letter-spacing: 0.04em;
  }

  .hero-sub {
    margin-top: 8px;
    font-family: var(--font-mono);
    font-size: 14px;
    font-variant-numeric: tabular-nums;
    color: var(--muted-foreground);
  }

  .goal-card {
    border: 1px solid var(--border);
    padding: 20px 24px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .goal-card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted-foreground);
  }

  .goal-name {
    font-family: var(--font-display);
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 0.03em;
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

  .goal-amounts {
    display: flex;
    align-items: baseline;
    gap: 4px;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }

  .goal-current {
    font-size: 20px;
    font-weight: 700;
    color: var(--foreground);
  }

  .goal-sep {
    color: var(--muted-foreground);
    font-size: 14px;
  }

  .goal-target {
    font-size: 14px;
    color: var(--muted-foreground);
  }

  .progress-bar {
    height: 4px;
    background: var(--border);
    width: 100%;
  }

  .progress-fill {
    height: 100%;
    background: var(--primary);
  }
</style>
