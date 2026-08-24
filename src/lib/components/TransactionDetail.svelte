<script lang="ts">
  import { upsertTransactionMeta } from '$lib/transactions';
  import { formatCents } from '$lib/money';
  import type { TransactionRow } from '$lib/types/transaction';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';

  interface Props {
    transaction: TransactionRow;
    open: boolean;
    onSaved: () => void;
  }

  let { transaction, open = $bindable(), onSaved }: Props = $props();

  let notes = $state('');
  let tagsInput = $state('');
  let reviewed = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);

  // Sync local state from the transaction prop when dialog opens.
  $effect(() => {
    if (open) {
      notes = transaction.notes;
      tagsInput = transaction.tags.join(', ');
      reviewed = transaction.reviewed;
      error = null;
    }
  });

  function parseTags(raw: string): string[] {
    return raw
      .split(',')
      .map((t) => t.trim())
      .filter((t) => t.length > 0);
  }

  async function save() {
    saving = true;
    error = null;
    try {
      await upsertTransactionMeta(transaction.id, notes, parseTags(tagsInput), reviewed);
      onSaved();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay />
    <Dialog.Content>
      <Dialog.Header>
        <Dialog.Title>Transaction</Dialog.Title>
        <Dialog.Description>
          {transaction.date} &middot; {transaction.account_name}
        </Dialog.Description>
      </Dialog.Header>

      <div class="detail-body">
        <div class="field read-only">
          <span class="label">Description</span>
          <span class="value">{transaction.description}</span>
        </div>
        <div class="field read-only">
          <span class="label">Amount</span>
          <span class="value mono" class:negative={transaction.amount_cents < 0}>
            {formatCents(transaction.amount_cents)}
          </span>
        </div>

        <div class="field">
          <label class="label" for="tx-notes">Notes</label>
          <textarea
            id="tx-notes"
            class="notes-input"
            bind:value={notes}
            rows="3"
            placeholder="Add a note..."
          ></textarea>
        </div>

        <div class="field">
          <label class="label" for="tx-tags">Tags</label>
          <input
            id="tx-tags"
            class="tags-input"
            type="text"
            bind:value={tagsInput}
            placeholder="groceries, work, travel"
          />
          <span class="hint">Comma-separated</span>
        </div>

        <div class="field checkbox-field">
          <label class="checkbox-label">
            <input type="checkbox" bind:checked={reviewed} />
            <span>Reviewed</span>
          </label>
        </div>
      </div>

      {#if error}
        <p class="error">{error}</p>
      {/if}

      <Dialog.Footer>
        <Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
        <Button onclick={save} disabled={saving}>
          {saving ? 'Saving...' : 'Save'}
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>

<style>
  .detail-body {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 4px 0 8px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .label {
    font-size: 11px;
    font-weight: 500;
    color: var(--muted-foreground);
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .value {
    font-size: 13px;
    color: var(--text);
  }

  .mono {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }

  .negative {
    color: var(--destructive);
  }

  .notes-input,
  .tags-input {
    width: 100%;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    font-size: 13px;
    font-family: inherit;
    padding: 8px 10px;
    resize: vertical;
    box-sizing: border-box;
  }

  .notes-input:focus,
  .tags-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .hint {
    font-size: 11px;
    color: var(--faint);
  }

  .checkbox-field {
    flex-direction: row;
    align-items: center;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text);
    cursor: pointer;
  }

  .error {
    font-size: 12px;
    color: var(--destructive);
    margin: 0;
  }
</style>
