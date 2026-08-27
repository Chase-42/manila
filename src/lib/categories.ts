import { invoke } from "@tauri-apps/api/core";
import type { CategoryRow } from "./generated/CategoryRow";

export type { CategoryRow };

export async function listCategories(): Promise<CategoryRow[]> {
  return invoke("list_categories");
}

export async function createCategory(
  name: string,
  kind: "flow" | "sinking",
): Promise<string> {
  return invoke("create_category", { name, kind });
}

export async function updateCategory(
  id: string,
  name: string,
): Promise<void> {
  return invoke("update_category", { id, name });
}

export async function upsertSplit(
  transactionId: string,
  targetType: string,
  targetId: string,
): Promise<void> {
  return invoke("upsert_split", { transactionId, targetType, targetId });
}
