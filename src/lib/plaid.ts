import { invoke } from "@tauri-apps/api/core";
import type { PlaidConnection } from "./types/plaid";

export async function listPlaidConnections(): Promise<PlaidConnection[]> {
  try {
    return await invoke<PlaidConnection[]>("list_plaid_connections");
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}

export async function upsertPlaidConnection(params: {
  plaid_item_id: string;
  access_token: string;
  institution_name: string;
  institution_id: string;
  account_ids: string;
}): Promise<string> {
  try {
    // Tauri v2 converts camelCase keys to snake_case for Rust
    return await invoke<string>("upsert_plaid_connection", {
      plaidItemId: params.plaid_item_id,
      accessToken: params.access_token,
      institutionName: params.institution_name,
      institutionId: params.institution_id,
      accountIds: params.account_ids,
    });
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}

export async function deletePlaidConnection(id: string): Promise<void> {
  try {
    await invoke<void>("delete_plaid_connection", { id });
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
}
