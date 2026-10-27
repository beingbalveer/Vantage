// Small custom fuzzy scorer (subsequence match with word-start bonuses)

export interface FuzzyMatchResult {
  score: number;
  indices: number[];
}

export function fuzzyScore(pattern: string, text: string): FuzzyMatchResult | null {
  const pLen = pattern.length;
  const tLen = text.length;

  if (pLen === 0) return { score: 1, indices: [] };
  if (pLen > tLen) return null;

  const lowerPattern = pattern.toLowerCase();
  const lowerText = text.toLowerCase();

  const indices: number[] = [];
  let score = 0;
  let pIdx = 0;
  let lastMatchIdx = -1;

  for (let tIdx = 0; tIdx < tLen; tIdx++) {
    if (lowerText[tIdx] === lowerPattern[pIdx]) {
      indices.push(tIdx);

      // Base match points
      score += 10;

      // Bonus for consecutive matches
      if (lastMatchIdx === tIdx - 1) {
        score += 15;
      }

      // Bonus for match at word start (after space, slash, dash, underscore, or camelCase)
      if (
        tIdx === 0 ||
        /[\s/\\_\-.]/.test(text[tIdx - 1]) ||
        (text[tIdx] >= "A" && text[tIdx] <= "Z" && text[tIdx - 1] >= "a" && text[tIdx - 1] <= "z")
      ) {
        score += 25;
      }

      lastMatchIdx = tIdx;
      pIdx++;

      if (pIdx === pLen) {
        // Matched full pattern
        // Penalize overall length distance slightly
        score -= (tLen - pLen);
        return { score, indices };
      }
    }
  }

  return null;
}

/**
 * Rank file paths for the explorer's quick filter. Empty queries return the
 * original list; otherwise only fuzzy matches survive, best score first.
 */
export function rankPaths(query: string, paths: string[]): string[] {
  const trimmed = query.trim();
  if (!trimmed) return [...paths];
  return paths
    .map((path) => ({ path, match: fuzzyScore(trimmed, path) }))
    .filter(
      (entry): entry is { path: string; match: FuzzyMatchResult } => entry.match !== null
    )
    .sort((a, b) => b.match.score - a.match.score || a.path.localeCompare(b.path))
    .map((entry) => entry.path);
}
