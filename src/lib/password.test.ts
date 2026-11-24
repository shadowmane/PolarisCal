import { describe, expect, it } from "vitest";
import type { User } from "../bindings/User";
import { checkPassword, DEFAULT_PASSWORD_POLICY, describePasswordRules, isPasswordExpired } from "./password";

const strict = {
  min_length: 12,
  require_uppercase: true,
  require_number: true,
  require_symbol: true,
  expiry_days: 90,
};

describe("password rules", () => {
  it("describes the rules", () => {
    expect(describePasswordRules(DEFAULT_PASSWORD_POLICY)).toBe("At least 10 characters.");
    expect(describePasswordRules(strict)).toBe(
      "At least 12 characters, including a capital letter, a number, and a special character such as ! or #.",
    );
    expect(describePasswordRules({ ...strict, require_symbol: false })).toBe(
      "At least 12 characters, including a capital letter and a number.",
    );
  });

  it("says what a new password is missing", () => {
    expect(checkPassword("short", "", strict)).toBe("Use at least 12 characters.");
    expect(checkPassword("long enough pass", "", strict)).toBe(
      "Include a capital letter, a number, and a special character such as ! or #.",
    );
    // A space isn't a special character; punctuation is.
    expect(checkPassword("Long enough 2026", "", strict)).toBe("Include a special character such as ! or #.");
    expect(checkPassword("Long-enough 2026", "", strict)).toBeUndefined();
    expect(checkPassword("jsmith12345", "JSMITH12345", DEFAULT_PASSWORD_POLICY)).toMatch(/username/);
  });

  it("knows when a password has expired", () => {
    const now = new Date("2026-11-24T12:00:00Z");
    const user = (changed: string | null) => ({ password_changed_at: changed }) as User;
    expect(isPasswordExpired(user("2026-08-27T12:00:00Z"), strict, now)).toBe(false); // 89 days
    expect(isPasswordExpired(user("2026-08-26T12:00:00Z"), strict, now)).toBe(true); // 90 days
    expect(isPasswordExpired(user(null), strict, now)).toBe(false);
    expect(isPasswordExpired(user("2020-01-01T00:00:00Z"), DEFAULT_PASSWORD_POLICY, now)).toBe(false);
  });
});
