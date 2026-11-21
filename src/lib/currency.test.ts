import { describe, expect, it } from "vitest";
import { CURRENCY_SYMBOLS, currencySymbol, formatMoney, setCurrencySymbol } from "./currency";

describe("currency", () => {
  it("lists dollar, euro, and pound first", () => {
    expect(CURRENCY_SYMBOLS.slice(0, 3).map((c) => c.id)).toEqual(["$", "€", "£"]);
  });

  it("puts the symbol before the amount, with a space after letter symbols", () => {
    expect(formatMoney(1234.5, "€")).toMatch(/^€1.234.50$/);
    expect(formatMoney(125, "kr")).toMatch(/^kr 125.00$/);
    expect(formatMoney(125, "R$")).toMatch(/^R\$125.00$/);
  });

  it("falls back to the dollar for a blank setting", () => {
    setCurrencySymbol("£");
    expect(currencySymbol()).toBe("£");
    setCurrencySymbol("  ");
    expect(currencySymbol()).toBe("$");
  });
});
