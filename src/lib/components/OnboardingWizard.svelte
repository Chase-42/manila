<script lang="ts">
  import { enrollSync } from "$lib/sync";
  import PlaidLinkDialog from "$lib/components/PlaidLinkDialog.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";

  let { onDone }: { onDone: () => void } = $props();

  type Phase = "sync" | "bank" | "done";

  let phase = $state<Phase>("sync");
  let syncUrl = $state(localStorage.getItem("manila.plaidProxyUrl") ?? "");
  let syncWorking = $state(false);
  let syncError = $state<string | null>(null);
  let syncConfigured = $state(false);

  let plaidOpen = $state(false);
  let plaidConnected = $state(false);

  function validateUrl(url: string): string | null {
    if (!url.trim()) return "Server URL is required.";
    if (!url.startsWith("http://") && !url.startsWith("https://"))
      return "URL must start with http:// or https://";
    return null;
  }

  async function handleConnect() {
    syncError = null;
    const err = validateUrl(syncUrl);
    if (err) {
      syncError = err;
      return;
    }
    syncWorking = true;
    try {
      await enrollSync(syncUrl.trim());
      localStorage.setItem("manila.plaidProxyUrl", syncUrl.trim());
      syncConfigured = true;
      phase = "bank";
    } catch (e) {
      syncError = e instanceof Error ? e.message : String(e);
    } finally {
      syncWorking = false;
    }
  }

  function skipSync() {
    phase = "done";
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

    {#if phase === "sync"}
      <h1 class="step-heading">Set up sync</h1>
      <p class="step-body">
        Connect to a Manila sync server to enable multi-device access to your vault.
      </p>
      <div class="step-form">
        <Input
          type="url"
          placeholder="https://your-server.example.com"
          bind:value={syncUrl}
          disabled={syncWorking}
          autocomplete="off"
        />
        {#if syncError}
          <p class="field-error">{syncError}</p>
        {/if}
        <Button
          onclick={handleConnect}
          disabled={syncWorking}
          size="lg"
          class="action-btn"
        >
          {syncWorking ? "Connecting…" : "Connect & enroll"}
        </Button>
        <button class="skip-link" onclick={skipSync} disabled={syncWorking}>
          Skip for now
        </button>
      </div>
    {:else if phase === "bank"}
      <h1 class="step-heading">Connect a bank account</h1>
      <p class="step-body">
        Link your bank for automatic transaction import via Plaid.
      </p>
      <div class="step-form">
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
      </div>
    {:else}
      <h1 class="step-heading">You're all set!</h1>
      <p class="step-body">
        {#if syncConfigured && plaidConnected}
          Sync and bank connection are configured.
        {:else if syncConfigured}
          Sync is configured. Add bank connections any time from Accounts.
        {:else}
          Set up sync and bank connections any time from Settings and Accounts.
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

  .skip-link:disabled {
    opacity: 0.5;
    cursor: default;
  }

  :global(.action-btn) {
    width: 100%;
  }
</style>
