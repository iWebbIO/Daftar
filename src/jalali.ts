// Jalali calendar, the same arithmetic the Rust side uses.
//
// A date in this app is a Julian Day Number: one integer that sorts and
// subtracts correctly. This module turns it into what a Persian reader expects.

const BREAKS = [
  -61, 9, 38, 199, 426, 686, 756, 818, 1111, 1181, 1210, 1635, 2060, 2097, 2192, 2262, 2324, 2394,
  2456, 3178,
];

const div = (a: number, b: number) => Math.trunc(a / b);
const mod = (a: number, b: number) => a % b;

export const MONTHS = [
  "فروردین",
  "اردیبهشت",
  "خرداد",
  "تیر",
  "مرداد",
  "شهریور",
  "مهر",
  "آبان",
  "آذر",
  "دی",
  "بهمن",
  "اسفند",
];

export const WEEKDAYS = ["شنبه", "یکشنبه", "دوشنبه", "سه‌شنبه", "چهارشنبه", "پنجشنبه", "جمعه"];

export interface JDate {
  year: number;
  month: number;
  day: number;
}

function jalCal(jy: number, withoutLeap: boolean): { leap: number; gy: number; march: number } {
  const bl = BREAKS.length;
  const gy = jy + 621;
  let leapJ = -14;
  let jp = BREAKS[0]!;
  let jump = 0;
  for (let i = 1; i < bl; i += 1) {
    const jm = BREAKS[i]!;
    jump = jm - jp;
    if (jy < jm) break;
    leapJ += div(jump, 33) * 8 + div(mod(jump, 33), 4);
    jp = jm;
  }
  let n = jy - jp;
  leapJ += div(n, 33) * 8 + div(mod(n, 33) + 3, 4);
  if (mod(jump, 33) === 4 && jump - n === 4) leapJ += 1;
  const leapG = div(gy, 4) - div((div(gy, 100) + 1) * 3, 4) - 150;
  const march = 20 + leapJ - leapG;

  let leap = 0;
  if (!withoutLeap) {
    if (jump - n < 6) n = n - jump + div(jump + 4, 33) * 33;
    leap = mod(mod(n + 1, 33) - 1, 4);
    if (leap === -1) leap = 4;
  }
  return { leap, gy, march };
}

export function g2d(gy: number, gm: number, gd: number): number {
  let d =
    div((gy + div(gm - 8, 6) + 100100) * 1461, 4) +
    div(153 * mod(gm + 9, 12) + 2, 5) +
    gd -
    34840408;
  d = d - div(div(gy + 100100 + div(gm - 8, 6), 100) * 3, 4) + 752;
  return d;
}

export function d2g(jdn: number): { gy: number; gm: number; gd: number } {
  let j = 4 * jdn + 139361631;
  j = j + div(div(4 * jdn + 183187720, 146097) * 3, 4) * 4 - 3908;
  const i = div(mod(j, 1461), 4) * 5 + 308;
  const gd = div(mod(i, 153), 5) + 1;
  const gm = mod(div(i, 153), 12) + 1;
  const gy = div(j, 1461) - 100100 + div(8 - gm, 6);
  return { gy, gm, gd };
}

export function j2d(jy: number, jm: number, jd: number): number {
  const { gy, march } = jalCal(jy, true);
  return g2d(gy, 3, march) + (jm - 1) * 31 - div(jm, 7) * (jm - 7) + jd - 1;
}

export function d2j(jdn: number): JDate {
  const { gy } = d2g(jdn);
  let jy = gy - 621;
  const { leap, march } = jalCal(jy, false);
  const jdn1f = g2d(gy, 3, march);
  let k = jdn - jdn1f;
  if (k >= 0) {
    if (k <= 185) {
      return { year: jy, month: 1 + div(k, 31), day: mod(k, 31) + 1 };
    }
    k -= 186;
  } else {
    jy -= 1;
    k += 179;
    if (leap === 1) k += 1;
  }
  return { year: jy, month: 7 + div(k, 30), day: mod(k, 30) + 1 };
}

export function isLeap(jy: number): boolean {
  return jalCal(jy, false).leap === 0;
}

export function monthLen(jy: number, jm: number): number {
  if (jm <= 6) return 31;
  if (jm <= 11) return 30;
  return isLeap(jy) ? 30 : 29;
}

/** 0 = شنبه … 6 = جمعه */
export function weekday(jdn: number): number {
  return (((jdn % 7) + 2) % 7) as number;
}

export function weekdayName(jdn: number): string {
  return WEEKDAYS[weekday(jdn)]!;
}

/** The shop's today, in the phone's own timezone. */
export function today(): number {
  const now = new Date();
  return g2d(now.getFullYear(), now.getMonth() + 1, now.getDate());
}

/** The day number of a Date. */
export function dayOf(date: Date): number {
  return g2d(date.getFullYear(), date.getMonth() + 1, date.getDate());
}

/** `۸ آبان` — the short form used inside lists. */
export function dayMonth(jdn: number): string {
  const j = d2j(jdn);
  return `${fa(j.day)} ${MONTHS[j.month - 1]}`;
}

/** `۸ آبان ۱۴۰۵` */
export function longDate(jdn: number): string {
  const j = d2j(jdn);
  return `${fa(j.day)} ${MONTHS[j.month - 1]} ${fa(j.year)}`;
}

/** `۱۴۰۵/۰۸/۰۸` */
export function shortDate(jdn: number): string {
  const j = d2j(jdn);
  return fa(`${String(j.year).padStart(4, "0")}/${String(j.month).padStart(2, "0")}/${String(
    j.day,
  ).padStart(2, "0")}`);
}

/** Relative words a shopkeeper would say out loud. */
export function relativeDay(jdn: number, base = today()): string {
  const diff = jdn - base;
  if (diff === 0) return "امروز";
  if (diff === -1) return "دیروز";
  if (diff === 1) return "فردا";
  if (diff === 2) return "پس‌فردا";
  if (diff < -1 && diff > -7) return `${fa(-diff)} روز پیش`;
  if (diff > 2 && diff < 8) return `${fa(diff)} روز دیگر`;
  return longDate(jdn);
}

function fa(n: number | string): string {
  return String(n).replace(/[0-9]/g, (d) => "۰۱۲۳۴۵۶۷۸۹"[Number(d)]!);
}
