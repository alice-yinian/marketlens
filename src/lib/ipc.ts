/**
 * IPC 唯一出口（设计文档 §3.2 原则 1/2、§5 的 lib/ipc.ts）。
 *
 * 前端到 Rust 的所有调用都经过这里，组件不直接 import `@tauri-apps/api`。
 * 这样命令面收敛、可审计，也保证前端永远拿不到 SQL 或密钥库的原始能力。
 *
 * 新增命令 = 在 Commands 映射表里加一行，不需要再写包装函数。
 */
import { invoke } from "@tauri-apps/api/core";

import type { AppInfo } from "./types";

/** 命令名 → { 入参, 出参 } 的单一事实来源 */
export interface Commands {
  app_info: { args: undefined; result: AppInfo };
}

/**
 * 类型化分发器：命令名决定出参类型，编译期即可发现前后端契约漂移。
 * 这是刻意的 DI 边界（便于测试时替换、也便于将来审计全部命令面）。
 */
export function call<K extends keyof Commands>(
  cmd: K,
  args?: Commands[K]["args"],
): Promise<Commands[K]["result"]> {
  return invoke<Commands[K]["result"]>(cmd, args as Record<string, unknown> | undefined);
}
