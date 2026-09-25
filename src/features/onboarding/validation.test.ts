import { describe, expect, it } from "vitest";

import { S } from "../../lib/strings";
import {
  WATCHLIST_LIMIT,
  validateVaultCreate,
  validateVaultUnlock,
  validateWatchlistSelection,
} from "./validation";

describe("validateVaultCreate", () => {
  it("两次密码不一致时拒绝提交并说明原因", () => {
    expect(validateVaultCreate("correct-horse", "correct-hors")).toBe(
      S.onboarding.vault.mismatch,
    );
  });

  it("空密码时拒绝提交（后端也会拒绝空密码）", () => {
    expect(validateVaultCreate("", "")).toBe(S.onboarding.vault.empty);
  });

  it("非空且一致时放行", () => {
    expect(validateVaultCreate("correct-horse", "correct-horse")).toBeNull();
  });
});

describe("validateVaultUnlock", () => {
  it("空密码拒绝，非空放行", () => {
    expect(validateVaultUnlock("")).toBe(S.onboarding.vault.empty);
    expect(validateVaultUnlock("hunter2")).toBeNull();
  });
});

describe("validateWatchlistSelection", () => {
  it("少于 1 个时拒绝", () => {
    expect(validateWatchlistSelection(0)).toBe(S.onboarding.watchlist.needOne);
  });

  it("超过上限时拒绝，并说明「标的数乘进复盘请求量」的原因", () => {
    const message = validateWatchlistSelection(WATCHLIST_LIMIT + 1);
    expect(message).not.toBeNull();
    expect(message).toContain(String(WATCHLIST_LIMIT));
    expect(message).toContain("复盘请求量");
  });

  it("1 个与刚好到上限都放行", () => {
    expect(validateWatchlistSelection(1)).toBeNull();
    expect(validateWatchlistSelection(WATCHLIST_LIMIT)).toBeNull();
  });
});
