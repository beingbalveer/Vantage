import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultPaneWidths, savedPaneWidth } from "../src/lib/app/layout";
afterEach(() => vi.unstubAllGlobals());
describe("approved layout", () => {
  it.each([
    [1280, {sidebar:222,commits:244,files:235}],
    [1400, {sidebar:222,commits:244,files:235}],
    [1600, {sidebar:246,commits:274,files:260}],
    [1750, {sidebar:268,commits:292,files:300}],
  ])("matches prototype at %i pixels", (width, expected) => {
    expect(defaultPaneWidths(width)).toEqual(expected);
  });
  it("preserves a valid saved width", () => {
    vi.stubGlobal("localStorage", {getItem:()=>"310"});
    expect(savedPaneWidth("commits",274)).toBe(310);
  });
  it.each([null,"bad","0","-50","Infinity"])("uses default for invalid saved value %s", value => {
    vi.stubGlobal("localStorage", {getItem:()=>value});
    expect(savedPaneWidth("commits",274)).toBe(274);
  });
  it("survives storage access failure", () => {
    vi.stubGlobal("localStorage", {getItem:()=>{throw Error("unavailable")}});
    expect(savedPaneWidth("files",260)).toBe(260);
  });
});
