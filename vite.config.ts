import { readFileSync } from "node:fs";

import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

const { version } = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as {
  version: string;
};

// The dev server Tauri loads (`tauri dev`); `vite --mode mock` serves the UI
// in a plain browser with the core faked (see src/dev/mock.ts).
export default defineConfig({
  plugins: [svelte()],
  define: { __APP_VERSION__: JSON.stringify(version) },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: ["es2022", "chrome105", "safari15"],
    sourcemap: false,
  },
  // Under vitest, Svelte's browser build (not its server build).
  ...(process.env.VITEST ? { resolve: { conditions: ["browser"] } } : {}),
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    restoreMocks: true,
  },
});
