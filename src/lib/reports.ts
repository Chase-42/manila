import { invoke } from '@tauri-apps/api/core';
import type { CategorySpendReport } from '$lib/generated/CategorySpendReport';
import type { MonthlySpendTrend } from '$lib/generated/MonthlySpendTrend';

export async function getSpendingByCategory(month: string): Promise<CategorySpendReport[]> {
  return invoke<CategorySpendReport[]>('get_spending_by_category', { month });
}

export async function getMonthlySpendTrend(months: number): Promise<MonthlySpendTrend[]> {
  return invoke<MonthlySpendTrend[]>('get_monthly_spend_trend', { months });
}

export function lastNMonths(n: number): string[] {
  const months: string[] = [];
  const now = new Date();
  let year = now.getFullYear();
  let month = now.getMonth() + 1;
  for (let i = 0; i < n; i++) {
    months.push(`${String(year).padStart(4, '0')}-${String(month).padStart(2, '0')}`);
    month--;
    if (month === 0) {
      month = 12;
      year--;
    }
  }
  return months;
}

export function formatMonthLabel(yyyyMm: string): string {
  const [year, month] = yyyyMm.split('-').map(Number);
  return new Date(year, month - 1, 1).toLocaleDateString('en-US', { month: 'short', year: 'numeric' });
}
