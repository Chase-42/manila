import { invoke } from "@tauri-apps/api/core";
import type {
  ColumnMapping,
  CsvPreview,
  ImportDecision,
  ImportResult,
  PendingImport,
} from "./types/import";

export type ColRole = "ignore" | "date" | "description" | "amount" | "debit" | "credit";
export type Mode = "single" | "split";

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

export function detectRole(lower: string, taken: Set<ColRole>): ColRole {
  if (!taken.has("date") && DATE_HEADERS.has(lower)) return "date";
  if (!taken.has("description") && DESC_HEADERS.has(lower)) return "description";
  if (!taken.has("amount") && AMOUNT_HEADERS.has(lower)) return "amount";
  if (!taken.has("debit") && DEBIT_HEADERS.has(lower)) return "debit";
  if (!taken.has("credit") && CREDIT_HEADERS.has(lower)) return "credit";
  return "ignore";
}

export function autoDetect(
  headers: string[],
): { assignments: Record<string, ColRole>; detectedMode: Mode } {
  const result: Record<string, ColRole> = Object.fromEntries(
    headers.map((h) => [h, "ignore" as ColRole]),
  );
  const taken = new Set<ColRole>();

  for (const h of headers) {
    const role = detectRole(h.toLowerCase().trim(), taken);
    result[h] = role;
    if (role !== "ignore") taken.add(role);
  }

  const detectedMode: Mode =
    taken.has("debit") && taken.has("credit") && !taken.has("amount") ? "split" : "single";

  return { assignments: result, detectedMode };
}

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
