// Installs only the Tauri CLI, exactly as pinned in app/ui/package-lock.json, for the release
// workflow's signing job: `@tauri-apps/cli` and its native package for this platform, nothing
// else. Each tarball is downloaded from the URL in the lockfile and checked against the lockfile's
// sha512 integrity before it is unpacked. No npm, no install scripts.
//
// Usage:  node fetch-tauri-cli.mjs <package-lock.json> <out-dir>
// Prints the path of the CLI entry point (<out-dir>/node_modules/@tauri-apps/cli/tauri.js).
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";

const [lockPath, outArg] = process.argv.slice(2);
if (!lockPath || !outArg) throw new Error("usage: node fetch-tauri-cli.mjs <package-lock.json> <out-dir>");
const out = resolve(outArg);

const NATIVE = {
  "win32-x64": "cli-win32-x64-msvc",
  "linux-x64": "cli-linux-x64-gnu",
  "darwin-arm64": "cli-darwin-arm64",
  "darwin-x64": "cli-darwin-x64",
}[`${process.platform}-${process.arch}`];
if (!NATIVE) throw new Error(`no Tauri CLI package for ${process.platform}-${process.arch}`);

const lock = JSON.parse(readFileSync(lockPath, "utf8")).packages;
rmSync(out, { recursive: true, force: true });

for (const name of ["cli", NATIVE]) {
  const key = `node_modules/@tauri-apps/${name}`;
  const p = lock[key];
  if (!p?.resolved || !p?.integrity) throw new Error(`${key} is not in the lockfile`);
  if (!p.resolved.startsWith("https://registry.npmjs.org/@tauri-apps/")) throw new Error(`${key}: unexpected URL`);
  const [algo, expected] = p.integrity.split("-", 2);
  if (algo !== "sha512") throw new Error(`${key}: expected a sha512 integrity`);

  const res = await fetch(p.resolved);
  if (!res.ok) throw new Error(`${key}: download failed (${res.status})`);
  const tgz = Buffer.from(await res.arrayBuffer());
  const actual = createHash("sha512").update(tgz).digest("base64");
  if (actual !== expected) throw new Error(`${key}: sha512 doesn't match the lockfile`);

  const dest = join(out, "node_modules", "@tauri-apps", name);
  mkdirSync(dest, { recursive: true });
  const file = join(out, `${name}.tgz`);
  writeFileSync(file, tgz);
  // npm tarballs keep everything under "package/". Relative paths: GNU tar reads "C:" as a host.
  const rel = join("node_modules", "@tauri-apps", name);
  execFileSync("tar", ["-xzf", `${name}.tgz`, "-C", rel, "--strip-components=1"], { cwd: out, stdio: "inherit" });
  rmSync(file);
  console.error(`ok  @tauri-apps/${name} ${p.version} (sha512 matches the lockfile)`);
}
console.log(join(out, "node_modules", "@tauri-apps", "cli", "tauri.js"));
