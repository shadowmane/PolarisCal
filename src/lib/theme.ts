// Light, dark, or follow Windows. One shared value, so the sidebar button and the
// Appearance setting always agree (HeroUI's own useTheme keeps a separate copy per use).

import { useSyncExternalStore } from "react";

export type ThemeChoice = "light" | "dark" | "system";

/** The key HeroUI used, so the choice people already made carries over. */
const STORAGE_KEY = "heroui-theme";
const PREFERS_DARK = "(prefers-color-scheme: dark)";

function stored(): ThemeChoice {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    // Storage unavailable: fall back to following Windows.
  }
  return "system";
}

let choice: ThemeChoice = stored();
const listeners = new Set<() => void>();
const media = window.matchMedia?.(PREFERS_DARK);

function systemTheme(): "light" | "dark" {
  return media?.matches ? "dark" : "light";
}

function resolve(c: ThemeChoice): "light" | "dark" {
  return c === "system" ? systemTheme() : c;
}

function apply() {
  const theme = resolve(choice);
  const root = document.documentElement;
  root.classList.remove(theme === "dark" ? "light" : "dark");
  root.classList.add(theme);
  root.setAttribute("data-theme", theme);
}

function notify() {
  apply();
  listeners.forEach((l) => l());
}

export function setTheme(next: ThemeChoice) {
  choice = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // Not remembered for next time, but still applied now.
  }
  notify();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

// Windows switching between light and dark matters while following it.
media?.addEventListener("change", () => {
  if (choice === "system") notify();
});
apply();

/** The choice (`theme`), what it currently means (`resolvedTheme`), and `setTheme`. */
export function useTheme() {
  const theme = useSyncExternalStore(subscribe, () => choice);
  // Re-renders on Windows' changes too, because notify() runs for them.
  const resolvedTheme = useSyncExternalStore(subscribe, () => resolve(choice));
  return { theme, resolvedTheme, setTheme };
}
