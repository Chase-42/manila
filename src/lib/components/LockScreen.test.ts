import { describe, expect, it } from "vitest";
import { validatePassword } from "../lockscreen";

describe("validatePassword", () => {
  it("returns error when password is empty", () => {
    expect(validatePassword("", "", false)).toBe("Password is required.");
    expect(validatePassword("", "", true)).toBe("Password is required.");
  });

  it("returns null when password is non-empty and not initializing", () => {
    expect(validatePassword("secret", "", false)).toBeNull();
  });

  it("returns error when initializing with mismatched confirm", () => {
    expect(validatePassword("secret", "different", true)).toBe("Passwords do not match.");
  });

  it("returns null when initializing with matching passwords", () => {
    expect(validatePassword("secret", "secret", true)).toBeNull();
  });
});
