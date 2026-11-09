import { describe, it, expect, beforeEach } from "vitest";
import {
  ONBOARDING_PENDING_KEY,
  markOnboardingPending,
  isOnboardingPending,
  clearOnboardingPending,
} from "./onboarding";

describe("onboarding helpers", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("markOnboardingPending sets the key", () => {
    markOnboardingPending();
    expect(localStorage.getItem(ONBOARDING_PENDING_KEY)).toBe("1");
  });

  it("isOnboardingPending returns true when key is set", () => {
    localStorage.setItem(ONBOARDING_PENDING_KEY, "1");
    expect(isOnboardingPending()).toBe(true);
  });

  it("isOnboardingPending returns false when key is absent", () => {
    expect(isOnboardingPending()).toBe(false);
  });

  it("clearOnboardingPending removes the key", () => {
    localStorage.setItem(ONBOARDING_PENDING_KEY, "1");
    clearOnboardingPending();
    expect(localStorage.getItem(ONBOARDING_PENDING_KEY)).toBeNull();
  });
});
