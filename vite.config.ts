import { defineConfig, configDefaults } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;
const port = Number(process.env.VITE_PORT ?? 5174);

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return;
          if (id.includes("@xyflow/react")) return "xyflow";
          if (id.includes("react-markdown") || id.includes("remark-gfm")) {
            return "markdown";
          }
          if (id.includes("@tauri-apps")) return "tauri";
          if (
            id.includes("/react-dom/") ||
            id.includes("/react/") ||
            id.includes("/react-is/")
          ) {
            return "react";
          }
        },
      },
    },
  },
  server: {
    port,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  test: {
    exclude: [
      ...configDefaults.exclude,
      ".claude/**",
      "e2e/**",
    ],
  },
});
