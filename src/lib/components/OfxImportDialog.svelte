<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { importOfx } from "$lib/import";
  import type { ImportResult } from "$lib/types/import";
  import type { Account } from "$lib/types/account";

  let {
    account,
    open = $bindable(false),
  }: {
    account: Account;
    open?: boolean;
  } = $props();

  let step = $state<1 | 2>(1);
  let fileName = $state("");
  let fileContent = $state("");
  let fileError = $state<string | null>(null);
  let fileInputEl = $state<HTMLInputElement | null>(null);

  let importing = $state(false);
  let importResult = $state<ImportResult | null>(null);
  let importError = $state<string | null>(null);

  $effect(() => {
    if (open) {
      step = 1;
      fileName = "";
      fileContent = "";
      fileError = null;
      importing = false;
      importResult = null;
      importError = null;
    }
  });

  function handleFileChange(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    fileName = file.name;
    fileError = null;
    const reader = new FileReader();
    reader.onload = (ev) => {
      fileContent = (ev.target?.result as string) ?? "";
      step = 2;
    };
    reader.readAsText(file);
  }

  async function handleImport() {
    if (importing) return;
    importing = true;
    importError = null;
    try {
      importResult = await importOfx(fileContent, account.id, fileName);
    } catch (e) {
      importError = e instanceof Error ? e.message : String(e);
    } finally {
      importing = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Import OFX / QFX</Dialog.Title>
    </Dialog.Header>

    {#if step === 1}
      <div class="step">
        <p class="context">
          Importing into <strong>{account.name}</strong>
        </p>
        {#if fileError}
          <p class="error">{fileError}</p>
        {/if}
        <input
          bind:this={fileInputEl}
          type="file"
          accept=".ofx,.qfx"
          style="display:none"
          onchange={handleFileChange}
        />
        <Button onclick={() => fileInputEl?.click()}>Choose OFX / QFX file</Button>
      </div>

    {:else if step === 2}
      <div class="step">
        {#if importResult}
          <p class="success">Import complete.</p>
          <p class="summary">
            {importResult.imported_count} transaction{importResult.imported_count === 1 ? "" : "s"} imported{importResult.skipped_count > 0
              ? `, ${importResult.skipped_count} skipped`
              : ""}.
          </p>
          {#if importResult.errors.length > 0}
            <ul class="error-list">
              {#each importResult.errors as err}
                <li>{err}</li>
              {/each}
            </ul>
          {/if}
          <Dialog.Footer>
            <Button onclick={() => (open = false)}>Done</Button>
          </Dialog.Footer>
        {:else}
          <p class="context">
            <strong>{fileName}</strong> into <strong>{account.name}</strong>
          </p>
          {#if importError}
            <p class="error">{importError}</p>
          {/if}
          <Dialog.Footer>
            <Button variant="ghost" onclick={() => (step = 1)} disabled={importing}>Back</Button>
            <Button onclick={handleImport} disabled={importing}>
              {importing ? "Importing..." : "Import transactions"}
            </Button>
          </Dialog.Footer>
        {/if}
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>

<style>
  .step {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .context {
    font-size: 13px;
    color: var(--muted-foreground);
    margin: 0;
  }

  .context strong {
    color: var(--text);
    font-weight: 500;
  }

  .error {
    font-size: 12px;
    color: var(--destructive);
    margin: 0;
    font-family: var(--font-mono);
  }

  .success {
    font-size: 13px;
    color: var(--positive);
    margin: 0;
    font-weight: 500;
  }

  .summary {
    font-size: 13px;
    color: var(--text);
    margin: 0;
  }

  .error-list {
    margin: 0;
    padding-left: 16px;
    font-size: 11px;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
  }

  .error-list li {
    margin-bottom: 4px;
  }
</style>
