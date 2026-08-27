import { invoke } from "@tauri-apps/api/core";
import type { CategoryGroupRow } from "./generated/CategoryGroupRow";

export type { CategoryGroupRow };

export async function listCategoryGroups(): Promise<CategoryGroupRow[]> {
  return invoke("list_category_groups");
}

export async function createCategoryGroup(name: string): Promise<string> {
  return invoke("create_category_group", { name });
}

export async function updateCategoryGroup(id: string, name: string): Promise<void> {
  return invoke("update_category_group", { id, name });
}

export async function deleteCategoryGroup(id: string): Promise<void> {
  return invoke("delete_category_group", { id });
}

export async function assignCategoryToGroup(
  categoryId: string,
  groupId: string | null,
): Promise<void> {
  return invoke("assign_category_to_group", {
    category_id: categoryId,
    group_id: groupId,
  });
}
