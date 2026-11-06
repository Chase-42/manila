<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import QRCode from "qrcode";
  import { configureSync, getSyncConfig, getSyncStatus, runSync, revokeSync } from "$lib/sync";
  import type { SyncLastRun } from "$lib/generated/SyncLastRun";
  import { vaultStatus, refreshVaultStatus } from "$lib/stores/vault";
  import { validatePassword } from "$lib/lockscreen";

  let serverUrl = $state("");
  let urlError = $state<string | null>(null);
  let saveSuccess = $state(false);

  let syncing = $state(false);
  let syncResult = $state<string | null>(null);
  let syncError = $state<string | null>(null);

  let lastRun = $state<SyncLastRun | null>(null);

  let enrolled = $state(false);
  let revoking = $state(false);
  let revokeResult = $state<string | null>(null);
  let revokeError = $state<string | null>(null);

  let biometricAvailable = $state(false);
  let biometricLabel = $state("biometric unlock");
  let biometricWorking = $state(false);
  let biometricError = $state<string | null>(null);

  let currentPassword = $state("");
  let newPassword = $state("");
  let confirmNewPassword = $state("");
  let changePwWorking = $state(false);
  let changePwError = $state<string | null>(null);
  let changePwSuccess = $state(false);

  let enrollmentWords = $state<string[] | null>(null);
  let enrollmentError = $state<string | null>(null);
  let qrCanvas = $state<HTMLCanvasElement | null>(null);

  $effect(() => {
    if (qrCanvas && enrollmentWords) {
      QRCode.toCanvas(qrCanvas, enrollmentWords.join(" "), { width: 200, margin: 2 });
    }
  });

  onMount(async () => {
    lastRun = await getSyncStatus().catch(() => null);
    const cfg = await getSyncConfig().catch(() => null);
    enrolled = cfg?.enrolled ?? false;

    try {
      enrollmentWords = await invoke<string[]>("get_enrollment_phrase");
    } catch {
      enrollmentError = "Unlock the vault to view your enrollment code.";
    }

    biometricAvailable = await invoke<boolean>("check_biometric_availability").catch(() => false);
    biometricLabel = await invoke<string>("biometric_label").catch(() => "biometric unlock");
  });

  async function handleEnableBiometric() {
    biometricWorking = true;
    biometricError = null;
    try {
      await invoke("enable_biometric_unlock");
      await refreshVaultStatus();
    } catch (e) {
      biometricError = e instanceof Error ? e.message : String(e);
    } finally {
      biometricWorking = false;
    }
  }

  async function handleDisableBiometric() {
    biometricWorking = true;
    biometricError = null;
    try {
      await invoke("disable_biometric_unlock");
      await refreshVaultStatus();
    } catch (e) {
      biometricError = e instanceof Error ? e.message : String(e);
    } finally {
      biometricWorking = false;
    }
  }

  async function handleRevoke() {
    revoking = true;
    revokeResult = null;
    revokeError = null;
    try {
      await revokeSync();
      enrolled = false;
      revokeResult = "Enrollment revoked. All devices are now disconnected from the sync server.";
    } catch (e) {
      revokeError = e instanceof Error ? e.message : String(e);
    } finally {
      revoking = false;
    }
  }

  function validateUrl(url: string): string | null {
    if (!url.trim()) return "Server URL is required.";
    if (!url.startsWith("http://") && !url.startsWith("https://"))
      return "URL must start with http:// or https://";
    return null;
  }

  async function handleSave() {
    urlError = null;
    saveSuccess = false;
    const err = validateUrl(serverUrl);
    if (err) {
      urlError = err;
      return;
    }
    try {
      await configureSync(serverUrl);
      saveSuccess = true;
    } catch (e) {
      urlError = e instanceof Error ? e.message : String(e);
    }
  }

  async function handleChangePassword(e: Event) {
    e.preventDefault();
    changePwError = null;
    changePwSuccess = false;
    const validationError = validatePassword(newPassword, confirmNewPassword, true);
    if (validationError) {
      changePwError = validationError;
      return;
    }
    changePwWorking = true;
    try {
      await invoke("change_password", { currentPassword, newPassword });
      changePwSuccess = true;
      currentPassword = "";
      newPassword = "";
      confirmNewPassword = "";
    } catch (e) {
      changePwError =
        String(e) === "incorrect password" ? "Incorrect password." : "Failed to change password.";
    } finally {
      changePwWorking = false;
    }
  }

  async function handleSyncNow() {
    syncing = true;
    syncResult = null;
    syncError = null;
    try {
      const status = await runSync();
      syncResult = `Pushed ${status.pushed_count} change(s), applied ${status.applied_count} remote change(s).`;
      lastRun = await getSyncStatus().catch(() => null);
    } catch (e) {
      syncError = e instanceof Error ? e.message : String(e);
    } finally {
      syncing = false;
    }
  }
</script>

<div class="settings-page">
  <h1>Settings</h1>

  <section class="section">
    <h2>Sync Server</h2>
    <div class="field">
      <label for="server-url">Server URL</label>
      <input
        id="server-url"
        type="text"
        placeholder="https://your-sync-server.example.com"
        bind:value={serverUrl}
      />
      {#if urlError}
        <p class="error">{urlError}</p>
      {/if}
      {#if saveSuccess}
        <p class="success">Server URL saved.</p>
      {/if}
    </div>
    <button onclick={handleSave} class="btn-primary">Save</button>
  </section>

  <section class="section">
    <h2>Sync Now</h2>
    <button onclick={handleSyncNow} disabled={syncing} class="btn-primary">
      {syncing ? "Syncing…" : "Sync Now"}
    </button>
    {#if syncResult}
      <p class="success">{syncResult}</p>
    {/if}
    {#if syncError}
      <p class="error">{syncError}</p>
    {/if}
    <p class="status-line">
      {#if lastRun}
        Last synced: {lastRun.last_synced_at}
      {:else}
        Never synced
      {/if}
    </p>
  </section>

  <section class="section">
    <h2>Enrollment Code</h2>
    {#if enrollmentError}
      <p class="muted">{enrollmentError}</p>
    {:else if enrollmentWords}
      <p class="phrase-instructions">
        On the new device, choose "Enroll from another device" and enter this phrase.
      </p>
      <div class="phrase-grid">
        {#each enrollmentWords as word, i (i)}
          <div class="phrase-word">
            <span class="word-num">{i + 1}</span>
            <span class="word-text">{word}</span>
          </div>
        {/each}
      </div>
      <canvas bind:this={qrCanvas} class="qr-canvas"></canvas>
    {/if}
  </section>

  <section class="section">
    <h2>Enrollment</h2>
    <p class="status-line">
      {enrolled ? "Connected to sync server." : "Not enrolled with a sync server."}
    </p>
    <button
      onclick={handleRevoke}
      disabled={!enrolled || revoking}
      class="btn-danger"
    >
      {revoking ? "Revoking…" : "Revoke enrollment"}
    </button>
    {#if revokeResult}
      <p class="success">{revokeResult}</p>
    {/if}
    {#if revokeError}
      <p class="error">{revokeError}</p>
    {/if}
  </section>

  <section class="section">
    <h2>Change password</h2>
    {#if !$vaultStatus?.unlocked}
      <p class="muted">Unlock the vault to change your password.</p>
    {:else}
      <form onsubmit={handleChangePassword}>
        <div class="field">
          <label for="current-password">Current password</label>
          <input
            id="current-password"
            type="password"
            bind:value={currentPassword}
            disabled={changePwWorking}
            autocomplete="current-password"
          />
        </div>
        <div class="field">
          <label for="new-password">New password</label>
          <input
            id="new-password"
            type="password"
            bind:value={newPassword}
            disabled={changePwWorking}
            autocomplete="new-password"
          />
        </div>
        <div class="field">
          <label for="confirm-new-password">Confirm new password</label>
          <input
            id="confirm-new-password"
            type="password"
            bind:value={confirmNewPassword}
            disabled={changePwWorking}
            autocomplete="new-password"
          />
        </div>
        {#if changePwError}
          <p class="error">{changePwError}</p>
        {/if}
        {#if changePwSuccess}
          <p class="success">Password changed.</p>
        {/if}
        <button type="submit" disabled={changePwWorking} class="btn-primary">
          {changePwWorking ? "Changing…" : "Change password"}
        </button>
      </form>
    {/if}
  </section>

  {#if biometricAvailable}
    <section class="section">
      <h2>Security</h2>
      {#if !$vaultStatus?.unlocked}
        <p class="muted">Unlock the vault to manage biometric unlock.</p>
      {:else}
        <p class="status-line">
          {$vaultStatus?.biometric_enabled
            ? `${biometricLabel} unlock is enabled.`
            : `${biometricLabel} unlock is disabled.`}
        </p>
        {#if $vaultStatus?.biometric_enabled}
          <button
            onclick={handleDisableBiometric}
            disabled={biometricWorking}
            class="btn-danger"
          >
            {biometricWorking ? "Disabling…" : "Disable biometric unlock"}
          </button>
        {:else}
          <button
            onclick={handleEnableBiometric}
            disabled={biometricWorking}
            class="btn-primary"
          >
            {biometricWorking ? "Enabling…" : `Enable ${biometricLabel} unlock`}
          </button>
        {/if}
        {#if biometricError}
          <p class="error">{biometricError}</p>
        {/if}
      {/if}
    </section>
  {/if}
</div>

<style>
  .settings-page {
    padding: 32px;
    max-width: 480px;
  }

  h1 {
    font-size: 20px;
    font-weight: 600;
    margin: 0 0 24px;
    color: var(--foreground);
  }

  .section {
    margin-bottom: 32px;
  }

  h2 {
    font-size: 14px;
    font-weight: 600;
    color: var(--muted-foreground);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin: 0 0 12px;
  }

  .field {
    margin-bottom: 12px;
  }

  label {
    display: block;
    font-size: 13px;
    color: var(--foreground);
    margin-bottom: 6px;
  }

  input[type="text"] {
    width: 100%;
    padding: 8px 10px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--foreground);
    font-size: 13px;
    box-sizing: border-box;
  }

  input[type="text"]:focus {
    outline: none;
    border-color: var(--ring);
  }

  .btn-primary {
    padding: 8px 16px;
    background: var(--primary);
    color: var(--primary-foreground);
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-primary:hover:not(:disabled) {
    opacity: 0.9;
  }

  .error {
    color: var(--destructive);
    font-size: 12px;
    margin: 6px 0 0;
  }

  .success {
    color: var(--success, #22c55e);
    font-size: 12px;
    margin: 6px 0 0;
  }

  .status-line {
    font-size: 12px;
    color: var(--muted-foreground);
    margin: 10px 0 0;
  }

  .muted {
    font-size: 12px;
    color: var(--muted-foreground);
    margin: 0;
  }

  .phrase-instructions {
    font-size: 13px;
    color: var(--muted-foreground);
    margin: 0 0 12px;
    line-height: 1.5;
  }

  .phrase-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
    width: 100%;
    background: var(--card);
    border: 1px solid var(--border);
    padding: 12px;
    margin-bottom: 12px;
  }

  .phrase-word {
    display: flex;
    align-items: baseline;
    gap: 5px;
    padding: 4px 6px;
    background: var(--background);
    border: 1px solid var(--border);
  }

  .word-num {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--muted-foreground);
    min-width: 14px;
    text-align: right;
    user-select: none;
  }

  .word-text {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--foreground);
    font-weight: 500;
  }

  .qr-canvas {
    display: block;
    border: 1px solid var(--border);
  }

  .btn-danger {
    padding: 8px 16px;
    background: var(--destructive);
    color: var(--destructive-foreground, #fff);
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }

  .btn-danger:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-danger:hover:not(:disabled) {
    opacity: 0.9;
  }
</style>
