import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { createGoal, deleteGoal, formatDaysUntilTarget } from "./goals";

afterEach(() => clearMocks());

describe("formatDaysUntilTarget", () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it("returns 'Due today' when target is today", () => {
    vi.setSystemTime(new Date("2026-08-29T12:00:00"));
    expect(formatDaysUntilTarget("2026-08-29")).toBe("Due today");
  });

  it("returns '1 day left' for tomorrow", () => {
    vi.setSystemTime(new Date("2026-08-29T12:00:00"));
    expect(formatDaysUntilTarget("2026-08-30")).toBe("1 day left");
  });

  it("returns 'X days left' for multiple days", () => {
    vi.setSystemTime(new Date("2026-08-29T12:00:00"));
    expect(formatDaysUntilTarget("2026-09-05")).toBe("7 days left");
  });

  it("returns 'Past target date' when overdue", () => {
    vi.setSystemTime(new Date("2026-08-29T12:00:00"));
    expect(formatDaysUntilTarget("2026-08-01")).toBe("Past target date");
  });
});

describe("createGoal", () => {
  it("invokes create_goal with correct argument shape", async () => {
    const spy = vi.fn().mockResolvedValue({ id: "g1", name: "Vacation", target_amount_cents: 500000, category_id: null, target_date: null, achieved_at: null, created_at: "2026-01-01T00:00:00Z" });
    mockIPC((cmd, args) => {
      if (cmd === "create_goal") return spy(cmd, args);
      return undefined;
    });

    await createGoal("Vacation", 500000, null, null);

    expect(spy).toHaveBeenCalledOnce();
    const [, args] = spy.mock.calls[0] as [string, Record<string, unknown>];
    expect(args).toEqual({
      name: "Vacation",
      targetAmountCents: 500000,
      categoryId: null,
      targetDate: null,
    });
  });
});

describe("deleteGoal", () => {
  it("invokes delete_goal with correct argument shape", async () => {
    const spy = vi.fn().mockResolvedValue(undefined);
    mockIPC((cmd, args) => {
      if (cmd === "delete_goal") return spy(cmd, args);
      return undefined;
    });

    await deleteGoal("goal-id-1");

    expect(spy).toHaveBeenCalledOnce();
    const [, args] = spy.mock.calls[0] as [string, Record<string, unknown>];
    expect(args).toEqual({ id: "goal-id-1" });
  });
});
