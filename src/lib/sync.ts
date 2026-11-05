import { invoke } from "@tauri-apps/api/core";
import type { SyncConfig } from "./generated/SyncConfig";
import type { SyncStatus } from "./generated/SyncStatus";
import type { SyncLastRun } from "./generated/SyncLastRun";

export async function configureSync(serverUrl: string): Promise<void> {
  try {
    await invoke("configure_sync", { serverUrl });
  } catch (e) {
    throw e instanceof Error ? e : new Error(String(e));
  }
}

export async function getSyncConfig(): Promise<SyncConfig> {
  try {
    return await invoke<SyncConfig>("get_sync_config");
  } catch (e) {
    throw e instanceof Error ? e : new Error(String(e));
  }
}

export async function enrollSync(serverUrl: string): Promise<string> {
  try {
    return await invoke<string>("enroll_sync", { serverUrl });
  } catch (e) {
    throw e instanceof Error ? e : new Error(String(e));
  }
}

export async function runSync(): Promise<SyncStatus> {
  try {
    return await invoke<SyncStatus>("run_sync");
  } catch (e) {
    throw e instanceof Error ? e : new Error(String(e));
  }
}

export async function getSyncStatus(): Promise<SyncLastRun | null> {
  try {
    return await invoke<SyncLastRun | null>("get_sync_status");
  } catch (e) {
    throw e instanceof Error ? e : new Error(String(e));
  }
}

export async function revokeSync(): Promise<void> {
  try {
    await invoke("revoke_sync");
  } catch (e) {
    throw e instanceof Error ? e : new Error(String(e));
  }
}
