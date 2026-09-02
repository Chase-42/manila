<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { vaultStatus } from "$lib/stores/vault";
  import { validatePassword } from "$lib/lockscreen";
  import { Input } from "$lib/components/ui/input";
  import { Button } from "$lib/components/ui/button";

  let { onUnlocked }: { onUnlocked: () => void } = $props();

  let password = $state("");
  let confirmPassword = $state("");
  let error = $state<string | null>(null);
  let submitting = $state(false);

  const isInitializing = $derived($vaultStatus?.initialized === false);
  const failureMessage = $derived(
    isInitializing ? "Failed to create vault. Try again." : "Incorrect password.",
  );

  async function handleSubmit(e: Event) {
    e.preventDefault();
    error = validatePassword(password, confirmPassword, isInitializing);
    if (error) return;

    submitting = true;
    try {
      await (isInitializing
        ? invoke("create_vault", { password })
        : invoke("unlock_vault", { password }));
      onUnlocked();
    } catch {
      error = failureMessage;
    } finally {
      submitting = false;
    }
  }
</script>

<div class="lock-screen">
  <div class="lock-card">
    <div class="wordmark">manila</div>
    <h1 class="mode-label">
      {isInitializing ? "Set up your vault" : "Welcome back"}
    </h1>

    <form class="lock-form" onsubmit={handleSubmit}>
      <div class="field">
        <Input
          type="password"
          placeholder="Master password"
          bind:value={password}
          disabled={submitting}
          autocomplete={isInitializing ? "new-password" : "current-password"}
        />
      </div>

      {#if isInitializing}
        <div class="field">
          <Input
            type="password"
            placeholder="Confirm password"
            bind:value={confirmPassword}
            disabled={submitting}
            autocomplete="new-password"
          />
        </div>
      {/if}

      <Button
        type="submit"
        class="unlock-btn"
        disabled={submitting}
        size="lg"
      >
        {isInitializing ? "Create vault" : "Unlock"}
      </Button>

      {#if error}
        <p class="error">{error}</p>
      {/if}
    </form>
  </div>
</div>

<style>
  .lock-screen {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    background: var(--background);
  }

  .lock-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 24px;
    width: 320px;
  }

  .wordmark {
    font-family: var(--font-display);
    font-size: 32px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--primary);
  }

  .mode-label {
    font-family: var(--font-display);
    font-size: 16px;
    font-weight: 500;
    color: var(--muted-foreground);
    margin: 0;
    letter-spacing: 0.02em;
  }

  .lock-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
  }

  .field {
    width: 100%;
  }

  .error {
    font-size: 12px;
    color: var(--destructive);
    margin: 0;
    text-align: center;
  }
</style>
