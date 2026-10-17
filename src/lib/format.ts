// Formatting helpers for dates, numbers, and avatar colors

export function formatRelativeTime(dateStr: string): string {
  try {
    const date = new Date(dateStr);
    const now = new Date();
    const diffSec = Math.floor((now.getTime() - date.getTime()) / 1000);

    if (diffSec < 60) {
      return "just now";
    }
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) {
      return `${diffMin}m ago`;
    }
    const diffHours = Math.floor(diffMin / 60);
    if (diffHours < 24) {
      return `${diffHours}h ago`;
    }
    const diffDays = Math.floor(diffHours / 24);
    if (diffDays === 1) {
      return "yesterday";
    }
    if (diffDays < 7) {
      return `${diffDays}d ago`;
    }

    return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  } catch {
    return dateStr;
  }
}

export function formatFullTimestamp(dateStr: string): string {
  try {
    const date = new Date(dateStr);
    return date.toLocaleString();
  } catch {
    return dateStr;
  }
}

// 8 stable desaturated hues for repo/author avatars (AA contrast in both light/dark)
const AVATAR_PALETTE = [
  "#2f81f7", // blue
  "#238636", // green
  "#8957e5", // purple
  "#d29922", // amber/orange
  "#da3633", // red
  "#1f6feb", // cyan/blue
  "#b08800", // yellow
  "#bf4b8a", // magenta
];

export function hashStringToColor(str: string): string {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = (hash << 5) - hash + str.charCodeAt(i);
    hash |= 0;
  }
  const index = Math.abs(hash) % AVATAR_PALETTE.length;
  return AVATAR_PALETTE[index];
}

export function formatStats(additions: number, deletions: number): string {
  return `+${additions} −${deletions}`;
}

export interface VirtualWindow {
  startIndex: number;
  endIndex: number;
  offsetTop: number;
  totalHeight: number;
  visibleCount: number;
}

export function computeVirtualWindow(
  totalCount: number,
  scrollTop: number,
  viewportHeight: number,
  itemHeight: number,
  overscan = 20
): VirtualWindow {
  if (totalCount <= 0 || itemHeight <= 0) {
    return { startIndex: 0, endIndex: 0, offsetTop: 0, totalHeight: 0, visibleCount: 0 };
  }
  const totalHeight = totalCount * itemHeight;
  const rawStart = Math.floor(Math.max(0, scrollTop) / itemHeight);
  const rawEnd = Math.ceil(Math.max(0, scrollTop + viewportHeight) / itemHeight);

  const startIndex = Math.max(0, rawStart - overscan);
  const endIndex = Math.min(totalCount, rawEnd + overscan);
  const offsetTop = startIndex * itemHeight;
  const visibleCount = Math.max(0, endIndex - startIndex);

  return {
    startIndex,
    endIndex,
    offsetTop,
    totalHeight,
    visibleCount,
  };
}

export function splitPath(fullPath: string): { dir: string; name: string } {
  const normalized = fullPath.replace(/\\/g, "/");
  const idx = normalized.lastIndexOf("/");
  if (idx === -1) {
    return { dir: "", name: normalized };
  }
  return {
    dir: normalized.slice(0, idx),
    name: normalized.slice(idx + 1),
  };
}
