// A UX audit that does not need eyes.
//
// For every screen it measures the things a designer would point at: text that
// is clipped, a page that scrolls sideways, a big empty hole in the layout, a
// primary button that is not where a thumb is, and numbers that are not the
// biggest thing on the screen.
//
//   node scripts/audit.mjs
//   node scripts/audit.mjs order payment:1 --size=360x740
//
// Exit code 1 if any screen has a blocking problem (clipping or sideways scroll).

import puppeteer from "puppeteer-core";
import { existsSync } from "node:fs";

const CHROME = [
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe",
].find((p) => existsSync(p));

const args = process.argv.slice(2);
const sizeArg = args.find((a) => a.startsWith("--size="));
const [w, h] = (sizeArg ? sizeArg.split("=")[1] : "390x844").split("x").map(Number);
const query = args.find((a) => a.startsWith("--query="))?.slice("--query=".length) ?? "";
const names = args.filter((a) => !a.startsWith("--"));

const ROUTES = names.length
  ? names
  : [
      "today",
      "parties",
      "party:1",
      "order",
      "order/sale/1",
      "payment:1",
      "invoice:1",
      "tasks",
      "cheques",
      "sums",
      "sums/1",
      "reports",
      "search",
      "products",
      "notes",
      "settings",
      "more",
      "calc",
      // a record that does not exist must degrade in Persian, never blank
      "party:999",
      "invoice:999",
    ];

const base = process.env.BASE_URL ?? "http://127.0.0.1:5183/";

const browser = await puppeteer.launch({
  executablePath: CHROME,
  headless: true,
  args: ["--no-sandbox", "--disable-gpu"],
  defaultViewport: { width: w, height: h, deviceScaleFactor: 1 },
});

const page = await browser.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));

const report = [];
let blocking = 0;

for (const [i, name] of ROUTES.entries()) {
  const [route, param] = name.split(":");
  await page.goto(`${base}?a=${i}${query ? `&${query}` : ""}#${route}${param ? `/${param}` : ""}`, {
    waitUntil: "domcontentloaded",
  });
  await page.evaluate(() => document.fonts.ready);
  await new Promise((r) => setTimeout(r, 600));

  const m = await page.evaluate(() => {
    const vw = window.innerWidth;
    const vh = window.innerHeight;

    // Text that is cut off inside its own box.
    const clipped = [...document.querySelectorAll("*")]
      .filter((el) => {
        const cs = getComputedStyle(el);
        if (cs.overflow === "hidden" || cs.textOverflow === "ellipsis") return false;
        if (!el.textContent?.trim()) return false;
        return el.scrollWidth > el.clientWidth + 2 && el.clientWidth > 0;
      })
      .slice(0, 4)
      .map((el) => `${el.tagName}.${String(el.className).slice(0, 24)}`);

    // A hole in the middle of the page: the biggest vertical gap between two
    // stacked, visible children of the main column.
    const body = document.querySelector(".body") || document.querySelector(".screen");
    let gap = 0;
    let gapWhere = "";
    if (body) {
      const kids = [...body.children].filter((el) => {
        const r = el.getBoundingClientRect();
        return r.height > 4 && getComputedStyle(el).display !== "none";
      });
      for (let k = 1; k < kids.length; k += 1) {
        const a = kids[k - 1].getBoundingClientRect();
        const b = kids[k].getBoundingClientRect();
        const d = b.top - a.bottom;
        if (d > gap) {
          gap = Math.round(d);
          gapWhere = `${kids[k - 1].className || kids[k - 1].tagName} → ${kids[k].className || kids[k].tagName}`;
        }
      }
    }

    // The biggest type on the screen, and whether it is a number. Measured from
    // an element's *own* text so a number wrapped with a unit is not skipped.
    let biggest = { size: 0, text: "" };
    for (const el of document.querySelectorAll("*")) {
      const own = [...el.childNodes]
        .filter((n) => n.nodeType === 3)
        .map((n) => n.textContent)
        .join("")
        .trim();
      if (!own) continue;
      const size = parseFloat(getComputedStyle(el).fontSize);
      if (size > biggest.size) {
        biggest = { size: Math.round(size), text: own.slice(0, 18) };
      }
    }

    // Where the primary button sits vertically.
    const primary =
      document.querySelector(".btn--primary, .numpad__key--ok, .ord-total .btn") ??
      null;
    const pr = primary?.getBoundingClientRect();

    // Fingers, not cursors: anything tappable smaller than 44px is a target he
    // will miss in a shop, one-handed.
    const small = [...document.querySelectorAll("button, [role=button], a")]
      .filter((el) => {
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) return false;
        if (getComputedStyle(el).visibility === "hidden") return false;
        return r.height < 44 || r.width < 44;
      })
      .slice(0, 5)
      .map((el) => {
        const r = el.getBoundingClientRect();
        const label = el.textContent?.trim().slice(0, 14) || el.className || el.tagName;
        return `${label}(${Math.round(r.width)}x${Math.round(r.height)})`;
      });

    // Contrast of text against what is actually behind it.
    const srgb = (v) => {
      const c = v / 255;
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    };
    const lum = ([r, g, b]) => 0.2126 * srgb(r) + 0.7152 * srgb(g) + 0.0722 * srgb(b);
    const parse = (c) => (c.match(/[\d.]+/g) ?? []).map(Number);
    const behind = (el) => {
      let node = el;
      while (node) {
        const bg = parse(getComputedStyle(node).backgroundColor || "");
        if (bg.length >= 3 && (bg[3] === undefined || bg[3] > 0.5)) return bg;
        node = node.parentElement;
      }
      return [255, 255, 255];
    };
    const lowContrast = [];
    for (const el of document.querySelectorAll(".row__sub, .row__note, .tiny, .faint, .muted, .label")) {
      const own = [...el.childNodes]
        .filter((n) => n.nodeType === 3)
        .map((n) => n.textContent)
        .join("")
        .trim();
      if (!own) continue;
      const fg = parse(getComputedStyle(el).color);
      const bg = behind(el);
      const l1 = lum(fg.slice(0, 3));
      const l2 = lum(bg.slice(0, 3));
      const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
      if (ratio < 3.2) {
        lowContrast.push(`${own.slice(0, 14)} ${ratio.toFixed(1)}:1`);
      }
    }

    return {
      sideways: document.documentElement.scrollWidth - vw,
      clipped,
      gap,
      gapWhere,
      biggest,
      primaryTop: pr ? Math.round(pr.top / vh) : null,
      small,
      lowContrast: [...new Set(lowContrast)].slice(0, 4),
    };
  });

  const flags = [];
  if (m.sideways > 0) {
    flags.push(`SIDEWAYS +${m.sideways}`);
    blocking += 1;
  }
  if (m.clipped.length) {
    flags.push(`CLIPPED ${m.clipped.join(",")}`);
    blocking += 1;
  }
  if (m.gap > 180) flags.push(`GAP ${m.gap}px (${m.gapWhere})`);
  if (m.small.length) flags.push(`SMALL ${m.small.join(",")}`);
  if (m.lowContrast.length) flags.push(`FAINT ${m.lowContrast.join(",")}`);

  report.push(
    `${name.padEnd(14)} ${flags.length ? flags.join("  ") : "ok"}` +
      `   [biggest ${m.biggest.size}px «${m.biggest.text}»]`,
  );
}

await browser.close();

console.log(report.join("\n"));
if (errors.length) {
  console.log("\nPAGE ERRORS:");
  for (const e of [...new Set(errors)].slice(0, 6)) console.log(" -", e);
}
process.exit(blocking > 0 ? 1 : 0);
