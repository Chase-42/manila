import { invoke } from "@tauri-apps/api/core";
import type { ColumnMapping, CsvPreview, ImportResult } from "./types/import";

export async function parseCsvPreview(content: string): Promise<CsvPreview> {
  try {
    return await invoke<CsvPreview>("parse_csv_preview", { content });
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}

export async function importCsv(
  content: string,
  mapping: ColumnMapping,
  accountId: string,
  filename: string,
): Promise<ImportResult> {
  try {
    return await invoke<ImportResult>("import_csv", {
      content,
      mapping,
      accountId,
      filename,
    });
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}
