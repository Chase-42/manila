import { listGoalsWithProgress } from "$lib/goals";
import { listCategories } from "$lib/categories";

export const load = async () => {
  try {
    const [goals, categories] = await Promise.all([
      listGoalsWithProgress(),
      listCategories(),
    ]);
    return { goals, categories };
  } catch {
    return { goals: [], categories: [] };
  }
};
