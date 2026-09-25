import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import "./styles.css";

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
