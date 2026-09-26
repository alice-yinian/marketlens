/**
 * 值防抖：输入停止 `delayMs` 后才把最新值暴露出去。
 *
 * 用于模板编辑器的正文：连续敲键期间不重复调用后端，停手 300ms 才做**语法校验**。
 * 生成提示词不走这里——它是显式点击触发的 mutation，跟输入节奏无关。
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
