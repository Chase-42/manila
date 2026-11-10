<script lang="ts">
  import { ensureEnrolled } from "$lib/sync";
  import PlaidLinkDialog from "$lib/components/PlaidLinkDialog.svelte";
  import { Button } from "$lib/components/ui/button";

  let { onDone }: { onDone: () => void } = $props();

  type Phase = "bank" | "done";

  let phase = $state<Phase>("bank");
  let enrolling = $state(false);
  let enrollError = $state<string | null>(null);

  let plaidOpen = $state(false);
  let plaidConnected = $state(false);

  $effect(() => {
    if (phase === "bank") {
      runEnsureEnrolled();
    }
  });

  async function runEnsureEnrolled() {
    enrolling = true;
    enrollError = null;
    try {
      await ensureEnrolled();
    } catch (e) {
      enrollError = e instanceof Error ? e.message : String(e);
    } finally {
      enrolling = false;
    }
  }

  function handlePlaidSuccess() {
    plaidConnected = true;
    phase = "done";
  }

  function skipBank() {
    phase = "done";
  }
</script>

<PlaidLinkDialog bind:open={plaidOpen} onsuccess={handlePlaidSuccess} />

<div class="onboarding-screen">
  <div class="onboarding-card" class:wide={phase !== "done"}>
    <div class="wordmark">manila</div>

    {#if phase === "bank"}
      <h1 class="step-heading">Connect a bank account</h1>
      <p class="step-body">
        Link your bank for automatic transaction import via Plaid.
      </p>
      <p class="step-body transparency">
        Bank sync uses Manila's secure server. Your financial data is never stored there.
      </p>
      <div class="step-form">
        {#if enrolling}
          <p class="status-msg">Connecting to Manila's server…</p>
          <Button size="lg" class="action-btn" disabled={true}>
            Connect a bank
          </Button>
        {:else if enrollError}
          <p class="field-error">{enrollError}</p>
          <Button onclick={runEnsureEnrolled} size="lg" class="action-btn">
            Try again
          </Button>
        {:else}
          <Button
            onclick={() => (plaidOpen = true)}
            size="lg"
            class="action-btn"
          >
            Connect a bank
          </Button>
          <button class="skip-link" onclick={skipBank}>
            Skip for now
          </button>
        {/if}
      </div>
    {:else}
      <h1 class="step-heading">You're all set!</h1>
      <p class="step-body">
        {#if plaidConnected}
          Bank connection is configured.
        {:else}
          Add bank connections any time from Accounts.
        {/if}
      </p>
      <Button onclick={onDone} size="lg" class="action-btn">
        Get started
      </Button>
    {/if}
  </div>
</div>

<style>
  .onboarding-screen {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    background: var(--background);
  }

  .onboarding-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 24px;
    width: 320px;
  }

  .onboarding-card.wide {
    width: 400px;
  }

  .wordmark {
    font-family: var(--font-display);
    font-size: 32px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--primary);
  }

  .step-heading {
    font-family: var(--font-display);
    font-size: 16px;
    font-weight: 500;
    color: var(--muted-foreground);
    margin: 0;
    letter-spacing: 0.02em;
    text-align: center;
  }

  .step-body {
    font-size: 13px;
    color: var(--muted-foreground);
    text-align: center;
    margin: 0;
    line-height: 1.5;
    max-width: 340px;
  }

  .step-body.transparency {
    font-size: 11px;
    opacity: 0.7;
  }

  .step-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
  }

  .field-error {
    font-size: 12px;
    color: var(--destructive);
    margin: 0;
    text-align: center;
  }

  .status-msg {
    font-size: 13px;
    color: var(--muted-foreground);
    margin: 0;
    text-align: center;
  }

  .skip-link {
    background: none;
    border: none;
    padding: 0;
    font-size: 12px;
    color: var(--muted-foreground);
    cursor: pointer;
    text-align: center;
  }

  .skip-link:hover {
    color: var(--foreground);
  }

  :global(.action-btn) {
    width: 100%;
  }
</style>
