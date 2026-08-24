import { invoke } from "@tauri-apps/api/core";
import type {
  ColumnMapping,
  CsvPreview,
  ImportDecision,
  ImportResult,
  PendingImport,
} from "./types/import";

export async function parseCsvPreview(content: string): Promise<CsvPreview> {
  try {
    return await invoke<CsvPreview>("parse_csv_preview", { content });
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}

export async function previewCsvImport(
  content: string,
  mapping: ColumnMapping,
  accountId: string,
): Promise<PendingImport> {
  try {
    return await invoke<PendingImport>("preview_csv_import", {
      content,
      mapping,
      accountId,
    });
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}

export async function previewOfxImport(
  content: string,
  accountId: string,
): Promise<PendingImport> {
  try {
    return await invoke<PendingImport>("preview_ofx_import", {
      content,
      accountId,
    });
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}

export async function importCsv(
  content: string,
  mapping: ColumnMapping,
  accountId: string,
  filename: string,
  decisions?: ImportDecision[],
): Promise<ImportResult> {
  try {
    return await invoke<ImportResult>("import_csv", {
      content,
      mapping,
      accountId,
      filename,
      decisions: decisions ?? null,
    });
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}

export async function importOfx(
  content: string,
  accountId: string,
  filename: string,
  decisions?: ImportDecision[],
): Promise<ImportResult> {
  try {
    return await invoke<ImportResult>("import_ofx", {
      content,
      accountId,
      filename,
      decisions: decisions ?? null,
    });
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}
