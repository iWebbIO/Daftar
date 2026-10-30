// Prints the visible text of screens, so copy can be reviewed without a picture.
//
//   node scripts/text.mjs
//   node scripts/text.mjs --query=empty=1
//   node scripts/text.mjs today order payment:1

import puppeteer from "puppeteer-core";
import { existsSync } from "node:fs";

const CHROME = [
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe",
].find((p) => existsSync(p));

const args = process.argv.slice(2);
const query = args.find((a) => a.startsWith("--query="))?.slice("--query=".length) ?? "";
const names = args.filter((a) => !a.startsWith("--"));
const routes = names.length
  ? names
  : ["today", "parties", "party:1", "order", "payment:1", "invoice:1", "tasks", "cheques", "sums", "reports", "search", "products", "notes", "settings", "more"];

const browser = await puppeteer.launch({
  executablePath: CHROME,
  headless: true,
  args: ["--no-sandbox", "--disable-gpu"],
  defaultViewport: { width: 390, height: 844, deviceScaleFactor: 1 },
});
const page = await browser.newPage();

for (const [i, name] of routes.entries()) {
  const [route, param] = name.split(":");
  await page.goto(
    `http://127.0.0.1:5183/?t=${i}${query ? `&${query}` : ""}#${route}${param ? `/${param}` : ""}`,
    { waitUntil: "domcontentloaded" },
  );
  await page.evaluate(() => document.fonts.ready);
  await new Promise((r) => setTimeout(r, 500));
  const text = await page.evaluate(() => {
    const frame = document.querySelector(".app-frame");
    return (frame?.innerText ?? "").split("\n").map((l) => l.trim()).filter(Boolean);
  });
  console.log(`\n── ${name} ${query ? `(${query})` : ""}`);
  console.log(text.join(" · ").slice(0, 700));
}

await browser.close();
