import { invoke } from '@tauri-apps/api/core';
import type { TransactionRow } from '$lib/generated/TransactionRow';

export async function listTransactions(): Promise<TransactionRow[]> {
  return invoke<TransactionRow[]>('list_transactions');
}

export async function searchTransactions(query: string): Promise<TransactionRow[]> {
  return invoke<TransactionRow[]>('search_transactions', { query });
}

export async function upsertTransactionMeta(
  transactionId: string,
  notes: string,
  tags: string[],
  reviewed: boolean
): Promise<void> {
  return invoke('upsert_transaction_meta', { transactionId, notes, tags, reviewed });
}

export async function exportTransactionsCsv(): Promise<string> {
  return invoke<string>('export_transactions_csv');
}
