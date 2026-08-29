import { getHomeView } from '$lib/budget';
import { listGoalsWithProgress } from '$lib/goals';

export const load = async () => {
  const today = new Date();
  const todayStr = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`;
  const [homeResult, goalsResult] = await Promise.allSettled([
    getHomeView(todayStr),
    listGoalsWithProgress(),
  ]);
  return {
    home: homeResult.status === 'fulfilled' ? homeResult.value : null,
    goals: goalsResult.status === 'fulfilled' ? goalsResult.value : [],
  };
};
