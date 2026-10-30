/**
 * The version the interface shows in «بیشتر».
 *
 * Kept as a constant on purpose: reading it from the build config couples a
 * screen to the bundler, and this number is bumped in the same commit as the
 * tag (package.json, tauri.conf.json, Cargo.toml, and here).
 */
export const APP_VERSION = "0.1.6";
