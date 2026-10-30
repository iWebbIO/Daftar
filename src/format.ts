// Money and quantity as the shopkeeper reads them.
//
// Rust keeps integers; everything Persian happens here, in one place, so a
// number looks identical in a list, on a button and in a total.

export const THOUSANDS = "٬";
export const DECIMAL = "٫";

const FA_DIGITS = "۰۱۲۳۴۵۶۷۸۹";

/** Any digit, Persian, Arabic-Indic or Latin, becomes Persian. */
export function fa(input: string | number): string {
  return String(input)
    .replace(/[0-9]/g, (d) => FA_DIGITS[Number(d)]!)
    .replace(/[\u0660-\u0669]/g, (d) => FA_DIGITS[d.charCodeAt(0) - 0x0660]!);
}

/** Back to Latin, for anything that has to be parsed or compared. */
export function latin(input: string): string {
  return input
    .replace(/[۰-۹]/g, (d) => String(d.charCodeAt(0) - 0x06f0))
    .replace(/[٠-٩]/g, (d) => String(d.charCodeAt(0) - 0x0660));
}

/** `۳۴۵٬۰۰۰` */
export function fmtAmount(n: number): string {
  const negative = n < 0;
  const digits = Math.abs(Math.round(n)).toString();
  let out = "";
  for (let i = 0; i < digits.length; i += 1) {
    if (i > 0 && (digits.length - i) % 3 === 0) out += THOUSANDS;
    out += digits[i];
  }
  return (negative ? "−" : "") + fa(out);
}

/** `۳۴۵٬۰۰۰ تومان` */
export function fmtToman(n: number): string {
  return `${fmtAmount(n)} تومان`;
}

/** `۵٬۱۷۵` without the unit, for tables where the unit is in the header. */
export function fmtPlain(n: number): string {
  return fmtAmount(n);
}

/** Thousandths to a readable amount sold by weight: 2500 -> `۲٫۵`. */
export function fmtQty(milli: number): string {
  const whole = Math.trunc(milli / 1000);
  const frac = Math.abs(milli % 1000);
  if (frac === 0) return fa(String(whole));
  const s = `${whole}.${String(frac).padStart(3, "0")}`.replace(/0+$/, "");
  return fa(s).replace(".", DECIMAL);
}

/** Parses anything the phone keyboard produces into a number. */
export function parseAmount(input: string): number | null {
  let text = latin(input.trim()).replace(/[٬,\s_٫]/g, "");
  let scale = 1;
  const m = text.match(/^(.*?)(هزار|ه)$/);
  if (m && m[1]!.trim() !== "") {
    text = m[1]!.trim();
    scale = 1000;
  }
  if (!/^-?\d+$/.test(text)) return null;
  return Number(text) * scale;
}

/** Parses a quantity into thousandths. `۲٫۵` and `2.5` both mean 2500. */
export function parseQty(input: string): number | null {
  const text = latin(input.trim())
    .replace(/[٬,\s]/g, "")
    .replace(DECIMAL, ".");
  if (!/^\d*(\.\d*)?$/.test(text) || text === "" || text === ".") return null;
  return Math.round(Number(text) * 1000);
}

/** One short, readable quantity with its unit: `۱۵ کیلو`. */
export function fmtWithUnit(milli: number, unit: string): string {
  return `${fmtQty(milli)} ${unit}`;
}

export const UNITS = ["کیلو", "عدد", "کارتن", "بسته", "شیشه", "کیسه", "شانه"];

/**
 * The line total. Quantity is thousandths, price is per whole unit, so the
 * product is divided back down — the same arithmetic the Rust side does.
 */
export function lineTotal(qtyMilli: number, price: number): number {
  return Math.round((qtyMilli * price) / 1000);
}
