<script lang="ts">
  import { onMount } from "svelte";
  import { listAccounts } from "$lib/accounts";
  import { listPlaidConnections, deletePlaidConnection } from "$lib/plaid";
  import type { Account } from "$lib/types/account";
  import type { PlaidConnection } from "$lib/types/plaid";
  import { Button } from "$lib/components/ui/button";
  import AccountDialog from "$lib/components/AccountDialog.svelte";
  import CsvImportDialog from "$lib/components/CsvImportDialog.svelte";
  import OfxImportDialog from "$lib/components/OfxImportDialog.svelte";
  import PlaidLinkDialog from "$lib/components/PlaidLinkDialog.svelte";

  let accounts = $state<Account[]>([]);
  let dialogOpen = $state(false);
  let editingAccount = $state<Account | undefined>(undefined);
  let importDialogOpen = $state(false);
  let importingAccount = $state<Account | undefined>(undefined);
  let ofxDialogOpen = $state(false);
  let ofxImportingAccount = $state<Account | undefined>(undefined);
  let plaidConnections = $state<PlaidConnection[]>([]);
  let plaidError = $state<string | null>(null);
  let plaidLinkOpen = $state(false);

  async function loadAccounts() {
    try {
      accounts = await listAccounts();
    } catch {
      // Silently empty when running without Tauri (pnpm dev)
    }
  }

  async function loadConnections() {
    try {
      plaidConnections = await listPlaidConnections();
    } catch {
      // Silently empty when running without Tauri (pnpm dev)
    }
  }

  async function disconnect(id: string) {
    plaidError = null;
    try {
      await deletePlaidConnection(id);
      await loadConnections();
    } catch (e) {
      plaidError = e instanceof Error ? e.message : String(e);
    }
  }

  function accountCount(accountIds: string): number {
    try {
      return (JSON.parse(accountIds) as unknown[]).length;
    } catch {
      return 0;
    }
  }

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleDateString();
  }

  onMount(() => {
    loadAccounts();
    loadConnections();
  });

  function openCreate() {
    editingAccount = undefined;
    dialogOpen = true;
  }

  function openEdit(account: Account) {
    editingAccount = account;
    dialogOpen = true;
  }

  function openImport(account: Account) {
    importingAccount = account;
    importDialogOpen = true;
  }

  function openOfxImport(account: Account) {
    ofxImportingAccount = account;
    ofxDialogOpen = true;
  }
</script>

<AccountDialog
  account={editingAccount}
  bind:open={dialogOpen}
  onsaved={loadAccounts}
/>

<PlaidLinkDialog bind:open={plaidLinkOpen} onsuccess={loadConnections} />

{#if importingAccount}
  <CsvImportDialog account={importingAccount} bind:open={importDialogOpen} />
{/if}

{#if ofxImportingAccount}
  <OfxImportDialog account={ofxImportingAccount} bind:open={ofxDialogOpen} />
{/if}

<div class="page">
  <header class="page-header">
    <h1>Accounts</h1>
    <div class="header-actions">
      <Button variant="ghost" onclick={() => (plaidLinkOpen = true)}>Connect a bank</Button>
      <Button onclick={openCreate}>New account</Button>
    </div>
  </header>

  {#if accounts.length === 0}
    <div class="empty-state">
      <p>No accounts yet. Add one to get started.</p>
    </div>
  {:else}
    <div class="table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th>Name</th>
            <th>Type</th>
            <th>Subtype</th>
            <th>Institution</th>
            <th>Currency</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each accounts as account (account.id)}
            <tr>
              <td class="name">{account.name}</td>
              <td>{account.account_type}</td>
              <td>{account.subtype}</td>
              <td>{account.institution}</td>
              <td class="mono">{account.currency}</td>
              <td class="actions">
                <button class="edit-btn" onclick={() => openImport(account)}>Import CSV</button>
                <button class="edit-btn" onclick={() => openOfxImport(account)}>Import OFX</button>
                <button class="edit-btn" onclick={() => openEdit(account)}>Edit</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  <div class="connected-banks">
    <h2>Connected banks</h2>
    {#if plaidError}
      <p class="plaid-error">{plaidError}</p>
    {/if}
    {#if plaidConnections.length === 0}
      <div class="connections-empty">
        <p>No connected banks. Connections will appear here after linking a bank.</p>
      </div>
    {:else}
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>Institution</th>
              <th>Accounts</th>
              <th>Connected</th>
              <th>Sync status</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {#each plaidConnections as conn (conn.id)}
              <tr>
                <td class="name">{conn.institution_name}</td>
                <td>{accountCount(conn.account_ids)}</td>
                <td>{formatDate(conn.connected_at)}</td>
                <td class="sync-status">{conn.last_synced_at ? "Synced" : "Never synced"}</td>
                <td class="actions">
                  <button class="edit-btn disconnect" onclick={() => disconnect(conn.id)}>
                    Disconnect
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 24px;
    background: var(--surface-raised);
    border-bottom: 1px solid var(--border);
  }

  .header-actions {
    display: flex;
    gap: 8px;
  }

  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    letter-spacing: 0.04em;
  }

  .empty-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .empty-state p {
    color: var(--faint);
    font-size: 13px;
  }

  .table-wrap {
    flex: 1;
    overflow: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }

  thead {
    position: sticky;
    top: 0;
    background: var(--surface-raised);
    z-index: 1;
  }

  td {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text);
  }

  tr:hover td {
    background: var(--surface-hover);
  }

  .name {
    font-weight: 500;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .actions {
    text-align: right;
  }

  .edit-btn {
    background: none;
    border: none;
    color: var(--muted-foreground);
    font-size: 11px;
    font-family: var(--font-display);
    cursor: pointer;
    padding: 4px 8px;
    letter-spacing: 0.03em;
  }

  .edit-btn:hover {
    color: var(--accent);
  }

  .disconnect:hover {
    color: var(--destructive, #e05252);
  }

  .connected-banks {
    border-top: 1px solid var(--border);
    padding: 16px 24px 0;
  }

  .connected-banks h2 {
    margin: 0 0 12px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    letter-spacing: 0.04em;
  }

  .connections-empty {
    padding: 24px 0;
    text-align: center;
  }

  .connections-empty p {
    color: var(--faint);
    font-size: 13px;
  }

  .plaid-error {
    font-size: 12px;
    color: var(--destructive, #e05252);
    margin: 0 0 8px;
  }

  .sync-status {
    color: var(--muted-foreground);
    font-size: 11px;
  }
</style>
