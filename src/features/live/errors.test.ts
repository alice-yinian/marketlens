import { describe, expect, it } from "vitest";

import { errorPayloadOf } from "./errors";

describe("errorPayloadOf", () => {
  it("透传 Rust AppError 的序列化载荷", () => {
    expect(errorPayloadOf({ code: "Http", message: "网络请求失败：超时", retryable: true })).toEqual({
      code: "Http",
      message: "网络请求失败：超时",
      retryable: true,
    });
  });

  it("retryable 非 true 时不显示重试按钮（Migration / Config / Okx 都不可重试）", () => {
    expect(errorPayloadOf({ code: "Migration", message: "迁移失败", retryable: false }).retryable).toBe(
      false,
    );
    expect(errorPayloadOf({ code: "Okx", message: "参数错误" }).retryable).toBe(false);
  });

  it("字符串错误退化为不可重试的 Unknown", () => {
    expect(errorPayloadOf("boom")).toEqual({ code: "Unknown", message: "boom", retryable: false });
  });

  it("Error 实例取 message，不把 code 误当成可重试", () => {
    expect(errorPayloadOf(new Error("invoke failed"))).toEqual({
      code: "Unknown",
      message: "invoke failed",
      retryable: false,
    });
  });
});
