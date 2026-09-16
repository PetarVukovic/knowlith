import fs from "node:fs"
import os from "node:os"
import path from "node:path"
import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import tailwindcss from "@tailwindcss/vite"

/**
 * The daemon's API token, read from disk.
 *
 * In the shipped product the page comes from the daemon, which writes the
 * token into the HTML. Under `npm run dev` the page comes from Vite, so
 * instead of handing a browser on another port a way through the daemon's
 * front door, Vite forwards `/api` itself and attaches the token here — in
 * Node, where a web page cannot reach it. The browser stays same-origin
 * and never sees the secret at all.
 *
 * Read once at startup. `scripts/dev.sh` starts the daemon first, so the
 * file is already there; started the other way round, the fix is to
 * restart Vite, which is what the warning below says.
 */
const token = (() => {
  const root = process.env.KNOWLITH_HOME ?? path.join(os.homedir(), "Knowlith")
  const file = path.join(root, "api.token")
  try {
    const value = fs.readFileSync(file, "utf8").trim()
    if (value) return value
  } catch {
    /* reported below */
  }
  console.warn(`\n  No API token at ${file} — start the daemon, then restart this.\n`)
  return ""
})()

const daemon = `http://127.0.0.1:${process.env.KNOWLITH_PORT ?? "7717"}`

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": path.resolve(import.meta.dirname, "./src") },
  },
  server: {
    proxy: {
      "/api": {
        target: daemon,
        changeOrigin: false,
        headers: token ? { "x-knowlith-token": token } : undefined,
      },
    },
  },
})
