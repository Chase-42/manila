<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { vaultStatus } from "$lib/stores/vault";
  import { validatePassword } from "$lib/lockscreen";
  import { generateWordChallenges, type Challenge } from "$lib/phraseChallenge";
  import { Input } from "$lib/components/ui/input";
  import { Button } from "$lib/components/ui/button";

  let { onUnlocked }: { onUnlocked: () => void } = $props();

  type SetupPhase =
    | "password_entry"
    | "show_phrase"
    | "verify_phrase"
    | "restore_phrase";

  let setupPhase = $state<SetupPhase>("password_entry");
  let password = $state("");
  let confirmPassword = $state("");
  let error = $state<string | null>(null);
  let submitting = $state(false);

  let restorePhrase = $state("");
  let newPassword = $state("");
  let confirmNewPassword = $state("");

  let phraseWords = $state<string[]>([]);
  let challenges = $state<Challenge[]>([]);
  let answers = $state<Record<number, string>>({});

  const isInitializing = $derived($vaultStatus?.initialized === false);
  const failureMessage = $derived(
    isInitializing ? "Failed to create vault. Try again." : "Incorrect password.",
  );

  const allChallengesCorrect = $derived(
    challenges.length === 3 &&
      challenges.every((c) => answers[c.position] === c.correctWord),
  );

  async function beginPhraseCeremony() {
    const words = await invoke<string[]>("generate_recovery_phrase");
    phraseWords = words;
    challenges = generateWordChallenges(words);
    answers = {};
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

  async function handleAcknowledge() {
    setupPhase = "verify_phrase";
  }

  async function handleVerifyComplete() {
    submitting = true;
    try {
      await invoke("acknowledge_recovery_phrase");
      onUnlocked();
    } catch {
      error = "Failed to complete setup. Please restart the app.";
    } finally {
      submitting = false;
    }
  }

  function selectAnswer(position: number, word: string) {
    answers = { ...answers, [position]: word };
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
</script>

{#snippet passwordEntryContent()}
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

    {#if !isInitializing}
      <button class="forgot-link" onclick={goToRestore}>
        Forgot your password?
      </button>
    {/if}
  </form>
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

{#snippet showPhraseContent()}
  <h1 class="mode-label">Back up your recovery phrase</h1>
  <p class="phrase-instructions">
    Write down these 24 words in order. They are the only way to recover your vault if you forget your master password.
  </p>
  <div class="phrase-grid">
    {#each phraseWords as word, i (i)}
      <div class="phrase-word">
        <span class="word-num">{i + 1}</span>
        <span class="word-text">{word}</span>
      </div>
    {/each}
  </div>
  <Button onclick={handleAcknowledge} size="lg" class="proceed-btn">
    I've written these down
  </Button>
{/snippet}

{#snippet verifyPhraseContent()}
  <h1 class="mode-label">Verify your recovery phrase</h1>
  <p class="phrase-instructions">Select the correct word for each position.</p>
  <div class="challenges">
    {#each challenges as challenge (challenge.position)}
      <div class="challenge">
        <p class="challenge-label">Word #{challenge.position}</p>
        <div class="challenge-options">
          {#each challenge.options as option (option)}
            <button
              class="option-btn"
              class:selected={answers[challenge.position] === option}
              class:correct={answers[challenge.position] === option && option === challenge.correctWord}
              class:wrong={answers[challenge.position] === option && option !== challenge.correctWord}
              onclick={() => selectAnswer(challenge.position, option)}
            >
              {option}
            </button>
          {/each}
        </div>
      </div>
    {/each}
  </div>
  {#if error}
    <p class="error">{error}</p>
  {/if}
  <Button
    onclick={handleVerifyComplete}
    size="lg"
    class="proceed-btn"
    disabled={!allChallengesCorrect || submitting}
  >
    Continue
  </Button>
{/snippet}

<div class="lock-screen">
  <div class="lock-card" class:wide={setupPhase !== "password_entry"}>
    <div class="wordmark">manila</div>

    {#if setupPhase === "password_entry"}
      {@render passwordEntryContent()}
    {:else if setupPhase === "restore_phrase"}
      {@render restorePhraseContent()}
    {:else if setupPhase === "show_phrase"}
      {@render showPhraseContent()}
    {:else if setupPhase === "verify_phrase"}
      {@render verifyPhraseContent()}
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

  .phrase-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
    width: 100%;
    background: var(--card);
    border: 1px solid var(--border);
    padding: 16px;
  }

  .phrase-word {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 6px 8px;
    background: var(--background);
    border: 1px solid var(--border);
  }

  .word-num {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--muted-foreground);
    min-width: 16px;
    text-align: right;
    user-select: none;
  }

  .word-text {
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--foreground);
    font-weight: 500;
  }

  .challenges {
    display: flex;
    flex-direction: column;
    gap: 20px;
    width: 100%;
  }

  .challenge {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .challenge-label {
    font-family: var(--font-display);
    font-size: 12px;
    font-weight: 600;
    color: var(--muted-foreground);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    margin: 0;
  }

  .challenge-options {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
  }

  .option-btn {
    padding: 10px 12px;
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 500;
    background: var(--card);
    border: 1px solid var(--border);
    color: var(--foreground);
    cursor: pointer;
    text-align: center;
    transition: border-color 0.1s, background 0.1s;
  }

  .option-btn:hover {
    border-color: var(--primary);
    background: color-mix(in srgb, var(--primary) 8%, transparent);
  }

  .option-btn.selected {
    border-color: var(--primary);
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    color: var(--primary);
  }

  .option-btn.correct {
    border-color: var(--primary);
    background: color-mix(in srgb, var(--primary) 16%, transparent);
    color: var(--primary);
  }

  .option-btn.wrong {
    border-color: var(--destructive);
    background: color-mix(in srgb, var(--destructive) 12%, transparent);
    color: var(--destructive);
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
