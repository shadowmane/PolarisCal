// The organization's password rules (Settings > Administration), so every password form
// can describe them and check a new password before sending it. The backend checks the
// same rules (`PasswordPolicy` in polaris-core); its message wins if they ever differ.

import type { PasswordPolicy } from "../bindings/PasswordPolicy";
import type { User } from "../bindings/User";

export const DEFAULT_PASSWORD_POLICY: PasswordPolicy = {
  min_length: 10,
  require_uppercase: false,
  require_number: false,
  require_symbol: false,
  expiry_days: null,
};

let current: PasswordPolicy = DEFAULT_PASSWORD_POLICY;

/** Set from the auth status whenever it loads. */
export function setPasswordPolicy(policy: PasswordPolicy | undefined) {
  current = policy ?? DEFAULT_PASSWORD_POLICY;
}

export function passwordPolicy(): PasswordPolicy {
  return current;
}

/** "a", "a and b", "a, b, and c". */
function joinAnd(items: string[]): string {
  if (items.length <= 1) return items.join("");
  if (items.length === 2) return `${items[0]} and ${items[1]}`;
  return `${items.slice(0, -1).join(", ")}, and ${items[items.length - 1]}`;
}

function required(policy: PasswordPolicy): string[] {
  const items: string[] = [];
  if (policy.require_uppercase) items.push("a capital letter");
  if (policy.require_number) items.push("a number");
  if (policy.require_symbol) items.push("a special character such as ! or #");
  return items;
}

/** e.g. "At least 12 characters, including a capital letter and a number." */
export function describePasswordRules(policy: PasswordPolicy = current): string {
  const needs = required(policy);
  return needs.length
    ? `At least ${policy.min_length} characters, including ${joinAnd(needs)}.`
    : `At least ${policy.min_length} characters.`;
}

const isUpper = (c: string) => c !== c.toLowerCase() && c === c.toUpperCase();
const isNumber = (c: string) => /\p{N}/u.test(c);
const isSymbol = (c: string) => !/[\p{L}\p{N}\s]/u.test(c);

/** What's wrong with a new password, or undefined when it follows the rules. */
export function checkPassword(password: string, username = "", policy: PasswordPolicy = current): string | undefined {
  const chars = [...password];
  if (chars.length < policy.min_length) return `Use at least ${policy.min_length} characters.`;
  const missing: string[] = [];
  if (policy.require_uppercase && !chars.some(isUpper)) missing.push("a capital letter");
  if (policy.require_number && !chars.some(isNumber)) missing.push("a number");
  if (policy.require_symbol && !chars.some(isSymbol)) missing.push("a special character such as ! or #");
  if (missing.length) return `Include ${joinAnd(missing)}.`;
  if (username && password.trim().toLowerCase() === username.trim().toLowerCase()) {
    return "Your password can't be the same as your username.";
  }
  return undefined;
}

/** Whether this user's password has passed its expiry (the backend makes them change it). */
export function isPasswordExpired(user: User, policy: PasswordPolicy = current, now = new Date()): boolean {
  if (policy.expiry_days == null || !user.password_changed_at) return false;
  const age = now.getTime() - new Date(user.password_changed_at).getTime();
  return age >= policy.expiry_days * 24 * 60 * 60 * 1000;
}
