/**
 * IPC 错误载荷归一化。
 *
 * Rust 侧 `AppError` 的 `Serialize` 实现输出 `{ code, message, retryable }`
 * （见 src-tauri/src/error.rs），Tauri 会把该值原样作为 Promise 的 reject 原因。
 * 但 reject 原因在 JS 侧是 `unknown`：序列化失败时可能是字符串，
 * 所以这里做一次收敛，让界面永远拿到同一种形状。
 */
export interface ErrorPayload {
  code: string;
  message: string;
  /** 只有为 true 才展示「重试」按钮 */
  retryable: boolean;
}

const UNKNOWN_CODE = "Unknown";

export function errorPayloadOf(error: unknown): ErrorPayload {
  if (error instanceof Error) {
    return { code: UNKNOWN_CODE, message: error.message, retryable: false };
  }
  if (typeof error === "object" && error !== null) {
    const record = error as Record<string, unknown>;
    const message = record["message"] ?? record["error"];
    return {
      code: typeof record["code"] === "string" ? record["code"] : UNKNOWN_CODE,
      message: typeof message === "string" ? message : JSON.stringify(record),
      retryable: record["retryable"] === true,
    };
  }
  return { code: UNKNOWN_CODE, message: String(error), retryable: false };
}
