import { defineConfig } from "vitest/config";

/**
 * 独立于 `vite.config.ts`：只覆盖 `src/lib/` 纯函数/工具函数的单元测试，
 * 不引入 jsdom、不跑组件测试，避免和 Tauri 打包用的 vite 配置产生耦合。
 */
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
