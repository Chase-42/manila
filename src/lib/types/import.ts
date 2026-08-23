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
