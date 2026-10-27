// Pure helpers for the commit co-author picker (ALPH-155).
//
// The Rust backend re-validates every entry before it reaches git; these
// helpers give the UI immediate, clear feedback and keep the two rule sets in
// step. Nothing here may ever produce a string containing a newline or a
// leading `-`, so user text can never be interpreted as a git option/header.

export interface CoAuthor {
  name: string;
  email: string;
}

export type CoAuthorParseResult =
  | { ok: true; coAuthor: CoAuthor; formatted: string }
  | { ok: false; error: string };

const TRAILER_KEY = "Co-authored-by:";

function isPlausibleEmail(email: string): boolean {
  if (!email) return false;
  if (/[\s<>]/.test(email)) return false;
  for (const ch of email) {
    const code = ch.codePointAt(0)!;
    if (code < 0x20 || code === 0x7f) return false;
  }
  const at = email.lastIndexOf("@");
  if (at <= 0 || at === email.length - 1) return false;
  const local = email.slice(0, at);
  const domain = email.slice(at + 1);
  return !local.includes("@") && !domain.includes("@");
}

/**
 * Validate and normalize a single `Name <email>` co-author entry.
 * Returns the canonical value or a human-readable error.
 */
export function parseCoAuthor(input: string): CoAuthorParseResult {
  const trimmed = input.trim();
  if (!trimmed) return { ok: false, error: "Co-author cannot be empty" };

  for (const ch of trimmed) {
    const code = ch.codePointAt(0)!;
    if (code < 0x20 || code === 0x7f) {
      return { ok: false, error: "Co-author cannot contain newlines or control characters" };
    }
  }

  const open = trimmed.lastIndexOf("<");
  const close = trimmed.lastIndexOf(">");
  if (open === -1 || close === -1 || close <= open || close !== trimmed.length - 1) {
    return { ok: false, error: 'Invalid co-author: expected the form "Name <email>"' };
  }

  const name = trimmed.slice(0, open).trim();
  const email = trimmed.slice(open + 1, close).trim();

  if (!name) {
    return { ok: false, error: 'Invalid co-author: expected the form "Name <email>"' };
  }
  if (/[<>@]/.test(name)) {
    return {
      ok: false,
      error: "Invalid co-author: the name must not contain '<', '>' or '@'",
    };
  }
  if (!isPlausibleEmail(email)) {
    return { ok: false, error: `Invalid co-author: "${email}" is not a valid email address` };
  }

  return { ok: true, coAuthor: { name, email }, formatted: `${name} <${email}>` };
}

/** Canonical `Name <email>` string for a stored co-author. */
export function formatCoAuthor(c: CoAuthor): string {
  return `${c.name.trim()} <${c.email.trim()}>`;
}

/** Dedup key: emails are compared case-insensitively. */
export function coAuthorEmail(c: CoAuthor): string {
  return c.email.trim().toLowerCase();
}

/** Add a co-author unless its email is already present (returns a new array). */
export function addCoAuthor(list: CoAuthor[], candidate: CoAuthor): CoAuthor[] {
  const key = coAuthorEmail(candidate);
  if (list.some((c) => coAuthorEmail(c) === key)) return list;
  return [...list, candidate];
}

/** Remove every co-author matching the given email (case-insensitive). */
export function removeCoAuthor(list: CoAuthor[], email: string): CoAuthor[] {
  const key = email.trim().toLowerCase();
  return list.filter((c) => coAuthorEmail(c) !== key);
}

/**
 * Emails already present in `Co-authored-by:` trailers within a message.
 * Mirrors the backend's idempotency check so the UI can avoid re-adding someone
 * the user already typed by hand.
 */
export function existingTrailerEmails(message: string): string[] {
  const emails: string[] = [];
  for (const rawLine of message.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (line.length < TRAILER_KEY.length) continue;
    if (line.slice(0, TRAILER_KEY.length).toLowerCase() !== TRAILER_KEY.toLowerCase()) continue;
    const value = line.slice(TRAILER_KEY.length).trim();
    const open = value.lastIndexOf("<");
    const close = value.lastIndexOf(">");
    if (open === -1 || close <= open) continue;
    const email = value.slice(open + 1, close).trim().toLowerCase();
    if (email) emails.push(email);
  }
  return emails;
}
