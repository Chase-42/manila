export interface TransactionRow {
  id: string;
  account_id: string;
  account_name: string;
  date: string;
  amount_cents: number;
  description: string;
  notes: string;
  tags: string[];
  reviewed: boolean;
}
