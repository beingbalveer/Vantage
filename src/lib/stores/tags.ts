import { writable } from "svelte/store";
import type { TagInfo } from "../types";
import * as api from "../api";

/** Local tags per repo id, refreshed on demand (history load, tag create/push). */
export const tagsByRepo = writable<Record<string, TagInfo[]>>({});

export async function loadTags(repoId: string): Promise<TagInfo[]> {
  if (!repoId) return [];
  try {
    const tags = await api.tagList(repoId);
    tagsByRepo.update((map) => ({ ...map, [repoId]: tags }));
    return tags;
  } catch (e) {
    console.error(`Failed to load tags for ${repoId}:`, e);
    return [];
  }
}

/** Index tags by the commit SHA they point at, for chip rendering per row. */
export function groupTagsByTarget(tags: TagInfo[]): Map<string, TagInfo[]> {
  const map = new Map<string, TagInfo[]>();
  for (const tag of tags) {
    const list = map.get(tag.target);
    if (list) list.push(tag);
    else map.set(tag.target, [tag]);
  }
  return map;
}
