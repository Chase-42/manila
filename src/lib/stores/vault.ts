import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type { VaultStatus } from "$lib/generated/VaultStatus";

export const vaultStatus = writable<VaultStatus | null>(null);

export async function refreshVaultStatus(): Promise<void> {
  const status = await invoke<VaultStatus>("vault_status");
  vaultStatus.set(status);
}

export async function lockVault(): Promise<void> {
  await invoke("lock_vault");
  await refreshVaultStatus();
}
