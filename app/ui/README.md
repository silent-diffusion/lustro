# Lustro UI

Svelte 5 + TypeScript + Vite. Runs in a plain browser against a mock backend with fake data, and inside the desktop app over
Tauri IPC (same `Backend` interface, `src/ipc/backend.ts`).

```bash
npm install
npm run dev      # http://localhost:5173
npm run check    # svelte-check + tsc
npm test         # vitest
```

## Layout
- `src/ipc/` contract types and backend interface (see `docs/CONTRACTS.md`), plus the mock:
  - `mockData.ts`: the fake sample tree from `design/prototype/data.js`, per platform. Fake paths only.
  - `mockTree.ts`: expands it into a large deterministic tree stored in typed arrays (1M nodes builds in about a second).
  - `mock.ts`: `MockBackend`, with a simulated live scan (about 10 updates a second), snapshots and diffs, cleanup with undo, settings and updates.
- `src/lib/`: app state (`app.svelte.ts`), formatting, labels and per-OS wording, squarified treemap, canvas helpers, the terrain adapter.
- `src/components/`: shell, canvas block map and file tree (`CanvasMap`), ring chart, lazy folder tree, virtual list, terrain host.
- `src/screens/`: Overview, Live scan, Explore, What grew, Cleanup, Settings.
- `src/terrain/`: the 3D terrain module (separate owner), loaded lazily through `src/lib/terrain.ts`.

## Dev switches (mock only)
Settings → Developer preview, or URL parameters:
- `?platform=windows|macos|linux`: sample paths, wording, shortcuts.
- `?nodes=1000000`: fake tree size (stress test).
- `?fresh`: start with no scan.

## Shortcuts
Ctrl/⌘ + 1–4 tabs · Ctrl/⌘+R scan · Ctrl/⌘+, settings · Ctrl/⌘+F search. Cleanup: R remove, K keep, L later, ← → move.
