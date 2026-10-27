import { describe, it, expect } from "vitest";
import {
  addCoAuthor,
  coAuthorEmail,
  existingTrailerEmails,
  formatCoAuthor,
  parseCoAuthor,
  removeCoAuthor,
  type CoAuthor,
} from "../src/lib/domain/coauthors";

describe("parseCoAuthor", () => {
  it("accepts and normalizes a Name <email> entry", () => {
    const res = parseCoAuthor("  Jane Doe <jane@example.com>  ");
    expect(res.ok).toBe(true);
    if (res.ok) {
      expect(res.coAuthor).toEqual({ name: "Jane Doe", email: "jane@example.com" });
      expect(res.formatted).toBe("Jane Doe <jane@example.com>");
    }
  });

  it("rejects an empty entry", () => {
    expect(parseCoAuthor("   ").ok).toBe(false);
  });

  it("rejects entries without an email in angle brackets", () => {
    expect(parseCoAuthor("Jane Doe").ok).toBe(false);
    expect(parseCoAuthor("Jane Doe <jane@example.com").ok).toBe(false);
    expect(parseCoAuthor("<jane@example.com>").ok).toBe(false);
  });

  it("rejects an invalid email address", () => {
    expect(parseCoAuthor("Jane Doe <jane example.com>").ok).toBe(false);
    expect(parseCoAuthor("Jane Doe <jane@>").ok).toBe(false);
    expect(parseCoAuthor("Jane Doe <@example.com>").ok).toBe(false);
  });

  it("rejects newlines and other injection attempts", () => {
    expect(parseCoAuthor("Jane <jane@example.com>\nSigned-off-by: Evil <e@e.com>").ok).toBe(false);
    // Trailing text after the closing bracket (would look like a flag).
    expect(parseCoAuthor("Jane <jane@example.com> --author=evil").ok).toBe(false);
    expect(parseCoAuthor("Jane <jane@example.com> -m extra").ok).toBe(false);
  });

  it("rejects names containing angle brackets or @", () => {
    expect(parseCoAuthor("Ja<ne> <jane@example.com>").ok).toBe(false);
    expect(parseCoAuthor("jane@work <jane@example.com>").ok).toBe(false);
  });
});

describe("co-author list helpers", () => {
  const jane: CoAuthor = { name: "Jane Doe", email: "jane@example.com" };

  it("formats a co-author canonically", () => {
    expect(formatCoAuthor(jane)).toBe("Jane Doe <jane@example.com>");
  });

  it("compares emails case-insensitively", () => {
    expect(coAuthorEmail({ name: "J", email: "  JANE@Example.com " })).toBe("jane@example.com");
  });

  it("does not add a duplicate email", () => {
    const list = addCoAuthor([], jane);
    const again = addCoAuthor(list, { name: "Jane Again", email: "JANE@example.com" });
    expect(again).toHaveLength(1);
    expect(again).toBe(list);
  });

  it("adds distinct co-authors", () => {
    const list = addCoAuthor(addCoAuthor([], jane), { name: "Sam", email: "sam@example.com" });
    expect(list).toHaveLength(2);
  });

  it("removes by email case-insensitively", () => {
    const list = [jane, { name: "Sam", email: "sam@example.com" }];
    const removed = removeCoAuthor(list, "JANE@example.com");
    expect(removed).toEqual([{ name: "Sam", email: "sam@example.com" }]);
  });
});

describe("existingTrailerEmails", () => {
  it("finds emails in Co-authored-by trailers (case-insensitive)", () => {
    const message = "add feature\n\nCo-Authored-By: Jane <JANE@example.com>";
    expect(existingTrailerEmails(message)).toEqual(["jane@example.com"]);
  });

  it("returns nothing when there are no trailers", () => {
    expect(existingTrailerEmails("just a subject\n\nand a body")).toEqual([]);
  });
});
