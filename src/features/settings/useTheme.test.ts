import { describe, expect, it } from "vitest";

import { DEFAULT_THEME, resolveTheme } from "./useTheme";

describe("主题解析", () => {
  it("显式档位直接生效，与系统偏好无关", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("light", false)).toBe("light");
    expect(resolveTheme("dark", true)).toBe("dark");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("跟随系统时由系统偏好决定", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });

  /// 默认值必须与 Rust `Theme::default()` 一致：不一致会让首屏先按一个值着色、
  /// 读回设置后再改一次，而用户会看到一次闪烁。
  it("默认是跟随系统", () => {
    expect(DEFAULT_THEME).toBe("system");
  });
});
