// Regenerates THIRD-PARTY-NOTICES.md from the real dependency trees:
//   - Rust: `cargo metadata` (Cargo.lock, every target platform);
//   - npm:  app/ui/package-lock.json, plus the license texts in app/ui/node_modules.
// Usage (from the repo root, after `npm ci` in app/ui):  node scripts/third-party-notices.mjs
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const ui = join(root, "app", "ui");
const nm = join(ui, "node_modules");

// npm packages whose code or files end up in the app bundle (app/ui/dist). Everything else in
// package-lock.json is a build or test tool. Svelte is a dev dependency but compiles its runtime
// into the bundle; the fonts are imported as files.
const SHIPPED_NPM = new Set([
  "three", "@tauri-apps/api", "svelte", "clsx", "esm-env",
  "@fontsource/manrope", "@fontsource/jetbrains-mono",
]);

// --- npm
const lock = JSON.parse(readFileSync(join(ui, "package-lock.json"), "utf8")).packages;
const npm = new Map();
for (const [key, p] of Object.entries(lock)) {
  if (!key) continue;
  const name = key.split("node_modules/").pop();
  if (p.optional && /^(lightningcss|@rollup|@esbuild|@rolldown)[-/].*(android|darwin|freebsd|linux|win32|openharmony|wasm)/.test(name)) continue;
  npm.set(name, { name, version: p.version, license: p.license ?? "UNKNOWN", shipped: SHIPPED_NPM.has(name) });
}
const npmRows = [...npm.values()].sort((a, b) => a.name.localeCompare(b.name));

// --- Rust
const meta = JSON.parse(execFileSync("cargo", ["metadata", "--format-version", "1", "--locked"], {
  cwd: root, maxBuffer: 64 * 1024 * 1024, encoding: "utf8",
}));
const members = new Set(meta.workspace_members);
const crates = meta.packages
  .filter((p) => !members.has(p.id))
  .map((p) => ({ name: p.name, version: p.version, license: p.license ?? "UNKNOWN" }))
  .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));

const byLicense = (rows) => {
  const m = new Map();
  for (const r of rows) m.set(r.license, [...(m.get(r.license) ?? []), r]);
  return [...m.entries()].sort((a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0]));
};

const ofl = readFileSync(join(nm, "@fontsource", "manrope", "LICENSE"), "utf8");
const oflText = ofl.slice(ofl.indexOf("This Font Software is licensed")).trim();
const threeLicense = readFileSync(join(nm, "three", "LICENSE"), "utf8").trim();
const tauriApiVersion = npm.get("@tauri-apps/api")?.version;
const threeVersion = npm.get("three")?.version;
const svelteVersion = npm.get("svelte")?.version;

const out = [];
out.push(`# Third-party notices

Lustro is MIT-licensed (see [LICENSE](LICENSE)). It ships with, or is built from, the third-party
works below, which keep their own licenses. This file is generated from \`Cargo.lock\` and
\`app/ui/package-lock.json\` by \`node scripts/third-party-notices.mjs\`.

## Shipped in the app

### Fonts

| Font | Copyright | License |
|---|---|---|
| Manrope | Copyright 2019 The Manrope Project Authors (https://github.com/sharanda/manrope) | SIL Open Font License 1.1 |
| JetBrains Mono | Copyright 2020 The JetBrains Mono Project Authors (https://github.com/JetBrains/JetBrainsMono) | SIL Open Font License 1.1 |

Bundled unmodified from the [Fontsource](https://fontsource.org) packages \`@fontsource/manrope\` and
\`@fontsource/jetbrains-mono\`. The design prototype (\`design/prototype/vendor/fonts\`) carries
copies of the same fonts with their license files. The full license text is at the end of this file.

### JavaScript

| Package | Version | License |
|---|---|---|
| [three.js](https://github.com/mrdoob/three.js) (\`three\`; also vendored in \`design/prototype/vendor/three\`) | ${threeVersion} | MIT, Copyright © 2010-2026 three.js authors |
| [\`@tauri-apps/api\`](https://github.com/tauri-apps/tauri) | ${tauriApiVersion} | Apache-2.0 OR MIT |
| [Svelte](https://github.com/sveltejs/svelte) runtime (compiled into the app), with \`clsx\` and \`esm-env\` | ${svelteVersion} | MIT |

### Rust crates

The app is built on [Tauri](https://tauri.app) and the crates below, which are compiled into it.
The list covers every target platform in \`Cargo.lock\` (Windows, macOS, Linux and others), so each
build uses a subset. CI checks with \`cargo deny\` ([deny.toml](deny.toml)) that every license is
on the allowed list.
`);
for (const [lic, rows] of byLicense(crates)) {
  out.push(`<details>\n<summary><b>${lic}</b> (${rows.length})</summary>\n`);
  out.push(rows.map((r) => `${r.name} ${r.version}`).join(", ") + "\n\n</details>\n");
}

out.push(`### Windows installer

- Built with [NSIS](https://nsis.sourceforge.io) (zlib/libpng License).
- Embeds the Microsoft Edge WebView2 Runtime bootstrapper, which Microsoft allows applications to
  redistribute under the [WebView2 distribution terms](https://developer.microsoft.com/microsoft-edge/webview2/).

## Build and test tools (not shipped)

These npm packages are used to build or test the interface and are not part of the app. Among
them, \`lightningcss\` (MPL-2.0) is the CSS minifier, and \`@dimforge/rapier3d-compat\` (Apache-2.0)
is only a type dependency of \`@types/three\`; neither ends up in the app.
`);
for (const [lic, rows] of byLicense(npmRows.filter((r) => !r.shipped))) {
  out.push(`<details>\n<summary><b>${lic}</b> (${rows.length})</summary>\n`);
  out.push(rows.map((r) => `${r.name} ${r.version}`).join(", ") + "\n\n</details>\n");
}

out.push(`---

## SIL Open Font License 1.1 (Manrope, JetBrains Mono)

\`\`\`
${oflText}
\`\`\`

---

## MIT License (three.js)

\`\`\`
${threeLicense}
\`\`\`
`);

writeFileSync(join(root, "THIRD-PARTY-NOTICES.md"), out.join("\n").replace(/\r\n/g, "\n"));
console.log(`THIRD-PARTY-NOTICES.md: ${crates.length} crates, ${npmRows.length} npm packages`);
