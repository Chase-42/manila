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
    return await invoke<string>("create_account", params);
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
    await invoke<void>("update_account", params);
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}
