<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { parseCsvPreview, previewCsvImport, importCsv } from "$lib/import";
  import type { CsvPreview, ColumnMapping, ImportResult, PendingImport, ImportDecision } from "$lib/types/import";
  import type { Account } from "$lib/types/account";
  import ImportReviewStep from "$lib/components/ImportReviewStep.svelte";

  type ColRole = "ignore" | "date" | "description" | "amount" | "debit" | "credit";
  type Mode = "single" | "split";

  let {
    account,
    open = $bindable(false),
  }: {
    account: Account;
    open?: boolean;
  } = $props();

  // Steps: 1=pick file, 2=map columns, 3=confirm, 4=review uncertain, 5=result
  let step = $state<1 | 2 | 3 | 4 | 5>(1);
  let fileName = $state("");
  let fileContent = $state("");
  let preview = $state<CsvPreview | null>(null);
  let previewError = $state<string | null>(null);
  let fileInputEl = $state<HTMLInputElement | null>(null);

  let colAssignments = $state<Record<string, ColRole>>({});
  let mode = $state<Mode>("single");
  let flipSign = $state(false);

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
      preview = null;
      previewError = null;
      colAssignments = {};
      mode = "single";
      flipSign = false;
      previewing = false;
      pendingImport = null;
      decisions = new Map();
      importing = false;
      importResult = null;
      importError = null;
    }
  });

  // --- Auto-detection ---

  const DATE_HEADERS = new Set([
    "date", "transaction date", "posted date", "post date", "trans date",
    "settlement date", "trans. date", "posting date",
  ]);
  const DESC_HEADERS = new Set([
    "description", "memo", "payee", "merchant", "narrative", "details",
    "name", "transaction description", "transaction detail", "particulars",
  ]);
  const AMOUNT_HEADERS = new Set(["amount", "transaction amount", "amt", "transaction amt"]);
  const DEBIT_HEADERS = new Set([
    "debit", "debit amount", "withdrawal", "withdrawals", "dr", "debit (usd)", "amount debited",
  ]);
  const CREDIT_HEADERS = new Set([
    "credit", "credit amount", "deposit", "deposits", "cr", "credit (usd)", "amount credited",
  ]);

  function autoDetect(headers: string[]): { assignments: Record<string, ColRole>; detectedMode: Mode } {
    const result: Record<string, ColRole> = Object.fromEntries(
      headers.map((h) => [h, "ignore" as ColRole]),
    );
    const taken = new Set<ColRole>();

    for (const h of headers) {
      const lower = h.toLowerCase().trim();
      let role: ColRole = "ignore";
      if (!taken.has("date") && DATE_HEADERS.has(lower)) role = "date";
      else if (!taken.has("description") && DESC_HEADERS.has(lower)) role = "description";
      else if (!taken.has("amount") && AMOUNT_HEADERS.has(lower)) role = "amount";
      else if (!taken.has("debit") && DEBIT_HEADERS.has(lower)) role = "debit";
      else if (!taken.has("credit") && CREDIT_HEADERS.has(lower)) role = "credit";
      result[h] = role;
      if (role !== "ignore") taken.add(role);
    }

    const detectedMode: Mode =
      taken.has("debit") && taken.has("credit") && !taken.has("amount") ? "split" : "single";

    return { assignments: result, detectedMode };
  }

  // --- Derived column lookups ---

  const roleOf = (header: string): ColRole => colAssignments[header] ?? "ignore";

  const dateCol = $derived(
    Object.entries(colAssignments).find(([, r]) => r === "date")?.[0] ?? null,
  );
  const descCol = $derived(
    Object.entries(colAssignments).find(([, r]) => r === "description")?.[0] ?? null,
  );
  const amountCol = $derived(
    Object.entries(colAssignments).find(([, r]) => r === "amount")?.[0] ?? null,
  );
  const debitCol = $derived(
    Object.entries(colAssignments).find(([, r]) => r === "debit")?.[0] ?? null,
  );
  const creditCol = $derived(
    Object.entries(colAssignments).find(([, r]) => r === "credit")?.[0] ?? null,
  );

  const canProceed = $derived(
    dateCol !== null &&
      descCol !== null &&
      (mode === "single" ? amountCol !== null : debitCol !== null && creditCol !== null),
  );

  const detectedLabels = $derived.by(() => {
    const roles = Object.values(colAssignments);
    const required =
      mode === "single"
        ? (["date", "description", "amount"] as ColRole[])
        : (["date", "description", "debit", "credit"] as ColRole[]);
    return {
      found: required.filter((r) => roles.includes(r)),
      missing: required.filter((r) => !roles.includes(r)),
    };
  });

  // Mirror Rust sign logic: debit = outflow (negative), credit = inflow (positive).
  // Debit takes priority when both are non-empty, matching parse_row in csv.rs.
  function resolveSampleAmount(amount: string, debit: string, credit: string): string {
    if (amount) return amount;
    if (debit) return `-${debit}`;
    if (credit) return `+${credit}`;
    return "";
  }

  const parsedSample = $derived.by(() => {
    if (!preview) return [];
    const hdrs = preview.headers;
    const di = dateCol !== null ? hdrs.indexOf(dateCol) : -1;
    const xi = descCol !== null ? hdrs.indexOf(descCol) : -1;
    const ai = mode === "single" && amountCol !== null ? hdrs.indexOf(amountCol) : -1;
    const bi = mode === "split" && debitCol !== null ? hdrs.indexOf(debitCol) : -1;
    const ci = mode === "split" && creditCol !== null ? hdrs.indexOf(creditCol) : -1;

    return preview.sample_rows.map((row) => ({
      date: di >= 0 ? (row[di] ?? "") : "",
      description: xi >= 0 ? (row[xi] ?? "") : "",
      amount: resolveSampleAmount(
        ai >= 0 ? (row[ai] ?? "") : "",
        bi >= 0 ? (row[bi] ?? "") : "",
        ci >= 0 ? (row[ci] ?? "") : "",
      ),
    }));
  });

  const estimatedRowCount = $derived(
    fileContent ? fileContent.split("\n").filter((l) => l.trim()).length - 1 : 0,
  );

  // --- Handlers ---

  function setRole(header: string, role: ColRole) {
    const updated: Record<string, ColRole> = {};
    for (const [h, r] of Object.entries(colAssignments)) {
      updated[h] = r === role && h !== header ? "ignore" : r;
    }
    updated[header] = role;
    colAssignments = updated;
  }

  function changeMode(next: Mode) {
    if (next === mode) return;
    const updated: Record<string, ColRole> = {};
    for (const [h, r] of Object.entries(colAssignments)) {
      const incompatible =
        (next === "single" && (r === "debit" || r === "credit")) ||
        (next === "split" && r === "amount");
      updated[h] = incompatible ? "ignore" : r;
    }
    colAssignments = updated;
    if (next === "split") flipSign = false;
    mode = next;
  }

  function handleFileChange(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    fileName = file.name;
    previewError = null;
    if (file.size > 5 * 1024 * 1024) {
      previewError = "File is too large (max 5 MB). Export a smaller date range.";
      return;
    }
    const reader = new FileReader();
    reader.onload = (ev) => {
      fileContent = (ev.target?.result as string) ?? "";
      loadPreview();
    };
    reader.onerror = () => {
      previewError = "Could not read the selected file.";
    };
    reader.readAsText(file);
  }

  async function loadPreview() {
    try {
      const p = await parseCsvPreview(fileContent);
      preview = p;

      const { assignments, detectedMode } = autoDetect(p.headers);
      colAssignments = assignments;
      mode = detectedMode;

      const roles = Object.values(assignments);
      const allFound =
        roles.includes("date") &&
        roles.includes("description") &&
        (detectedMode === "single" ? roles.includes("amount") : roles.includes("debit") && roles.includes("credit"));

      step = allFound ? 3 : 2;
    } catch (e) {
      previewError = e instanceof Error ? e.message : String(e);
    }
  }

  function buildMapping(): ColumnMapping {
    return {
      date_col: dateCol!,
      description_col: descCol!,
      amount_col: mode === "single" ? amountCol : null,
      flip_sign: mode === "single" ? flipSign : false,
      debit_col: mode === "split" ? debitCol : null,
      credit_col: mode === "split" ? creditCol : null,
    };
  }

  async function handleConfirm() {
    if (previewing) return;
    previewing = true;
    importError = null;

    try {
      const result = await previewCsvImport(fileContent, buildMapping(), account.id);
      pendingImport = result;

      if (result.new_count === 0 && result.uncertain.length === 0) {
        // All exact dups, nothing to write.
        importResult = {
          batch_id: "",
          imported_count: 0,
          skipped_count: result.exact_duplicate_count,
          errors: result.errors,
        };
        step = 5;
      } else if (result.uncertain.length > 0) {
        decisions = new Map();
        step = 4;
      } else {
        // No uncertain matches, proceed directly to commit.
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
      importResult = await importCsv(fileContent, buildMapping(), account.id, fileName, decisionsArr);
      step = 5;
    } catch (e) {
      importError = e instanceof Error ? e.message : String(e);
    } finally {
      importing = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-2xl">
    <Dialog.Header>
      <Dialog.Title>
        {#if step === 1}Import CSV{:else if step === 2}Check column mapping{:else if step === 3}Review & import{:else if step === 4}Review duplicates{:else}Import result{/if}
      </Dialog.Title>
    </Dialog.Header>

    {#if step === 1}
      <div class="step">
        <p class="context">
          Importing into <strong>{account.name}</strong>
        </p>
        {#if previewError}
          <p class="error">{previewError}</p>
        {/if}
        <input
          bind:this={fileInputEl}
          type="file"
          accept=".csv"
          style="display:none"
          onchange={handleFileChange}
        />
        <Button onclick={() => fileInputEl?.click()}>Choose CSV file</Button>
      </div>

    {:else if step === 2 && preview}
      <div class="step">
        <p class="filename">{fileName}</p>

        <div class="detection-status">
          {#if detectedLabels.found.length > 0}
            <span class="found">Detected: {detectedLabels.found.join(", ")}</span>
          {/if}
          {#if detectedLabels.missing.length > 0}
            <span class="missing">Still needed: {detectedLabels.missing.join(", ")}</span>
          {/if}
        </div>

        <div class="mode-row">
          <label class="radio-label">
            <input
              type="radio"
              name="csv-mode"
              value="single"
              checked={mode === "single"}
              onchange={() => changeMode("single")}
            />
            Single amount column
          </label>
          <label class="radio-label">
            <input
              type="radio"
              name="csv-mode"
              value="split"
              checked={mode === "split"}
              onchange={() => changeMode("split")}
            />
            Debit / Credit columns
          </label>
          {#if mode === "single"}
            <label class="radio-label flip">
              <input type="checkbox" bind:checked={flipSign} />
              Flip sign (debits exported as positive)
            </label>
          {/if}
        </div>

        <div class="table-scroll">
          <table>
            <thead>
              <tr>
                {#each preview.headers as header}
                  <th class="select-cell">
                    <select
                      value={roleOf(header)}
                      onchange={(e) =>
                        setRole(header, (e.target as HTMLSelectElement).value as ColRole)}
                    >
                      <option value="ignore">Ignore</option>
                      <option value="date">Date</option>
                      <option value="description">Description</option>
                      {#if mode === "single"}
                        <option value="amount">Amount</option>
                      {:else}
                        <option value="debit">Debit</option>
                        <option value="credit">Credit</option>
                      {/if}
                    </select>
                  </th>
                {/each}
              </tr>
              <tr class="col-names">
                {#each preview.headers as header}
                  <th>{header}</th>
                {/each}
              </tr>
            </thead>
            <tbody>
              {#each preview.sample_rows as row}
                <tr>
                  {#each row as cell}
                    <td>{cell}</td>
                  {/each}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>

        <Dialog.Footer>
          <Button variant="ghost" onclick={() => (step = 1)}>Back</Button>
          <Button onclick={() => (step = 3)} disabled={!canProceed}>Next</Button>
        </Dialog.Footer>
      </div>

    {:else if step === 3}
      <div class="step">
        <div class="confirm-header">
          <p class="context">
            ~{estimatedRowCount} transaction{estimatedRowCount === 1 ? "" : "s"} from
            <strong>{fileName}</strong> into <strong>{account.name}</strong>
          </p>
          <button class="remap-link" onclick={() => (step = 2)}>Change column mapping</button>
        </div>

        {#if parsedSample.length > 0}
          <div class="table-scroll">
            <table>
              <thead>
                <tr class="col-names">
                  <th>Date</th>
                  <th>Description</th>
                  <th class="right">Amount</th>
                </tr>
              </thead>
              <tbody>
                {#each parsedSample as row}
                  <tr>
                    <td class="mono">{row.date}</td>
                    <td>{row.description}</td>
                    <td class="mono right">{row.amount}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
          <p class="sample-note">Showing first {parsedSample.length} rows</p>
        {/if}

        {#if importError}
          <p class="error">{importError}</p>
        {/if}

        <Dialog.Footer>
          <Button variant="ghost" onclick={() => (step = 2)} disabled={previewing}>Back</Button>
          <Button onclick={handleConfirm} disabled={previewing}>
            {previewing ? "Checking for duplicates..." : "Import transactions"}
          </Button>
        </Dialog.Footer>
      </div>

    {:else if step === 4 && pendingImport}
      <div class="step">
        <ImportReviewStep uncertain={pendingImport.uncertain} bind:decisions />

        {#if importError}
          <p class="error">{importError}</p>
        {/if}

        <Dialog.Footer>
          <Button variant="ghost" onclick={() => (step = 3)} disabled={importing}>Back</Button>
          <Button onclick={handleReviewConfirm} disabled={importing}>
            {importing ? "Importing..." : "Confirm & import"}
          </Button>
        </Dialog.Footer>
      </div>

    {:else if step === 5}
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
              {importResult.imported_count} row{importResult.imported_count === 1 ? "" : "s"} imported{importResult.skipped_count > 0
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

  .filename {
    font-size: 11px;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    margin: 0;
  }

  .detection-status {
    display: flex;
    gap: 16px;
    font-size: 11px;
    flex-wrap: wrap;
  }

  .detection-status .found {
    color: var(--positive);
  }

  .detection-status .missing {
    color: var(--muted-foreground);
  }

  .mode-row {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    align-items: center;
  }

  .radio-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
  }

  .radio-label.flip {
    color: var(--muted-foreground);
  }

  .table-scroll {
    overflow-x: auto;
    border: 1px solid var(--border);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 11px;
  }

  .select-cell {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--surface-raised);
  }

  select {
    width: 100%;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    font-size: 11px;
    font-family: var(--font-display);
    padding: 3px 4px;
    cursor: pointer;
  }

  select:focus {
    outline: 1px solid var(--accent);
  }

  .col-names th {
    padding: 5px 8px;
    background: var(--surface-raised);
    border-bottom: 1px solid var(--border);
    color: var(--muted-foreground);
    font-weight: 500;
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .col-names th.right {
    text-align: right;
  }

  tbody tr:hover td {
    background: var(--surface-hover);
  }

  tbody td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text);
    white-space: nowrap;
  }

  tbody td.mono {
    font-family: var(--font-mono);
  }

  tbody td.right {
    text-align: right;
  }

  .confirm-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .remap-link {
    background: none;
    border: none;
    color: var(--muted-foreground);
    font-size: 11px;
    font-family: var(--font-display);
    cursor: pointer;
    padding: 0;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .remap-link:hover {
    color: var(--accent);
  }

  .sample-note {
    font-size: 10px;
    color: var(--faint);
    margin: 0;
    text-align: right;
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
