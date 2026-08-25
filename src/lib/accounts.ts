import { invoke } from "@tauri-apps/api/core";
import type { Account } from "./types/account";

export async function listAccounts(): Promise<Account[]> {
  try {
    return await invoke<Account[]>("list_accounts");
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}

export async function createAccount(params: {
  name: string;
  account_type: string;
  subtype: string;
  institution: string;
  currency: string;
}): Promise<string> {
  try {
    // Tauri v2 converts camelCase keys to snake_case for Rust
    return await invoke<string>("create_account", {
      name: params.name,
      accountType: params.account_type,
      subtype: params.subtype,
      institution: params.institution,
      currency: params.currency,
    });
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}

export async function updateAccount(params: {
  id: string;
  name: string;
  account_type: string;
  subtype: string;
  institution: string;
}): Promise<void> {
  try {
    // Tauri v2 converts camelCase keys to snake_case for Rust
    await invoke<void>("update_account", {
      id: params.id,
      name: params.name,
      accountType: params.account_type,
      subtype: params.subtype,
      institution: params.institution,
    });
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}
