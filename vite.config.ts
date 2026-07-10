import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "esnext",
    rollupOptions: {
      input: {
        popup: resolve(__dirname, "popup.html"),
        settings: resolve(__dirname, "settings.html"),
      },
    },
  },
});
