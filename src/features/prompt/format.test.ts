/**
 * 提示词库纯函数测试：保存前的本地校验、另存为命名、导出文件名、预览统计。
 *
 * 隐私等级与时段校验**不在这个模块**：隐私属于设置页（`features/settings/format.ts`），
 * 时段校验属于复盘页（`features/review/format.ts`）——同一个概念只留一份实现，
 * 所以这里不为它们留任何用例。
 */
import { describe, expect, it } from "vitest";

import { S } from "../../lib/strings";
import {
  exportFileName,
  outputStats,
  saveAsName,
  templateProblem,
  templateProblemText,
} from "./format";

describe("模板保存校验与另存为命名", () => {
  it("名称或正文去空白后为空都算问题", () => {
    expect(templateProblem("  ", "body")).toBe("nameRequired");
    expect(templateProblem("name", " \n ")).toBe("bodyRequired");
    expect(templateProblem("name", "body")).toBeNull();
  });

  it("问题码有人话说明（不是把码直接摊给用户）", () => {
    expect(templateProblemText("nameRequired")).toBe(S.prompt.actions.nameRequired);
    expect(templateProblemText("bodyRequired")).toBe(S.prompt.actions.bodyRequired);
    expect(templateProblemText("nameRequired")).not.toContain("nameRequired");
  });

  it("从同名内置模板另存为时追加副本后缀", () => {
    expect(saveAsName("实盘速览", "实盘速览")).toBe(
      `实盘速览${S.prompt.actions.saveAsCopySuffix}`,
    );
    expect(saveAsName("我的模板", "实盘速览")).toBe("我的模板");
    expect(saveAsName("我的模板", null)).toBe("我的模板");
  });
});

describe("导出文件名", () => {
  const ms = new Date(2026, 8, 25, 10, 20, 30).getTime();

  it("形如 {模板名}-{等级}-{时间戳}.md", () => {
    expect(exportFileName("实盘速览", "L1", ms)).toBe("实盘速览-L1-20260925-102030.md");
  });

  it("非法文件名字符被替换，空名回落", () => {
    expect(exportFileName("a/b:c d", "L0", ms)).toBe("a_b_c_d-L0-20260925-102030.md");
    expect(exportFileName("   ", "L2", ms)).toBe("template-L2-20260925-102030.md");
  });
});

describe("outputStats", () => {
  it("透传 token 与字符数", () => {
    expect(outputStats({ token_estimate: 123, char_count: 456 })).toEqual({
      tokens: 123,
      chars: 456,
    });
  });

  it("非有限值回落为 0（不显示 NaN）", () => {
    expect(
      outputStats({ token_estimate: Number.NaN, char_count: Number.POSITIVE_INFINITY }),
    ).toEqual({ tokens: 0, chars: 0 });
  });
});
