<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { vaultStatus } from "$lib/stores/vault";
  import { validatePassword } from "$lib/lockscreen";
  import { markOnboardingPending } from "$lib/util/onboarding";
  import { Input } from "$lib/components/ui/input";
  import { Button } from "$lib/components/ui/button";

  let { onUnlocked }: { onUnlocked: () => void } = $props();

  type SetupPhase =
    | "password_entry"
    | "show_phrase"
    | "restore_phrase"
    | "enroll_phrase";

  let setupPhase = $state<SetupPhase>("password_entry");
  let password = $state("");
  let confirmPassword = $state("");
  let error = $state<string | null>(null);
  let submitting = $state(false);
  let biometricSubmitting = $state(false);
  let showPasswordForm = $state(false);
  let biometricLabel = $state("biometric unlock");

  onMount(async () => {
    biometricLabel = await invoke<string>("biometric_label");
  });

  let restorePhrase = $state("");
  let newPassword = $state("");
  let confirmNewPassword = $state("");

  let enrollPhrase = $state("");
  let enrollPassword = $state("");
  let enrollConfirmPassword = $state("");

  let phraseWords = $state<string[]>([]);
  let copySuccess = $state(false);

  const isInitializing = $derived($vaultStatus?.initialized === false);
  const failureMessage = $derived(
    isInitializing ? "Failed to create vault. Try again." : "Incorrect password.",
  );

  const biometricEnabled = $derived(
    !isInitializing && ($vaultStatus?.biometric_enabled ?? false),
  );

  async function handleSaveAndContinue() {
    submitting = true;
    try {
      await invoke("acknowledge_recovery_phrase");
      markOnboardingPending();
      onUnlocked();
    } catch {
      error = "Failed to complete setup. Please restart the app.";
    } finally {
      submitting = false;
    }
  }

  async function handleBiometricUnlock() {
    biometricSubmitting = true;
    error = null;
    try {
      await invoke("biometric_unlock");
      onUnlocked();
    } catch {
      error = "Biometric unlock failed. Use your password instead.";
      showPasswordForm = true;
    } finally {
      biometricSubmitting = false;
    }
  }

  async function beginPhraseCeremony() {
    const words = await invoke<string[]>("generate_recovery_phrase");
    phraseWords = words;
    setupPhase = "show_phrase";
  }

  async function handleSubmit(e: Event) {
    e.preventDefault();
    error = validatePassword(password, confirmPassword, isInitializing);
    if (error) return;

    submitting = true;
    try {
      if (isInitializing) {
        await invoke("create_vault", { password });
        await beginPhraseCeremony();
      } else {
        await invoke("unlock_vault", { password });
        onUnlocked();
      }
    } catch {
      error = failureMessage;
    } finally {
      submitting = false;
    }
  }

  async function handleRestoreSubmit(e: Event) {
    e.preventDefault();
    error = validatePassword(newPassword, confirmNewPassword, true);
    if (error) return;

    submitting = true;
    try {
      await invoke("restore_from_phrase", {
        phrase: restorePhrase.trim(),
        newPassword,
      });
      onUnlocked();
    } catch {
      error = "Recovery phrase not recognized.";
    } finally {
      submitting = false;
    }
  }

  function goToRestore() {
    error = null;
    restorePhrase = "";
    newPassword = "";
    confirmNewPassword = "";
    setupPhase = "restore_phrase";
  }

  function goToUnlock() {
    error = null;
    setupPhase = "password_entry";
  }

  function goToEnroll() {
    error = null;
    enrollPhrase = "";
    enrollPassword = "";
    enrollConfirmPassword = "";
    setupPhase = "enroll_phrase";
  }

  async function handleEnrollSubmit(e: Event) {
    e.preventDefault();
    error = validatePassword(enrollPassword, enrollConfirmPassword, true);
    if (error) return;
    if (!enrollPhrase.trim()) {
      error = "Recovery phrase is required.";
      return;
    }

    submitting = true;
    try {
      await invoke("enroll_vault_from_phrase", {
        phrase: enrollPhrase.trim(),
        password: enrollPassword,
      });
      onUnlocked();
    } catch {
      error = "Recovery phrase not recognized.";
    } finally {
      submitting = false;
    }
  }
</script>

{#snippet passwordEntryContent()}
  <h1 class="mode-label">
    {isInitializing ? "Set up your vault" : "Welcome back"}
  </h1>

  {#if biometricEnabled && !showPasswordForm}
    <div class="biometric-panel">
      <Button
        onclick={handleBiometricUnlock}
        disabled={biometricSubmitting}
        size="lg"
        class="unlock-btn"
      >
        {biometricSubmitting ? "Verifying…" : `Use ${biometricLabel}`}
      </Button>

      {#if error}
        <p class="error">{error}</p>
      {/if}

      <button
        class="forgot-link"
        onclick={() => { showPasswordForm = true; error = null; }}
      >
        Use password instead
      </button>
    </div>
  {:else}
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

      {#if !isInitializing}
        {#if biometricEnabled}
          <button class="forgot-link" onclick={() => { showPasswordForm = false; error = null; }}>
            Back to {biometricLabel}
          </button>
        {/if}
        <button class="forgot-link" onclick={goToRestore}>
          Forgot your password?
        </button>
      {:else}
        <button class="forgot-link" onclick={goToEnroll}>
          Enroll from another device
        </button>
      {/if}
    </form>
  {/if}
{/snippet}

{#snippet restorePhraseContent()}
  <h1 class="mode-label">Restore from recovery phrase</h1>
  <p class="phrase-instructions">
    Paste or type your 24-word recovery phrase, then set a new master password.
  </p>
  <form class="lock-form restore-form" onsubmit={handleRestoreSubmit}>
    <textarea
      class="phrase-textarea"
      placeholder="word1 word2 word3 ..."
      bind:value={restorePhrase}
      disabled={submitting}
      rows={4}
      spellcheck={false}
      autocomplete="off"
      autocapitalize="off"
    ></textarea>
    <div class="field">
      <Input
        type="password"
        placeholder="New master password"
        bind:value={newPassword}
        disabled={submitting}
        autocomplete="new-password"
      />
    </div>
    <div class="field">
      <Input
        type="password"
        placeholder="Confirm new password"
        bind:value={confirmNewPassword}
        disabled={submitting}
        autocomplete="new-password"
      />
    </div>
    <Button type="submit" size="lg" disabled={submitting || !restorePhrase.trim()}>
      Restore vault
    </Button>
    {#if error}
      <p class="error">{error}</p>
    {/if}
    <button class="forgot-link" onclick={goToUnlock}>
      Back to unlock
    </button>
  </form>
{/snippet}

{#snippet enrollPhraseContent()}
  <h1 class="mode-label">Enroll from another device</h1>
  <p class="phrase-instructions">
    Enter the 24-word phrase from your existing device, then set a master password for this device.
  </p>
  <form class="lock-form restore-form" onsubmit={handleEnrollSubmit}>
    <textarea
      class="phrase-textarea"
      placeholder="word1 word2 word3 ..."
      bind:value={enrollPhrase}
      disabled={submitting}
      rows={4}
      spellcheck={false}
      autocomplete="off"
      autocapitalize="off"
    ></textarea>
    <div class="field">
      <Input
        type="password"
        placeholder="New master password"
        bind:value={enrollPassword}
        disabled={submitting}
        autocomplete="new-password"
      />
    </div>
    <div class="field">
      <Input
        type="password"
        placeholder="Confirm password"
        bind:value={enrollConfirmPassword}
        disabled={submitting}
        autocomplete="new-password"
      />
    </div>
    <Button
      type="submit"
      size="lg"
      disabled={submitting || !enrollPhrase.trim()}
    >
      Enroll this device
    </Button>
    {#if error}
      <p class="error">{error}</p>
    {/if}
    <button class="forgot-link" onclick={goToUnlock}>
      Back to setup
    </button>
  </form>
{/snippet}

{#snippet showPhraseContent()}
  <h1 class="mode-label">Back up your recovery phrase</h1>
  <p class="phrase-instructions">
    Copy or download these 24 words. They are the only way to recover your vault if you forget your master password.
  </p>
  <textarea
    class="phrase-textarea phrase-display"
    readonly
    spellcheck={false}
    value={phraseWords.join(" ")}
    rows={4}
  ></textarea>
  <div class="phrase-actions">
    <Button
      variant="outline"
      size="sm"
      onclick={async () => {
        await navigator.clipboard.writeText(phraseWords.join(" "));
        copySuccess = true;
        setTimeout(() => (copySuccess = false), 2000);
      }}
    >
      {copySuccess ? "Copied!" : "Copy"}
    </Button>
    <Button
      variant="outline"
      size="sm"
      onclick={() => {
        const blob = new Blob([phraseWords.join(" ")], { type: "text/plain" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = "manila-recovery-phrase.txt";
        a.click();
        URL.revokeObjectURL(url);
      }}
    >
      Download
    </Button>
  </div>
  {#if error}
    <p class="error">{error}</p>
  {/if}
  <Button onclick={handleSaveAndContinue} size="lg" class="proceed-btn" disabled={submitting}>
    I've saved my recovery phrase
  </Button>
{/snippet}

<div class="lock-screen">
  <div class="lock-card" class:wide={setupPhase !== "password_entry"}>
    <div class="wordmark">manila</div>

    {#if setupPhase === "password_entry"}
      {@render passwordEntryContent()}
    {:else if setupPhase === "restore_phrase"}
      {@render restorePhraseContent()}
    {:else if setupPhase === "enroll_phrase"}
      {@render enrollPhraseContent()}
    {:else if setupPhase === "show_phrase"}
      {@render showPhraseContent()}
    {/if}
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

  .lock-card.wide {
    width: 560px;
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

  .phrase-instructions {
    font-size: 13px;
    color: var(--muted-foreground);
    text-align: center;
    margin: 0;
    line-height: 1.5;
    max-width: 440px;
  }

  .phrase-display {
    user-select: text;
    cursor: text;
  }

  .phrase-actions {
    display: flex;
    gap: 8px;
    width: 100%;
  }

  :global(.proceed-btn) {
    width: 100%;
  }

  .forgot-link {
    background: none;
    border: none;
    padding: 0;
    font-size: 12px;
    color: var(--muted-foreground);
    cursor: pointer;
    text-align: center;
  }

  .forgot-link:hover {
    color: var(--foreground);
  }

  .restore-form {
    width: 100%;
  }

  .phrase-textarea {
    width: 100%;
    background: var(--background);
    border: 1px solid var(--border);
    color: var(--foreground);
    font-family: var(--font-mono);
    font-size: 13px;
    padding: 10px 12px;
    resize: vertical;
    box-sizing: border-box;
    line-height: 1.6;
  }

  .phrase-textarea:focus {
    outline: none;
    border-color: var(--primary);
  }
</style>
