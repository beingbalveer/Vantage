import { writable } from "svelte/store";
import type { NavigationTarget } from "../../platform/types";

export const navigationTarget = writable<NavigationTarget | null>(null);

export function navigateTo(target: NavigationTarget) {
  navigationTarget.set({ ...target });
}

export function clearNavigation() {
  navigationTarget.set(null);
}
