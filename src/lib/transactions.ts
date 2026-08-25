import { invoke } from '@tauri-apps/api/core';
import type { TransactionRow } from '$lib/generated/TransactionRow';

export async function listTransactions(): Promise<TransactionRow[]> {
  return invoke<TransactionRow[]>('list_transactions');
}

export async function upsertTransactionMeta(
  transaction_id: string,
  notes: string,
  tags: string[],
  reviewed: boolean
): Promise<void> {
  return invoke('upsert_transaction_meta', { transaction_id, notes, tags, reviewed });
}
