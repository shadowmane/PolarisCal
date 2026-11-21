// The organization's currency symbol (an admin setting), shared by every money display.

import { useSyncExternalStore } from "react";

/** Dollar, euro, and pound first, then other common symbols. */
export const CURRENCY_SYMBOLS: { id: string; label: string }[] = [
  { id: "$", label: "$  Dollar" },
  { id: "€", label: "€  Euro" },
  { id: "£", label: "£  Pound sterling" },
  { id: "¥", label: "¥  Yen / Yuan" },
  { id: "₹", label: "₹  Rupee" },
  { id: "₩", label: "₩  Won" },
  { id: "₱", label: "₱  Philippine peso" },
  { id: "₪", label: "₪  Shekel" },
  { id: "₺", label: "₺  Lira" },
  { id: "₫", label: "₫  Dong" },
  { id: "฿", label: "฿  Baht" },
  { id: "₦", label: "₦  Naira" },
  { id: "R", label: "R  Rand" },
  { id: "R$", label: "R$  Real" },
  { id: "kr", label: "kr  Krona / Krone" },
  { id: "zł", label: "zł  Złoty" },
  { id: "Kč", label: "Kč  Koruna" },
  { id: "Ft", label: "Ft  Forint" },
  { id: "CHF", label: "CHF  Swiss franc" },
];

let symbol = "$";
const listeners = new Set<() => void>();

export function currencySymbol(): string {
  return symbol;
}

export function setCurrencySymbol(next: string | null | undefined) {
  const value = next?.trim() || "$";
  if (value === symbol) return;
  symbol = value;
  for (const l of listeners) l();
}

/** Re-renders the caller when the currency changes. */
export function useCurrencySymbol(): string {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => symbol,
  );
}

const amount = new Intl.NumberFormat(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });

/** "$1,234.50", "€1,234.50", or "kr 1,234.50" (letter symbols get a space). */
export function formatMoney(value: number, sym = symbol): string {
  const space = /^\p{L}{2,}$/u.test(sym) ? " " : "";
  return `${sym}${space}${amount.format(value)}`;
}
