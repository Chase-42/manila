<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { previewOfxImport, importOfx } from "$lib/import";
  import type { ImportResult, PendingImport, ImportDecision } from "$lib/types/import";
  import type { Account } from "$lib/types/account";
  import ImportReviewStep from "$lib/components/ImportReviewStep.svelte";

  let {
    account,
    open = $bindable(false),
  }: {
    account: Account;
    open?: boolean;
  } = $props();

  // Steps: 1=pick file, 2=confirm, 3=review uncertain, 4=result
  let step = $state<1 | 2 | 3 | 4>(1);
  let fileName = $state("");
  let fileContent = $state("");
  let fileError = $state<string | null>(null);
  let fileInputEl = $state<HTMLInputElement | null>(null);

  let previewing = $state(false);
  let pendingImport = $state<PendingImport | null>(null);
  let decisions = $state(new Map<string, boolean>());

  let importing = $state(false);
  let importResult = $state<ImportResult | null>(null);
  let importError = $state<string | null>(null);

  $effect(() => {
    if (open) {
      step = 1;
      fileName = "";
      fileContent = "";
      fileError = null;
      previewing = false;
      pendingImport = null;
      decisions = new Map();
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
    if (file.size > 5 * 1024 * 1024) {
      fileError = "File is too large (max 5 MB). Export a smaller date range.";
      return;
    }
    const reader = new FileReader();
    reader.onload = (ev) => {
      fileContent = (ev.target?.result as string) ?? "";
      step = 2;
    };
    reader.onerror = () => {
      fileError = "Could not read the selected file.";
    };
    reader.readAsText(file);
  }

  async function handleConfirm() {
    if (previewing) return;
    previewing = true;
    importError = null;
    try {
      const result = await previewOfxImport(fileContent, account.id);
      pendingImport = result;

      if (result.new_count === 0 && result.uncertain.length === 0) {
        importResult = {
          batch_id: "",
          imported_count: 0,
          skipped_count: result.exact_duplicate_count,
          errors: result.errors,
        };
        step = 4;
      } else if (result.uncertain.length > 0) {
        decisions = new Map();
        step = 3;
      } else {
        await runImport([]);
      }
    } catch (e) {
      importError = e instanceof Error ? e.message : String(e);
    } finally {
      previewing = false;
    }
  }

  async function handleReviewConfirm() {
    const decisionsArr: ImportDecision[] = [];
    for (const [sourceId, isDuplicate] of decisions) {
      decisionsArr.push({ candidate_source_id: sourceId, accept_as_duplicate: isDuplicate });
    }
    await runImport(decisionsArr);
  }

  async function runImport(decisionsArr: ImportDecision[]) {
    if (importing) return;
    importing = true;
    importError = null;
    try {
      importResult = await importOfx(fileContent, account.id, fileName, decisionsArr);
      step = 4;
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
        <p class="context">
          <strong>{fileName}</strong> into <strong>{account.name}</strong>
        </p>
        {#if importError}
          <p class="error">{importError}</p>
        {/if}
        <Dialog.Footer>
          <Button variant="ghost" onclick={() => (step = 1)} disabled={previewing}>Back</Button>
          <Button onclick={handleConfirm} disabled={previewing}>
            {previewing ? "Checking for duplicates..." : "Import transactions"}
          </Button>
        </Dialog.Footer>
      </div>

    {:else if step === 3 && pendingImport}
      <div class="step">
        <ImportReviewStep uncertain={pendingImport.uncertain} bind:decisions />

        {#if importError}
          <p class="error">{importError}</p>
        {/if}

        <Dialog.Footer>
          <Button variant="ghost" onclick={() => (step = 2)} disabled={importing}>Back</Button>
          <Button onclick={handleReviewConfirm} disabled={importing}>
            {importing ? "Importing..." : "Confirm & import"}
          </Button>
        </Dialog.Footer>
      </div>

    {:else if step === 4}
      <div class="step">
        {#if importResult}
          {#if importResult.imported_count === 0 && importResult.skipped_count > 0}
            <p class="muted">Nothing new to import.</p>
            <p class="summary">
              {importResult.skipped_count} duplicate{importResult.skipped_count === 1 ? "" : "s"} detected, all already in your ledger.
            </p>
          {:else}
            <p class="success">Import complete.</p>
            <p class="summary">
              {importResult.imported_count} transaction{importResult.imported_count === 1 ? "" : "s"} imported{importResult.skipped_count > 0
                ? `, ${importResult.skipped_count} skipped`
                : ""}.
            </p>
          {/if}
          {#if importResult.errors.length > 0}
            <ul class="error-list">
              {#each importResult.errors as err}
                <li>{err}</li>
              {/each}
            </ul>
          {/if}
        {/if}
        <Dialog.Footer>
          <Button onclick={() => (open = false)}>Done</Button>
        </Dialog.Footer>
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

  .muted {
    font-size: 13px;
    color: var(--muted-foreground);
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
