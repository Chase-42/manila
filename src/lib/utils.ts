import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import type { ClassValue } from "clsx";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// Re-exported for shadcn-svelte components that import from $lib/utils
export type {
  WithElementRef,
  WithoutChild,
  WithoutChildren,
  WithoutChildrenOrChild,
} from "bits-ui";
