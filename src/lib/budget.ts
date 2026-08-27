import { invoke } from "@tauri-apps/api/core";
import type { BudgetMonthView } from "./generated/BudgetMonthView";

export type { BudgetMonthView };
export type { BudgetCategoryRow } from "./generated/BudgetCategoryRow";
export type { BudgetGroupView } from "./generated/BudgetGroupView";
export type { IncomeCategoryRow } from "./generated/IncomeCategoryRow";

export async function getBudgetMonth(month: string): Promise<BudgetMonthView> {
  return invoke("get_budget_month", { month });
}

export async function setAllocation(
  categoryId: string,
  month: string,
  newAmountCents: number,
): Promise<void> {
  return invoke("set_allocation", {
    category_id: categoryId,
    month,
    new_amount_cents: newAmountCents,
  });
}

