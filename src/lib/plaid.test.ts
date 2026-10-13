import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { syncPlaidConnection } from "./plaid";

const mockInvoke = vi.mocked(invoke);

describe("syncPlaidConnection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("invokes sync_plaid_connection with camelCase keys", async () => {
    mockInvoke.mockResolvedValue({
      imported_count: 3,
      accounts_created: 1,
      skipped_count: 0,
      errors: [],
    });

    const result = await syncPlaidConnection("conn-id-123", "https://proxy.example.com");

    expect(mockInvoke).toHaveBeenCalledWith("sync_plaid_connection", {
      connectionId: "conn-id-123",
      proxyUrl: "https://proxy.example.com",
    });
    expect(result.imported_count).toBe(3);
    expect(result.accounts_created).toBe(1);
  });

  it("re-throws invoke errors as Error objects", async () => {
    mockInvoke.mockRejectedValue("locked");

    await expect(
      syncPlaidConnection("conn-id-123", "https://proxy.example.com"),
    ).rejects.toThrow("locked");
  });
});
