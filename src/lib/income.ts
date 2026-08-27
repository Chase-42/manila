import { invoke } from "@tauri-apps/api/core";
import type { IncomeCategoryItem } from "./generated/IncomeCategoryItem";

export type { IncomeCategoryItem };

export async function listIncomeCategories(): Promise<IncomeCategoryItem[]> {
  return invoke("list_income_categories");
}

export async function createIncomeCategory(name: string): Promise<string> {
  return invoke("create_income_category", { name });
}

export async function setIncomeCategoryHidden(
  id: string,
  hidden: boolean,
): Promise<void> {
  return invoke("set_income_category_hidden", { id, hidden });
}
