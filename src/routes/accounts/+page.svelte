<script lang="ts">
  import { onMount } from "svelte";
  import { listAccounts } from "$lib/accounts";
  import type { Account } from "$lib/types/account";
  import { Button } from "$lib/components/ui/button";
  import AccountDialog from "$lib/components/AccountDialog.svelte";

  let accounts = $state<Account[]>([]);
  let dialogOpen = $state(false);
  let editingAccount = $state<Account | undefined>(undefined);

  async function loadAccounts() {
    try {
      accounts = await listAccounts();
    } catch {
      // Silently empty when running without Tauri (pnpm dev)
    }
  }

  onMount(loadAccounts);

  function openCreate() {
    editingAccount = undefined;
    dialogOpen = true;
  }

  function openEdit(account: Account) {
    editingAccount = account;
    dialogOpen = true;
  }
</script>

<AccountDialog
  account={editingAccount}
  bind:open={dialogOpen}
  onsaved={loadAccounts}
/>

<div class="page">
  <header class="page-header">
    <h1>Accounts</h1>
    <Button onclick={openCreate}>New account</Button>
  </header>

  {#if accounts.length === 0}
    <div class="empty-state">
      <p>No accounts yet. Add one to get started.</p>
    </div>
  {:else}
    <div class="table-wrap">
      <table>
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
                <button class="edit-btn" onclick={() => openEdit(account)}>Edit</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
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

  th {
    padding: 8px 12px;
    text-align: left;
    color: var(--muted);
    font-weight: 500;
    font-size: 11px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    border-bottom: 1px solid var(--border);
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
    color: var(--muted);
    font-size: 11px;
    font-family: var(--font-display);
    cursor: pointer;
    padding: 4px 8px;
    letter-spacing: 0.03em;
  }

  .edit-btn:hover {
    color: var(--accent);
  }
</style>
