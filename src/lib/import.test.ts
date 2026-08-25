import { describe, it, expect } from "vitest";
import { detectRole, autoDetect } from "./import";
import type { ColRole } from "./import";

describe("detectRole", () => {
  it("detects date role", () => {
    const taken = new Set<ColRole>();
    expect(detectRole("date", taken)).toBe("date");
    expect(detectRole("transaction date", taken)).toBe("date");
    expect(detectRole("posted date", taken)).toBe("date");
  });

  it("detects description role", () => {
    const taken = new Set<ColRole>();
    expect(detectRole("description", taken)).toBe("description");
    expect(detectRole("memo", taken)).toBe("description");
    expect(detectRole("payee", taken)).toBe("description");
  });

  it("detects amount role", () => {
    const taken = new Set<ColRole>();
    expect(detectRole("amount", taken)).toBe("amount");
    expect(detectRole("transaction amount", taken)).toBe("amount");
  });

  it("detects debit role", () => {
    const taken = new Set<ColRole>();
    expect(detectRole("debit", taken)).toBe("debit");
    expect(detectRole("withdrawal", taken)).toBe("debit");
  });

  it("detects credit role", () => {
    const taken = new Set<ColRole>();
    expect(detectRole("credit", taken)).toBe("credit");
    expect(detectRole("deposit", taken)).toBe("credit");
  });

  it("falls back to ignore for unknown headers", () => {
    const taken = new Set<ColRole>();
    expect(detectRole("foobar", taken)).toBe("ignore");
    expect(detectRole("", taken)).toBe("ignore");
  });

  it("returns ignore when role is already taken", () => {
    const taken = new Set<ColRole>(["date"]);
    expect(detectRole("date", taken)).toBe("ignore");
    expect(detectRole("transaction date", taken)).toBe("ignore");
  });

  it("does not double-assign taken roles", () => {
    const taken = new Set<ColRole>(["description", "amount"]);
    expect(detectRole("memo", taken)).toBe("ignore");
    expect(detectRole("transaction amount", taken)).toBe("ignore");
  });
});

describe("autoDetect", () => {
  it("assigns all five roles from canonical headers", () => {
    const headers = ["Date", "Description", "Amount", "Debit", "Credit"];
    const { assignments } = autoDetect(headers);
    expect(assignments["Date"]).toBe("date");
    expect(assignments["Description"]).toBe("description");
    expect(assignments["Amount"]).toBe("amount");
    expect(assignments["Debit"]).toBe("debit");
    expect(assignments["Credit"]).toBe("credit");
  });

  it("ignores unrecognised headers", () => {
    const { assignments } = autoDetect(["Date", "Notes", "Amount"]);
    expect(assignments["Notes"]).toBe("ignore");
  });

  it("detects single mode when amount column is present", () => {
    const { detectedMode } = autoDetect(["Date", "Description", "Amount"]);
    expect(detectedMode).toBe("single");
  });

  it("detects split mode when debit and credit are present but amount is not", () => {
    const { detectedMode } = autoDetect(["Date", "Description", "Debit", "Credit"]);
    expect(detectedMode).toBe("split");
  });

  it("stays single when amount is present alongside debit/credit", () => {
    const { detectedMode } = autoDetect(["Date", "Description", "Amount", "Debit", "Credit"]);
    expect(detectedMode).toBe("single");
  });

  it("does not double-assign the same role when duplicate header names appear", () => {
    const headers = ["Date", "Date2", "Description", "Amount"];
    const { assignments } = autoDetect(headers);
    expect(assignments["Date"]).toBe("date");
    expect(assignments["Date2"]).toBe("ignore");
  });
});
