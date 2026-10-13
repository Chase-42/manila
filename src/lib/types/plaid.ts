export interface PlaidConnection {
  id: string;
  plaid_item_id: string;
  institution_name: string;
  institution_id: string;
  account_ids: string;
  connected_at: string;
  last_synced_at: string | null;
}

export interface SyncResult {
  imported_count: number;
  accounts_created: number;
  skipped_count: number;
  errors: string[];
}
