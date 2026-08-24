export interface CsvPreview {
  headers: string[];
  sample_rows: string[][];
}

export interface ColumnMapping {
  date_col: string;
  description_col: string;
  amount_col: string | null;
  flip_sign: boolean;
  debit_col: string | null;
  credit_col: string | null;
}

export interface ImportResult {
  batch_id: string;
  imported_count: number;
  skipped_count: number;
  errors: string[];
}

export interface UncertainMatch {
  candidate_source_id: string;
  candidate_date: string;
  candidate_amount_cents: number;
  candidate_description: string;
  existing_raw_record_id: string;
  existing_source_id: string;
}

export interface PendingImport {
  new_count: number;
  exact_duplicate_count: number;
  uncertain: UncertainMatch[];
  errors: string[];
}

export interface ImportDecision {
  candidate_source_id: string;
  accept_as_duplicate: boolean;
}
