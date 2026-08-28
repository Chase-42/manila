import { invoke } from "@tauri-apps/api/core";
import type { BudgetMonthView } from "./generated/BudgetMonthView";

export type { BudgetMonthView };
export type { BudgetCategoryRow } from "./generated/BudgetCategoryRow";
export type { BudgetGroupView } from "./generated/BudgetGroupView";
export type { IncomeCategoryRow } from "./generated/IncomeCategoryRow";
export type { ReallocationEntry } from "./generated/ReallocationEntry";

export async function getBudgetMonth(month: string): Promise<BudgetMonthView> {
  return invoke("get_budget_month", { month });
}

export async function setAllocation(
  categoryId: string,
  month: string,
  newAmountCents: number,
): Promise<void> {
  return invoke("set_allocation", {
    categoryId,
    month,
    newAmountCents,
  });
}

export async function closeMonth(month: string): Promise<void> {
  return invoke("close_month", { month });
}

export async function reallocate(
  fromCategoryId: string,
  toCategoryId: string,
  month: string,
  amountCents: number,
): Promise<void> {
  return invoke("reallocate", {
    fromCategoryId,
    toCategoryId,
    month,
    amountCents,
  });
}

export function parseCentsFromString(val: string): number {
  const n = parseFloat(val);
  return isNaN(n) ? 0 : Math.round(n * 100);
}

export function validateReallocation(
  fromId: string,
  toId: string,
  amountStr: string,
): string | null {
  if (parseCentsFromString(amountStr) <= 0) return "Amount must be greater than zero.";
  if (!fromId) return "Select a source category.";
  if (!toId) return "Select a destination category.";
  if (fromId === toId) return "Source and destination must be different.";
  return null;
}

