// One command to set the app version, because it lives in three files and a
// release is only correct when all three agree.
//
//   node scripts/version.mjs 0.2.0
//
// Deliberately does NOT touch src-tauri/Cargo.toml: the crate version is not the
// app version, and changing it rewrites Cargo.lock, which throws away the CI
// build cache on every release. build.rs compiles tauri.conf.json's version in
// as DAFTAR_VERSION for the updater.

import { readFileSync, writeFileSync } from "node:fs";

const next = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(next ?? "")) {
  console.error("usage: node scripts/version.mjs <major.minor.patch>");
  process.exit(1);
}

const read = (p) => readFileSync(p, "utf8");
const write = (p, s) => writeFileSync(p, s, "utf8");

// package.json
write(
  "package.json",
  read("package.json").replace(/"version": "\d+\.\d+\.\d+"/, `"version": "${next}"`),
);

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

const check = [
  ["package.json", /"version": "(\d+\.\d+\.\d+)"/],
  ["src-tauri/tauri.conf.json", /"version": "(\d+\.\d+\.\d+)"/],
  ["src/version.ts", /APP_VERSION = "(\d+\.\d+\.\d+)"/],
].map(([file, re]) => `${file}: ${read(file).match(re)?.[1]}`);

console.log(check.join("\n"));
if (check.some((line) => !line.endsWith(next))) {
  console.error(`not every file says ${next}`);
  process.exit(1);
}
