import { describe, it, expect, vi, afterEach } from "vitest";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { reallocate, parseCentsFromString, validateReallocation } from "./budget";

afterEach(() => clearMocks());

describe("parseCentsFromString", () => {
  it("converts a dollar string to cents", () => {
    expect(parseCentsFromString("10.00")).toBe(1000);
    expect(parseCentsFromString("0.01")).toBe(1);
    expect(parseCentsFromString("100")).toBe(10000);
  });

  it("returns 0 for empty or non-numeric input", () => {
    expect(parseCentsFromString("")).toBe(0);
    expect(parseCentsFromString("abc")).toBe(0);
  });
});

describe("validateReallocation", () => {
  it("returns null when all inputs are valid", () => {
    expect(validateReallocation("cat-a", "cat-b", "10.00")).toBeNull();
  });

  it("rejects zero amount", () => {
    expect(validateReallocation("cat-a", "cat-b", "0")).not.toBeNull();
    expect(validateReallocation("cat-a", "cat-b", "0.00")).not.toBeNull();
  });

  it("rejects empty amount", () => {
    expect(validateReallocation("cat-a", "cat-b", "")).not.toBeNull();
  });

  it("rejects missing source", () => {
    expect(validateReallocation("", "cat-b", "10.00")).not.toBeNull();
  });

  it("rejects missing destination", () => {
    expect(validateReallocation("cat-a", "", "10.00")).not.toBeNull();
  });

  it("rejects same source and destination", () => {
    expect(validateReallocation("cat-a", "cat-a", "10.00")).not.toBeNull();
  });
});

describe("reallocate", () => {
  it("invokes reallocate with correct argument shape", async () => {
    const spy = vi.fn().mockResolvedValue(undefined);
    mockIPC((cmd, args) => {
      if (cmd === "reallocate") return spy(cmd, args);
      return undefined;
    });

    await reallocate("cat-a", "cat-b", "2026-08", 5000);

    expect(spy).toHaveBeenCalledOnce();
    const [, args] = spy.mock.calls[0] as [string, Record<string, unknown>];
    expect(args).toEqual({
      fromCategoryId: "cat-a",
      toCategoryId: "cat-b",
      month: "2026-08",
      amountCents: 5000,
    });
  });

  it("propagates errors from the backend", async () => {
    mockIPC(() => { throw new Error("source and destination must differ"); });
    await expect(reallocate("x", "x", "2026-08", 1000)).rejects.toThrow();
  });
});
