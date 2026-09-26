import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import "./styles.css";

// 首屏着色：设置存在 SQLite 里，只能异步读回来，所以先按系统偏好猜一帧。
// 猜对了（跟随系统的用户）就没有闪烁；显式选了另一档的用户会看到一帧反色，
// 这是「不让本地存第二份主题状态」换来的一致性——两份状态迟早会不一致。
if (typeof window !== "undefined" && typeof window.matchMedia === "function") {
  document.documentElement.dataset.theme = window.matchMedia("(prefers-color-scheme: dark)")
    .matches
    ? "dark"
    : "light";
}

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // 按需采集模型（ADR #10）：数据由 TTL 缓存层决定新鲜度，
      // 前端不做自动轮询，也不在窗口聚焦时偷偷重新拉取。
      refetchOnWindowFocus: false,
      refetchOnReconnect: false,
      retry: 1,
      staleTime: Infinity,
    },
  },
});

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("#root 未找到：index.html 与 main.tsx 不匹配");
}

createRoot(rootElement).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
