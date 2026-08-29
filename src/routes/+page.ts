import { getHomeView } from '$lib/budget';

export const load = async () => {
  const today = new Date();
  const todayStr = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`;
  try {
    return { home: await getHomeView(todayStr) };
  } catch {
    return { home: null };
  }
};
