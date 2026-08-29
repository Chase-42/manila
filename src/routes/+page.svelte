<script lang="ts">
  import { formatCents } from '$lib/money';

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
</style>
