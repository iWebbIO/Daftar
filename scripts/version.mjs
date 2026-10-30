// One command to set the app version, because it lives in several files and a
// release is only correct when all of them agree.
//
//   node scripts/version.mjs 0.2.1
//
// package.json and package-lock.json are set by npm itself, because `npm ci` in
// CI refuses a lock whose root version disagrees with package.json — that is
// exactly how one release failed.
//
// Deliberately does NOT touch src-tauri/Cargo.toml: the crate version is not the
// app version, and changing it rewrites Cargo.lock, which throws away the CI
// build cache on every release. build.rs compiles tauri.conf.json's version in
// as DAFTAR_VERSION for the updater.

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const next = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(next ?? "")) {
  console.error("usage: node scripts/version.mjs <major.minor.patch>");
  process.exit(1);
}

// npm writes both package.json and package-lock.json, and checks they agree.
execFileSync("npm", ["version", next, "--no-git-tag-version", "--allow-same-version"], {
  stdio: "inherit",
  shell: true,
});

const read = (p) => readFileSync(p, "utf8");
const write = (p, s) => writeFileSync(p, s, "utf8");

// src-tauri/tauri.conf.json — the one Android and the updater both read
write(
  "src-tauri/tauri.conf.json",
  read("src-tauri/tauri.conf.json").replace(
    /"version": "\d+\.\d+\.\d+"/,
    `"version": "${next}"`,
  ),
);

// src/version.ts — what «بیشتر» shows
write(
  "src/version.ts",
  read("src/version.ts").replace(/(APP_VERSION = ")\d+\.\d+\.\d+(")/, `$1${next}$2`),
);

const checks = [
  ["package.json", /"version": "(\d+\.\d+\.\d+)"/],
  ["package-lock.json", /^\s{2}"version": "(\d+\.\d+\.\d+)"/m],
  ["src-tauri/tauri.conf.json", /"version": "(\d+\.\d+\.\d+)"/],
  ["src/version.ts", /APP_VERSION = "(\d+\.\d+\.\d+)"/],
].map(([file, re]) => `${file}: ${read(file).match(re)?.[1]}`);

console.log(checks.join("\n"));
if (checks.some((line) => !line.endsWith(next))) {
  console.error(`not every file says ${next}`);
  process.exit(1);
}
