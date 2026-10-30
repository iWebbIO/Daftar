<div dir="rtl">

# دفتر

دفتر حساب و سفارش فروشگاه، برای کسی که کالا را با وزن می‌فروشد و حساب مشتری‌ها را در دفتر کاغذی نگه می‌دارد.

همان دفتر است، با یک فرق: جمع‌ها را خودش می‌زند، قیمت دفعه قبل را یادش می‌ماند، خط خوردن اسم کسی که پول داده را خودش انجام می‌دهد، و فاکتور را با یک دکمه به تلگرام می‌فرستد.

بدون اینترنت کار می‌کند. تمام حساب‌ها روی خود گوشی است.

---

## نصب

از صفحه [Releases](https://github.com/iWebbIO/Daftar/releases/latest) فایل `Daftar-x.y.z.apk` را روی گوشی اندروید نصب کن. اگر گوشی اجازه نداد، در تنظیمات «نصب از منابع ناشناس» را روشن کن.

هر نسخه‌ی تازه خودش ساخته و امضا می‌شود؛ همان فایل روی همان گوشی به‌روز می‌شود، بدون اینکه چیزی از دست برود.

---

## وصل کردن تلگرام

یک بار انجام می‌شود و تمام. بعد از آن هم فاکتور و هم پشتیبان به تلگرام خودت می‌رود.

1. در تلگرام به **@BotFather** پیام بده و با `/newbot` یک ربات بساز. یک توکن می‌دهد.
2. در برنامه: **بیشتر ← تنظیمات و پشتیبان**، توکن را در کادر «توکن ربات» بگذار.
3. دکمه «وصل کن» را بزن، بعد ربات تازه‌ساخته‌ات را در تلگرام باز کن و **Start** را بزن. برنامه خودش وصل می‌شود.

از این به بعد فاکتورها و نسخه پشتیبان در چت خودت با آن ربات می‌آید و می‌توانی برای مشتری بفرستی.

---

## کارهای روزمره

| کار | راه |
|---|---|
| فاکتور نوشتن | دکمه سبز پایین صفحه |
| پول گرفتن از مشتری | «پول گرفتم» در صفحه امروز |
| خرید از منبع | «خرید» در صفحه امروز |
| دیدن اینکه کی چقدر بدهکار است | «حساب‌ها» |
| جست‌وجو | ذره‌بین بالای صفحه امروز |

**قیمت را کم تایپ می‌کنی.** وقتی یک کالا را برای یک مشتری می‌زنی، قیمت آخرین باری که به او فروختی خودش پر می‌شود. اگر قیمت عوض شده، همان‌جا عوضش کن.

**خط خوردن جای پاک کردن را گرفته.** فاکتور یا پرداختی که اشتباه بود خط می‌خورد و خوانده می‌شود، ولی از حساب‌ها کنار می‌رود.

---

## پشتیبان

هر روز که برنامه را باز کنی، خودش یک نسخه پشتیبان درست می‌کند و به تلگرام می‌فرستد. اگر اینترنت نبود، فایل روی گوشی می‌ماند و روز بعد دوباره تلاش می‌کند.

دستی هم می‌شود: **تنظیمات ← همین حالا پشتیبان بگیر**.

---

## چند نکته

- مبلغ‌ها به **تومان** است.
- تاریخ‌ها **شمسی** است.
- عددها فارسی نشان داده می‌شوند، ولی می‌توانی با هر صفحه‌کلیدی تایپ کنی.
- هیچ داده‌ای جایی جز گوشی خودت و چت تلگرام خودت نمی‌رود.

</div>

---

## برای برنامه‌نویس

Tauri 2 · Rust · React 19 · SQLite. Android فقط.

```sh
npm install
npm run dev                 # the interface in a browser, with a mock ledger
npm run tauri:dev           # the desktop shell, same code
npm run android:build       # the APK
```

```
src/                 the interface (React, one component kit, RTL by default)
src-tauri/src/       the whole app: SQLite, Jalali, money, invoices, Telegram
  domain/            every operation the interface can ask for
  invoice/           Persian shaping → SVG → PDF
  telegram/          the bot
scripts/shot.mjs     screenshots of any screen, at phone size
```

**One entry point.** The interface sends `{ op, args }` to a single Tauri command
and gets JSON back (`src-tauri/src/lib.rs`, routed in `domain/mod.rs`). Every
screen therefore speaks the same language as the tests.

**Numbers have one home.** Money is integer toman; quantities are thousandths;
dates are Julian Day Numbers. Jalali is a presentation of a day number
(`src-tauri/src/jalali.rs`, mirrored in `src/jalali.ts` and checked against
published dates).

**Balances have one formula.** `src-tauri/src/domain/balance.rs` is the only
place a party's balance is computed, and every screen reads it from there.

**The invoice is drawn once.** Persian is shaped with HarfBuzz (via `rustybuzz`)
into glyph outlines, and the same outlines become both the SVG the app previews
and the vector paths in the PDF — so what he sees is what Telegram receives.

```sh
cd src-tauri && cargo test        # 116 tests: arithmetic, flows, and a 200-invoice stress pass
npm run build                     # type-check + bundle
```

Tests are the contract here. The arithmetic in `domain/orders.rs`, the balance
in `domain/balance.rs` and the flows in `tests/e2e_ledger.rs` are the reason the
numbers can be trusted.

License: AGPL-3.0-only.
