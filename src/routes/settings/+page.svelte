<script lang="ts">
  import { onMount } from "svelte";
  import { configureSync, getSyncStatus, runSync } from "$lib/sync";
  import type { SyncLastRun } from "$lib/generated/SyncLastRun";

  let serverUrl = $state("");
  let urlError = $state<string | null>(null);
  let saveSuccess = $state(false);

  let syncing = $state(false);
  let syncResult = $state<string | null>(null);
  let syncError = $state<string | null>(null);

  let lastRun = $state<SyncLastRun | null>(null);

  onMount(async () => {
    lastRun = await getSyncStatus().catch(() => null);
  });

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
</style>
