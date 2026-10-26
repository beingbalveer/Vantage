/**
 * Motion helpers shared by CSS-driven and JS-driven animation.
 *
 * CSS transitions/animations are already neutralised by the
 * `prefers-reduced-motion` media query in `styles/base.css`; this keeps
 * imperative scrolling in CodeViewer consistent with that preference.
 */

import { isFeatureEnabled } from "./featureFlags";

export type MotionMatcher = (query: string) => { matches: boolean };

export function prefersReducedMotion(
  matchMediaFn: MotionMatcher | undefined = typeof window !== "undefined" &&
    typeof window.matchMedia === "function"
    ? window.matchMedia.bind(window)
    : undefined
): boolean {
  if (!isFeatureEnabled("ALPH_164_ACCESSIBILITY")) return false;
  if (!matchMediaFn) return false;
  try {
    return matchMediaFn("(prefers-reduced-motion: reduce)").matches;
  } catch {
    return false;
  }
}

/** Scroll behavior for `scrollIntoView`, honouring reduced motion. */
export function scrollBehavior(
  reduceMotion: boolean = prefersReducedMotion()
): ScrollBehavior {
  return reduceMotion ? "auto" : "smooth";
}
