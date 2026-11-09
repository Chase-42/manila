export const ONBOARDING_PENDING_KEY = "manila.onboarding_pending";

export function markOnboardingPending(): void {
  localStorage.setItem(ONBOARDING_PENDING_KEY, "1");
}

export function isOnboardingPending(): boolean {
  return localStorage.getItem(ONBOARDING_PENDING_KEY) !== null;
}

export function clearOnboardingPending(): void {
  localStorage.removeItem(ONBOARDING_PENDING_KEY);
}
