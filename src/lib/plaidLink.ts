interface PlaidLinkError {
  error_code: string;
  error_message: string;
}

interface PlaidInstitution {
  institution_id: string;
  name: string;
}

interface PlaidAccount {
  account_id: string;
}

export interface PlaidSuccessMetadata {
  institution: PlaidInstitution;
  accounts: PlaidAccount[];
}

interface PlaidHandler {
  open(): void;
  exit(options?: { force?: boolean }): void;
  destroy(): void;
}

interface PlaidLinkOptions {
  token: string;
  receivedRedirectUri?: string;
  onSuccess: (publicToken: string, metadata: PlaidSuccessMetadata) => void;
  onExit: (err: PlaidLinkError | null, metadata: unknown) => void;
}

declare global {
  interface Window {
    Plaid?: { create(options: PlaidLinkOptions): PlaidHandler };
  }
}

const PLAID_SCRIPT_URL =
  "https://cdn.plaid.com/link/v2/stable/link-initialize.js";

export function loadPlaidScript(): Promise<void> {
  return new Promise((resolve, reject) => {
    if (window.Plaid) {
      resolve();
      return;
    }
    if (document.querySelector(`script[src="${PLAID_SCRIPT_URL}"]`)) {
      if (window.Plaid) {
        resolve();
        return;
      }
      const existing = document.querySelector(
        `script[src="${PLAID_SCRIPT_URL}"]`,
      ) as HTMLScriptElement;
      existing.addEventListener("load", () => resolve());
      existing.addEventListener("error", () =>
        reject(new Error("Failed to load Plaid Link script")),
      );
      return;
    }
    const script = document.createElement("script");
    script.src = PLAID_SCRIPT_URL;
    script.onload = () => resolve();
    script.onerror = () => reject(new Error("Failed to load Plaid Link script"));
    document.head.appendChild(script);
  });
}

export interface ConnectionParams {
  plaid_item_id: string;
  access_token: string;
  institution_name: string;
  institution_id: string;
  account_ids: string;
}

export function extractConnectionParams(
  accessToken: string,
  itemId: string,
  metadata: PlaidSuccessMetadata,
): ConnectionParams {
  return {
    plaid_item_id: itemId,
    access_token: accessToken,
    institution_name: metadata.institution.name,
    institution_id: metadata.institution.institution_id,
    account_ids: JSON.stringify(metadata.accounts.map((a) => a.account_id)),
  };
}
