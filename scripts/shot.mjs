// Screenshot harness.
//
// Drives the Chrome already on this machine over the DevTools protocol, so a
// screen can be looked at exactly as it will appear on the phone: a real
// viewport, real fonts, real layout. Nothing about the app changes for it.
//
//   node scripts/shot.mjs today
//   node scripts/shot.mjs parties order party:3 --size 390x844
//
// Each name is a hash route the app understands. Output goes to .shots/.

import puppeteer from "puppeteer-core";
import { existsSync, mkdirSync } from "node:fs";

const CHROME_CANDIDATES = [
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe",
  `${process.env.LOCALAPPDATA}/Google/Chrome/Application/chrome.exe`,
];

const args = process.argv.slice(2);
const sizeArg = args.find((a) => a.startsWith("--size="));
const [w, h] = (sizeArg ? sizeArg.split("=")[1] : "390x844").split("x").map(Number);
const names = args.filter((a) => !a.startsWith("--"));
const base = process.env.BASE_URL ?? "http://127.0.0.1:5183/";

mkdirSync(".shots", { recursive: true });

const browser = await puppeteer.launch({
  executablePath: CHROME_CANDIDATES.find((p) => existsSync(p)),
  headless: true,
  args: ["--no-sandbox", "--disable-gpu", "--font-render-hinting=none"],
  defaultViewport: { width: w, height: h, deviceScaleFactor: 2 },
});

const page = await browser.newPage();
const problems = [];
page.on("pageerror", (e) => problems.push(`pageerror: ${e.message}`));
page.on("console", (m) => {
  if (m.type() === "error") problems.push(`console: ${m.text()}`);
});

const shots = names.length ? names : ["today"];

let index = 0;
for (const name of shots) {
  const [route, param] = name.split(":");
  index += 1;
  // A fresh document per shot: a hash-only change would not remount the app.
  await page.goto(`${base}?shot=${index}#${route}${param ? `/${param}` : ""}`, {
    waitUntil: "domcontentloaded",
  });
  await page.evaluate(() => document.fonts.ready);
  await new Promise((r) => setTimeout(r, 700));

  const metrics = await page.evaluate(() => ({
    innerWidth: window.innerWidth,
    scrollWidth: document.documentElement.scrollWidth,
    overflowing: [...document.querySelectorAll("*")]
      .filter((el) => {
        const r = el.getBoundingClientRect();
        return r.width > 0 && (r.right > window.innerWidth + 0.5 || r.left < -0.5);
      })
      .slice(0, 6)
      .map((el) => `${el.tagName}.${String(el.className).slice(0, 30)}`),
  }));

  const file = `.shots/${name.replace(/[:/]/g, "-")}.png`;
  await page.screenshot({ path: file, fullPage: false });
  console.log(
    `${file}  ${w}x${h}  scrollWidth=${metrics.scrollWidth}` +
      (metrics.overflowing.length ? `  OVERFLOW: ${metrics.overflowing.join(", ")}` : ""),
  );
}

if (problems.length) {
  console.log("\nPROBLEMS:");
  for (const p of [...new Set(problems)].slice(0, 12)) console.log(" -", p);
}

await browser.close();
