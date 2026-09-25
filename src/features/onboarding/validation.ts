/**
 * 引导流程的纯校验函数：不依赖 React、不读全局状态，输入相同则输出相同。
 *
 * 把「能不能点下一步」从组件里抽出来，是为了让边界（两次密码不一致、
 * 标的数超上限）能被单元测试直接钉住——这些正是会悄悄放行错误状态的路径。
 */
import { S } from "../../lib/strings";

/**
 * 标的数量上限。与 Rust 侧 `settings::WATCHLIST_LIMIT` 一致：
 * 后端仍会再校验一次，这里只是为了让用户在提交前就拿到明确原因。
 */
export const WATCHLIST_LIMIT = 10;

/** 创建密钥库：非空 + 两次一致。返回错误文案；`null` 表示可以提交。 */
export function validateVaultCreate(password: string, confirm: string): string | null {
  if (password.length === 0) return S.onboarding.vault.empty;
  if (password !== confirm) return S.onboarding.vault.mismatch;
  return null;
}

/** 解锁密钥库：只要求非空（后端还会再判一次空密码）。 */
export function validateVaultUnlock(password: string): string | null {
  return password.length === 0 ? S.onboarding.vault.empty : null;
}

/**
 * 标的集选择：至少 1 个、最多 `WATCHLIST_LIMIT` 个。
 * 返回错误文案；`null` 表示可以提交。
 */
export function validateWatchlistSelection(count: number): string | null {
  if (count < 1) return S.onboarding.watchlist.needOne;
  if (count > WATCHLIST_LIMIT) return S.onboarding.watchlist.tooMany(WATCHLIST_LIMIT);
  return null;
}
