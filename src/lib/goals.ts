import { invoke } from "@tauri-apps/api/core";
export type { Goal } from "./generated/Goal";
export type { GoalWithProgress } from "./generated/GoalWithProgress";
import type { GoalWithProgress } from "./generated/GoalWithProgress";
import type { Goal } from "./generated/Goal";

export async function listGoalsWithProgress(): Promise<GoalWithProgress[]> {
  return invoke("list_goals_with_progress");
}

export async function createGoal(
  name: string,
  targetAmountCents: number,
  categoryId: string | null,
  targetDate: string | null,
): Promise<Goal> {
  return invoke("create_goal", { name, targetAmountCents, categoryId, targetDate });
}

export async function updateGoal(
  id: string,
  name: string,
  targetAmountCents: number,
  categoryId: string | null,
  targetDate: string | null,
): Promise<Goal> {
  return invoke("update_goal", { id, name, targetAmountCents, categoryId, targetDate });
}

export async function deleteGoal(id: string): Promise<void> {
  return invoke("delete_goal", { id });
}

export function formatDaysUntilTarget(targetDateStr: string): string {
  const target = new Date(targetDateStr);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const diff = Math.ceil((target.getTime() - today.getTime()) / (1000 * 60 * 60 * 24));
  if (diff < 0) return "Past target date";
  if (diff === 0) return "Due today";
  if (diff === 1) return "1 day left";
  return `${diff} days left`;
}
