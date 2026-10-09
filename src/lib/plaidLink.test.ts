import { describe, it, expect } from "vitest";
import { extractConnectionParams } from "./plaidLink";
import type { PlaidSuccessMetadata } from "./plaidLink";

describe("extractConnectionParams", () => {
  it("maps all fields from metadata and exchange response", () => {
    const metadata: PlaidSuccessMetadata = {
      institution: { institution_id: "ins_123", name: "First National Bank" },
      accounts: [
        { account_id: "acc_a" },
        { account_id: "acc_b" },
      ],
    };

    const result = extractConnectionParams(
      "public-token-abc",
      "item-id-xyz",
      metadata,
    );

    expect(result.plaid_item_id).toBe("item-id-xyz");
    expect(result.access_token).toBe("public-token-abc");
    expect(result.institution_name).toBe("First National Bank");
    expect(result.institution_id).toBe("ins_123");
    expect(result.account_ids).toBe('["acc_a","acc_b"]');
  });

  it("encodes an empty accounts array as an empty JSON array string", () => {
    const metadata: PlaidSuccessMetadata = {
      institution: { institution_id: "ins_456", name: "Savings Bank" },
      accounts: [],
    };

    const result = extractConnectionParams("tok", "item-456", metadata);

    expect(result.account_ids).toBe("[]");
  });
});
