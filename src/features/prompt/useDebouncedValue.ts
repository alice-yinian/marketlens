/**
 * 值防抖：输入停止 `delayMs` 后才把最新值暴露出去。
 *
 * 用于编辑器的正文：连续敲键期间不重复调用后端，停手 300ms 才重新预览。
 * 隐私等级**不**走这里——切等级必须立即重新生成。
 */
import { useEffect, useState } from "react";

export function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);

  return debounced;
}
