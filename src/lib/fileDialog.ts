import { open } from "@tauri-apps/plugin-dialog";
import { readTextFile } from "@tauri-apps/plugin-fs";

export type PickedFile = { name: string; content: string };

export async function pickTextFile(filters: { name: string; extensions: string[] }[]): Promise<PickedFile | null> {
  const path = await open({ multiple: false, filters });
  if (!path) return null;
  const name = typeof path === "string" ? path.split("/").pop() ?? path : "";
  const content = await readTextFile(path as string);
  return { name, content };
}
