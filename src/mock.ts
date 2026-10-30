// A pretend backend for the browser.
//
// The real app talks to Rust through one command. When the interface is opened
// in a plain browser (`npm run dev`) there is no Rust behind it, so this keeps a
// small ledger in memory with the same shapes — enough to design against and to
// take screenshots of, and never loaded inside the actual app.

import type {
  AgeBucket,
  BackupInfo,
  BotInfo,
  Cheque,
  ChequeRow,
  Doc,
  DocFull,
  DocInput,
  DocRow,
  Note,
  Party,
  PartyBook,
  PartyCard,
  Payment,
  PaymentInput,
  PaymentRow,
  PricePoint,
  Product,
  ProductReport,
  ReportDays,
  SearchHit,
  Settings,
  SumSheet,
  Task,
  TaskRow,
  TodayView,
  Totals,
} from "./types";
import { g2d, longDate, today } from "./jalali";
import { fmtAmount, fmtQty, lineTotal } from "./format";

interface Store {
  parties: Party[];
  products: Product[];
  docs: Doc[];
  lines: Record<number, DocFull["lines"]>;
  payments: Payment[];
  allocations: { paymentId: number; docId: number; amount: number }[];
  cheques: Cheque[];
  tasks: Task[];
  notes: Note[];
  sheets: SumSheet[];
  settings: Settings;
  seq: number;
}

const now = () => Date.now();

function seed(): Store {
  const day = today();
  const d = (n: number) => day - n;
  const party = (id: number, name: string, kind: Party["kind"], opening = 0): Party => ({
    id,
    name,
    phone: null,
    kind,
    address: null,
    note: null,
    opening,
    archived: false,
    createdAt: now(),
    updatedAt: now(),
  });
  const product = (id: number, name: string, unit: string, salePrice: number, buyPrice: number): Product => ({
    id,
    name,
    unit,
    salePrice,
    buyPrice,
    active: true,
    createdAt: now(),
    updatedAt: now(),
  });

  const parties = [
    party(1, "لیلا امینی", "customer"),
    party(2, "حسین رضایی", "customer"),
    party(3, "مادر زهرا", "customer"),
    party(4, "حاج‌آقا محمدی", "supplier"),
    party(5, "نانوایی سنگک", "customer"),
    party(6, "رضا کبابی", "customer"),
    party(7, "فروشگاه کوثر", "both"),
    party(8, "علی سبزی‌فروش", "customer"),
  ];

  const products = [
    product(1, "نخود", "کیلو", 285_000, 240_000),
    product(2, "لوبیا چیتی", "کیلو", 345_000, 300_000),
    product(3, "عدس", "کیلو", 320_000, 275_000),
    product(4, "زردچوبه", "کیلو", 720_000, 640_000),
    product(5, "خرمای زاهدی", "کیلو", 480_000, 410_000),
    product(6, "ترشی مخلوط", "شیشه", 250_000, 180_000),
    product(7, "برنج دم‌سیاه", "کیلو", 640_000, 550_000),
    product(8, "کشک", "کیلو", 380_000, 320_000),
  ];

  const docs: Doc[] = [];
  const lines: Record<number, DocFull["lines"]> = {};
  let docId = 0;

  const addDoc = (
    partyId: number,
    dayN: number,
    rows: { productId: number; qty: number; price: number }[],
    paid = 0,
    kind: Doc["kind"] = "sale",
  ) => {
    docId += 1;
    const id = docId;
    const built = rows.map((r, i) => {
      const p = products.find((x) => x.id === r.productId)!;
      return {
        id: id * 100 + i,
        docId: id,
        pos: i + 1,
        productId: p.id,
        name: p.name,
        qtyMilli: r.qty,
        unit: p.unit,
        price: r.price,
        total: lineTotal(r.qty, r.price),
        overrideTotal: null,
        note: null,
        voided: false,
      };
    });
    const total = built.reduce((s, l) => s + l.total, 0);
    lines[id] = built;
    docs.push({
      id,
      kind,
      jyear: 1404,
      number: id,
      partyId,
      day: dayN,
      status: paid >= total ? "settled" : "open",
      discount: 0,
      rounding: 0,
      note: null,
      total,
      paid,
      remaining: total - paid,
      final: false,
      voidReason: null,
      createdAt: now(),
      updatedAt: now(),
    });
    return id;
  };

  addDoc(1, d(0), [
    { productId: 1, qty: 15_000, price: 285_000 },
    { productId: 2, qty: 20_000, price: 345_000 },
    { productId: 4, qty: 35_000, price: 720_000 },
  ]);
  addDoc(2, d(0), [
    { productId: 3, qty: 10_000, price: 320_000 },
    { productId: 8, qty: 2_500, price: 380_000 },
  ]);
  addDoc(3, d(0), [{ productId: 6, qty: 4_000, price: 250_000 }], 1_000_000);
  addDoc(5, d(1), [
    { productId: 5, qty: 12_000, price: 480_000 },
    { productId: 7, qty: 5_000, price: 640_000 },
  ]);
  addDoc(6, d(2), [{ productId: 2, qty: 8_000, price: 345_000 }]);
  addDoc(8, d(9), [{ productId: 1, qty: 30_000, price: 285_000 }]);
  addDoc(4, d(0), [{ productId: 2, qty: 100_000, price: 300_000 }], 0, "purchase");

  const payments: Payment[] = [];
  const allocations: Store["allocations"] = [];
  let payId = 0;
  const paidOnThird = docs.find((x) => x.partyId === 3 && x.paid > 0);
  if (paidOnThird) {
    payId += 1;
    payments.push({
      id: payId,
      partyId: 3,
      direction: "in",
      amount: paidOnThird.paid,
      day: day,
      method: "cash",
      ref: null,
      note: null,
      voided: false,
      voidReason: null,
      createdAt: now(),
    });
    allocations.push({ paymentId: payId, docId: paidOnThird.id, amount: paidOnThird.paid });
  }

  return {
    parties,
    products,
    docs,
    lines,
    payments,
    allocations,
    cheques: [
      {
        id: 1,
        partyId: 2,
        direction: "in",
        number: "۴۵۸۲۱",
        bank: "ملت",
        amount: 5_000_000,
        dueDay: day + 2,
        status: "pending",
        note: null,
        paymentId: null,
        createdAt: now(),
      },
      {
        id: 3,
        partyId: 6,
        direction: "in",
        number: "۹۰۸۷۷",
        bank: "ملی",
        amount: 3_500_000,
        dueDay: day - 4, // passed and still pending: the one he must not miss
        status: "pending",
        note: null,
        paymentId: null,
        createdAt: now(),
      },
      {
        id: 2,
        partyId: 4,
        direction: "out",
        number: "۱۱۲۰۹",
        bank: "صادرات",
        amount: 12_000_000,
        dueDay: day + 6,
        status: "pending",
        note: null,
        paymentId: null,
        createdAt: now(),
      },
    ],
    tasks: [
      { id: 1, text: "به حسین بار بده", day: day, partyId: 2, done: false, createdAt: now() },
      { id: 2, text: "از لیلا پول بگیر", day: day, partyId: 1, done: false, createdAt: now() },
      { id: 3, text: "زردچوبه سفارش بده", day: null, partyId: null, done: false, createdAt: now() },
    ],
    notes: [
      {
        id: 1,
        day,
        title: "امروز",
        body: "قیمت زردچوبه بالا رفت. به حاج‌آقا گفتم هفته بعد حساب می‌کنیم.",
        createdAt: now(),
        updatedAt: now(),
      },
    ],
    sheets: [
      {
        id: 1,
        day,
        title: "جمع هفته",
        createdAt: now(),
        rows: [
          { id: 1, sheetId: 1, pos: 1, label: "شنبه", amount: 3_450_000, partyId: null, struck: false },
          { id: 2, sheetId: 1, pos: 2, label: "یکشنبه", amount: 5_120_000, partyId: null, struck: false },
          { id: 3, sheetId: 1, pos: 3, label: "دوشنبه", amount: 2_800_000, partyId: null, struck: true },
        ],
        total: 8_570_000,
      },
    ],
    settings: {
      shopName: "خشکبار امینی",
      shopPhone: "۰۹۱۲۳۴۵۶۷۸۹",
      shopAddress: "تهران، بازار آهنگرها",
      bankName: "بانک ملت",
      cardHolder: "خشکبار امینی",
      cardNumber: "۶۰۳۷-۹۹۷۴-۱۲۳۴-۵۶۷۸",
      invoiceNote: "ممنون از خرید شما",
      roundStep: 0,
      botToken: "",
      chatId: "",
      // So the preview can be looked at in either unit: ?currency=rial, ?theme=dark
      currency:
        new URLSearchParams(globalThis.location?.search ?? "").get("currency") === "rial"
          ? "rial"
          : "toman",
      theme:
        new URLSearchParams(globalThis.location?.search ?? "").get("theme") === "dark"
          ? "dark"
          : "light",
      backupHour: 21,
      lastBackupAt: now() - 1000 * 60 * 60 * 20,
    },
    seq: 1000,
  };
}

let store: Store | null = null;
function db(): Store {
  if (!store) {
    store = seed();
    // ?empty=1 is the moment he actually meets the app: a brand-new ledger.
    if (new URLSearchParams(globalThis.location?.search ?? "").get("empty") === "1") {
      store.parties = [];
      store.products = [];
      store.docs = [];
      store.lines = {};
      store.payments = [];
      store.allocations = [];
      store.cheques = [];
      store.tasks = [];
      store.notes = [];
      store.sheets = [];
      store.settings.lastBackupAt = null;
    }
  }
  return store;
}

const nextId = () => (db().seq += 1);

/** A plain number as Persian digits, for invoice numbers and the like. */
const digits = (n: number | string) =>
  String(n).replace(/[0-9]/g, (d) => "۰۱۲۳۴۵۶۷۸۹"[Number(d)]!);

function balanceOf(partyId: number): number {
  const s = db();
  const p = s.parties.find((x) => x.id === partyId);
  if (!p) return 0;
  let b = p.opening;
  for (const d of s.docs) {
    if (d.partyId !== partyId || d.status === "void") continue;
    const rem = d.total - d.paid;
    b += d.kind === "sale" ? rem : -rem;
  }
  for (const pay of s.payments) {
    if (pay.partyId !== partyId || pay.voided) continue;
    const used = s.allocations
      .filter((a) => a.paymentId === pay.id)
      .reduce((n, a) => n + a.amount, 0);
    const loose = pay.amount - used;
    b += pay.direction === "in" ? -loose : loose;
  }
  return b;
}

function card(p: Party): PartyCard {
  const s = db();
  const docs = s.docs.filter((d) => d.partyId === p.id && d.status !== "void");
  return {
    party: p,
    balance: balanceOf(p.id),
    lastDay: docs.length ? Math.max(...docs.map((d) => d.day)) : null,
    openDocs: docs.filter((d) => d.remaining > 0).length,
  };
}

function docRows(filter: (d: Doc) => boolean): DocRow[] {
  const s = db();
  return s.docs
    .filter(filter)
    .sort((a, b) => b.day - a.day)
    .map((doc) => ({
      doc,
      partyName: s.parties.find((p) => p.id === doc.partyId)?.name ?? "",
    }));
}

const or = (v: unknown, fallback: unknown) => (v === undefined || v === null ? fallback : v);

export async function mockCall(op: string, args: Record<string, unknown>): Promise<unknown> {
  const s = db();
  const a = args ?? {};

  switch (op) {
    case "home.today": {
      const day = Number(or(a.day, today()));
      const docs = docRows((d) => d.day === day);
      const pays = s.payments.filter((p) => p.day === day && !p.voided);
      const totals: Totals = { owesMe: 0, iOwe: 0 };
      for (const p of s.parties) {
        const b = balanceOf(p.id);
        if (b > 0) totals.owesMe += b;
        else totals.iOwe -= b;
      }
      return {
        day,
        jalali: "",
        jalaliLong: "",
        weekday: "",
        sold: docs.filter((r) => r.doc.kind === "sale").reduce((n, r) => n + r.doc.total, 0),
        bought: docs.filter((r) => r.doc.kind === "purchase").reduce((n, r) => n + r.doc.total, 0),
        received: pays.filter((p) => p.direction === "in").reduce((n, p) => n + p.amount, 0),
        paid: pays.filter((p) => p.direction === "out").reduce((n, p) => n + p.amount, 0),
        docs,
        tasks: s.tasks
          .filter((t) => !t.done)
          .map((task) => ({
            task,
            partyName: task.partyId ? s.parties.find((p) => p.id === task.partyId)?.name ?? null : null,
          })),
        cheques: s.cheques
          .filter((c) => c.status === "pending")
          .map((cheque) => ({
            cheque,
            partyName: s.parties.find((p) => p.id === cheque.partyId)?.name ?? "",
          })),
        totals,
        lastBackupAt: s.settings.lastBackupAt,
      } satisfies TodayView;
    }

    case "settings.get":
      return s.settings;
    case "settings.save":
      s.settings = (a.settings as Settings) ?? s.settings;
      return true;

    case "parties.list": {
      const q = String(or(a.q, "")).trim();
      const filter = String(or(a.filter, "all"));
      let rows = s.parties.map(card);
      if (q) rows = rows.filter((r) => r.party.name.includes(q));
      if (filter === "owes") rows = rows.filter((r) => r.balance > 0);
      if (filter === "iowe") rows = rows.filter((r) => r.balance < 0);
      if (filter === "settled") rows = rows.filter((r) => r.balance === 0);
      rows.sort((x, y) => Math.abs(y.balance) - Math.abs(x.balance));
      return rows;
    }
    case "parties.get":
      return s.parties.find((p) => p.id === Number(a.id)) ?? s.parties[0];
    case "parties.save": {
      const incoming = a.party as Partial<Party> & { name: string };
      if (incoming.id) {
        const p = s.parties.find((x) => x.id === incoming.id)!;
        Object.assign(p, incoming, { updatedAt: now() });
        return p;
      }
      const created: Party = {
        id: nextId(),
        name: incoming.name,
        phone: incoming.phone ?? null,
        kind: incoming.kind ?? "customer",
        address: incoming.address ?? null,
        note: incoming.note ?? null,
        opening: incoming.opening ?? 0,
        archived: false,
        createdAt: now(),
        updatedAt: now(),
      };
      s.parties.push(created);
      return created;
    }
    case "parties.book": {
      const id = Number(a.id);
      const p = s.parties.find((x) => x.id === id)!;
      const docs = s.docs.filter((d) => d.partyId === id && d.status !== "void");
      const payments = s.payments.filter((x) => x.partyId === id && !x.voided);
      const items = [
        ...docs.map((d) => ({
          kind: d.kind,
          id: d.id,
          day: d.day,
          title: d.kind === "sale" ? `فاکتور ${d.number}` : `خرید ${d.number}`,
          amount: d.total,
          remaining: d.remaining,
          status: d.status,
        })),
        ...payments.map((x) => ({
          kind: "payment",
          id: x.id,
          day: x.day,
          title: x.direction === "in" ? "پول گرفتم" : "پول دادم",
          amount: x.amount,
          remaining: 0,
          status: "",
        })),
      ].sort((x, y) => y.day - x.day);
      return {
        party: p,
        balance: balanceOf(id),
        items,
        docs,
        payments,
      } satisfies PartyBook;
    }
    case "parties.suggest": {
      const q = String(or(a.q, "")).trim();
      return s.parties
        .filter((p) => !q || p.name.includes(q))
        .map(card)
        .sort((x, y) => (y.lastDay ?? 0) - (x.lastDay ?? 0))
        .slice(0, 6);
    }
    case "parties.totals": {
      const totals: Totals = { owesMe: 0, iOwe: 0 };
      for (const p of s.parties) {
        const b = balanceOf(p.id);
        if (b > 0) totals.owesMe += b;
        else totals.iOwe -= b;
      }
      return totals;
    }

    case "products.list": {
      const q = String(or(a.q, "")).trim();
      return s.products.filter((p) => !q || p.name.includes(q));
    }
    case "products.save": {
      const incoming = a.product as Partial<Product> & { name: string };
      if (incoming.id) {
        const p = s.products.find((x) => x.id === incoming.id)!;
        Object.assign(p, incoming, { updatedAt: now() });
        return p;
      }
      const created: Product = {
        id: nextId(),
        name: incoming.name,
        unit: incoming.unit ?? "کیلو",
        salePrice: incoming.salePrice ?? 0,
        buyPrice: incoming.buyPrice ?? 0,
        active: true,
        createdAt: now(),
        updatedAt: now(),
      };
      s.products.push(created);
      return created;
    }
    case "products.history": {
      const id = Number(a.productId);
      const p = s.products.find((x) => x.id === id);
      return [
        { price: p?.salePrice ?? 0, day: today() },
        { price: Math.round((p?.salePrice ?? 0) * 0.96), day: today() - 12 },
        { price: Math.round((p?.salePrice ?? 0) * 0.9), day: today() - 40 },
      ] satisfies PricePoint[];
    }
    case "products.usual":
      return s.products.slice(0, 6);

    case "orders.get": {
      const id = Number(a.id);
      const doc = s.docs.find((d) => d.id === id)!;
      return {
        doc,
        party: s.parties.find((p) => p.id === doc.partyId)!,
        lines: s.lines[id] ?? [],
      } satisfies DocFull;
    }
    case "orders.save": {
      const input = a.input as DocInput;
      const id = input.id ?? nextId();
      const built = input.lines.map((l, i) => ({
        id: id * 100 + i,
        docId: id,
        pos: i + 1,
        productId: l.productId ?? null,
        name: l.name,
        qtyMilli: l.qtyMilli,
        unit: l.unit,
        price: l.price,
        total: l.overrideTotal ?? lineTotal(l.qtyMilli, l.price),
        overrideTotal: l.overrideTotal ?? null,
        note: l.note ?? null,
        voided: l.voided ?? false,
      }));
      const total = built.filter((l) => !l.voided).reduce((n, l) => n + l.total, 0) - input.discount;
      s.lines[id] = built;
      const existing = s.docs.find((d) => d.id === id);
      if (existing) {
        Object.assign(existing, { total, remaining: total - existing.paid, updatedAt: now() });
        return existing;
      }
      const doc: Doc = {
        id,
        kind: input.kind,
        jyear: 1404,
        number: s.docs.length + 1,
        partyId: input.partyId,
        day: input.day,
        status: input.receivedNow >= total ? "settled" : "open",
        discount: input.discount,
        rounding: 0,
        note: input.note ?? null,
        total,
        paid: input.receivedNow,
        remaining: total - input.receivedNow,
        final: input.final,
        voidReason: null,
        createdAt: now(),
        updatedAt: now(),
      };
      s.docs.push(doc);
      if (input.receivedNow > 0) {
        const pid = nextId();
        s.payments.push({
          id: pid,
          partyId: input.partyId,
          direction: input.kind === "sale" ? "in" : "out",
          amount: input.receivedNow,
          day: input.day,
          method: input.receivedMethod,
          ref: null,
          note: null,
          voided: false,
          voidReason: null,
          createdAt: now(),
        });
        s.allocations.push({ paymentId: pid, docId: id, amount: input.receivedNow });
      }
      return doc;
    }
    case "orders.void": {
      const doc = s.docs.find((d) => d.id === Number(a.id))!;
      doc.status = "void";
      doc.voidReason = String(a.reason);
      return doc;
    }
    case "orders.setFinal": {
      const doc = s.docs.find((d) => d.id === Number(a.id))!;
      doc.final = Boolean(a.final);
      return doc;
    }
    case "orders.repeat": {
      const doc = s.docs.find((d) => d.id === Number(a.id))!;
      return {
        kind: doc.kind,
        partyId: doc.partyId,
        day: today(),
        discount: 0,
        roundStep: 0,
        final: false,
        receivedNow: 0,
        receivedMethod: "cash",
        updatePrices: false,
        lines: (s.lines[doc.id] ?? []).map((l) => ({
          productId: l.productId,
          name: l.name,
          qtyMilli: l.qtyMilli,
          unit: l.unit,
          price: l.price,
          overrideTotal: null,
          voided: false,
        })),
      } satisfies DocInput;
    }
    case "orders.list": {
      const kind = a.kind ? String(a.kind) : null;
      const partyId = a.partyId ? Number(a.partyId) : null;
      return docRows(
        (d) => (!kind || d.kind === kind) && (!partyId || d.partyId === partyId) && d.status !== "void",
      );
    }

    case "payments.save": {
      const input = a.input as PaymentInput;
      const id = input.id ?? nextId();
      const payment: Payment = {
        id,
        partyId: input.partyId,
        direction: input.direction,
        amount: input.amount,
        day: input.day,
        method: input.method,
        ref: input.ref ?? null,
        note: input.note ?? null,
        voided: false,
        voidReason: null,
        createdAt: now(),
      };
      s.payments.push(payment);
      for (const alloc of input.allocations ?? []) {
        s.allocations.push({ paymentId: id, docId: alloc.docId, amount: alloc.amount });
        const doc = s.docs.find((d) => d.id === alloc.docId);
        if (doc) {
          doc.paid += alloc.amount;
          doc.remaining = doc.total - doc.paid;
          if (doc.paid >= doc.total) doc.status = "settled";
        }
      }
      return payment;
    }
    case "payments.void": {
      const p = s.payments.find((x) => x.id === Number(a.id))!;
      p.voided = true;
      p.voidReason = String(a.reason);
      for (const alloc of s.allocations.filter((x) => x.paymentId === p.id)) {
        const doc = s.docs.find((d) => d.id === alloc.docId);
        if (doc) {
          doc.paid -= alloc.amount;
          doc.remaining = doc.total - doc.paid;
          doc.status = doc.paid <= 0 ? "open" : "open";
        }
      }
      return p;
    }
    case "payments.list":
      return s.payments
        .filter((p) => !a.partyId || p.partyId === Number(a.partyId))
        .sort((x, y) => y.day - x.day)
        .map((payment) => {
          const allocated = s.allocations
            .filter((x) => x.paymentId === payment.id)
            .reduce((n, x) => n + x.amount, 0);
          return {
            payment,
            partyName: s.parties.find((p) => p.id === payment.partyId)?.name ?? "",
            allocated,
            unallocated: payment.amount - allocated,
          };
        }) satisfies PaymentRow[];
    case "payments.suggest": {
      const partyId = Number(a.partyId);
      let left = Number(a.amount);
      const out: { docId: number; amount: number }[] = [];
      const open = s.docs
        .filter((d) => d.partyId === partyId && d.remaining > 0 && d.status !== "void")
        .sort((x, y) => x.day - y.day);
      for (const doc of open) {
        if (left <= 0) break;
        const take = Math.min(left, doc.remaining);
        out.push({ docId: doc.id, amount: take });
        left -= take;
      }
      return out;
    }

    case "cheques.list":
      return s.cheques.map((cheque) => ({
        cheque,
        partyName: s.parties.find((p) => p.id === cheque.partyId)?.name ?? "",
      })) satisfies ChequeRow[];
    case "cheques.save": {
      const incoming = a.cheque as Partial<Cheque>;
      const created: Cheque = {
        id: nextId(),
        partyId: Number(incoming.partyId),
        direction: incoming.direction ?? "in",
        number: incoming.number ?? null,
        bank: incoming.bank ?? null,
        amount: Number(incoming.amount),
        dueDay: Number(incoming.dueDay),
        status: "pending",
        note: incoming.note ?? null,
        paymentId: null,
        createdAt: now(),
      };
      s.cheques.push(created);
      return created;
    }
    case "cheques.status": {
      const c = s.cheques.find((x) => x.id === Number(a.id))!;
      c.status = String(a.status) as Cheque["status"];
      return c;
    }

    case "tasks.list":
      return s.tasks
        .filter((t) => a.includeDone || !t.done)
        .map((task) => ({
          task,
          partyName: task.partyId ? s.parties.find((p) => p.id === task.partyId)?.name ?? null : null,
        })) satisfies TaskRow[];
    case "tasks.save": {
      const incoming = a.task as Partial<Task> & { text: string };
      if (incoming.id) {
        const t = s.tasks.find((x) => x.id === incoming.id)!;
        Object.assign(t, incoming);
        return t;
      }
      const created: Task = {
        id: nextId(),
        text: incoming.text,
        day: incoming.day ?? null,
        partyId: incoming.partyId ?? null,
        done: false,
        createdAt: now(),
      };
      s.tasks.push(created);
      return created;
    }
    case "tasks.toggle": {
      const t = s.tasks.find((x) => x.id === Number(a.id))!;
      t.done = !t.done;
      return t;
    }

    case "notes.list":
      return s.notes.sort((x, y) => y.day - x.day);
    case "notes.save": {
      const incoming = a.note as Partial<Note> & { body: string };
      if (incoming.id) {
        const n = s.notes.find((x) => x.id === incoming.id)!;
        Object.assign(n, incoming, { updatedAt: now() });
        return n;
      }
      const created: Note = {
        id: nextId(),
        day: incoming.day ?? today(),
        title: incoming.title ?? null,
        body: incoming.body,
        createdAt: now(),
        updatedAt: now(),
      };
      s.notes.push(created);
      return created;
    }

    case "sums.list":
      return s.sheets;
    case "sums.get":
      return s.sheets.find((x) => x.id === Number(a.id)) ?? s.sheets[0];
    case "sums.save": {
      const incoming = a.sheet as Partial<SumSheet>;
      const total = (incoming.rows ?? [])
        .filter((r) => !r.struck)
        .reduce((n, r) => n + r.amount, 0);
      const sheet: SumSheet = {
        id: nextId(),
        day: incoming.day ?? today(),
        title: incoming.title ?? null,
        createdAt: now(),
        rows: incoming.rows ?? [],
        total,
      };
      s.sheets.push(sheet);
      return sheet;
    }

    case "reports.days": {
      const from = Number(a.from);
      const to = Number(a.to);
      const out: ReportDays[] = [];
      for (let d = from; d <= to; d += 1) {
        const docs = s.docs.filter((x) => x.day === d && x.status !== "void");
        const pays = s.payments.filter((x) => x.day === d && !x.voided);
        if (docs.length === 0 && pays.length === 0) continue;
        out.push({
          day: d,
          sold: docs.filter((x) => x.kind === "sale").reduce((n, x) => n + x.total, 0),
          bought: docs.filter((x) => x.kind === "purchase").reduce((n, x) => n + x.total, 0),
          received: pays.filter((x) => x.direction === "in").reduce((n, x) => n + x.amount, 0),
          paid: pays.filter((x) => x.direction === "out").reduce((n, x) => n + x.amount, 0),
          count: docs.length,
        });
      }
      return out;
    }
    case "reports.products": {
      const acc = new Map<string, ProductReport>();
      for (const doc of s.docs) {
        if (doc.kind !== "sale" || doc.status === "void") continue;
        for (const line of s.lines[doc.id] ?? []) {
          if (line.voided) continue;
          const key = line.name;
          const row =
            acc.get(key) ??
            ({
              productId: line.productId,
              name: line.name,
              qtyMilli: 0,
              unit: line.unit,
              revenue: 0,
              lastPrice: line.price,
            } satisfies ProductReport);
          row.qtyMilli += line.qtyMilli;
          row.revenue += line.total;
          acc.set(key, row);
        }
      }
      return [...acc.values()].sort((x, y) => y.revenue - x.revenue);
    }
    case "reports.ageing": {
      const buckets: AgeBucket[] = [
        { label: "تا ۷ روز", amount: 0, count: 0 },
        { label: "۸ تا ۳۰ روز", amount: 0, count: 0 },
        { label: "بیش از ۳۰ روز", amount: 0, count: 0 },
      ];
      const base = today();
      for (const doc of s.docs) {
        if (doc.kind !== "sale" || doc.status === "void" || doc.remaining <= 0) continue;
        const age = base - doc.day;
        const i = age <= 7 ? 0 : age <= 30 ? 1 : 2;
        buckets[i]!.amount += doc.remaining;
        buckets[i]!.count += 1;
      }
      return buckets;
    }

    case "search.all": {
      const q = String(or(a.q, "")).trim();
      if (q.length < 2) return [];
      const hits: SearchHit[] = [];
      for (const p of s.parties) {
        if (p.name.includes(q)) {
          hits.push({ kind: "party", id: p.id, title: p.name, subtitle: "طرف حساب", amount: balanceOf(p.id) });
        }
      }
      for (const p of s.products) {
        if (p.name.includes(q)) {
          hits.push({
            kind: "product",
            id: p.id,
            title: p.name,
            subtitle: `${p.unit} · آخرین قیمت`,
            amount: p.salePrice,
          });
        }
      }
      return hits.slice(0, 30);
    }

    case "invoice.svg":
      return { svg: mockInvoiceSvg(Number(a.docId)) };
    case "invoice.preview":
      return { path: "" };
    case "invoice.send":
      return { ok: false, message: "در حالت پیش‌نمایش تلگرام وصل نیست" };
    case "invoice.sendStatement":
      return { ok: false, message: "در حالت پیش‌نمایش تلگرام وصل نیست" };

    case "telegram.check":
      return { username: "daftar_bot", name: "دفتر", chatId: "123456", chatTitle: "من" } satisfies BotInfo;
    case "telegram.link":
      return { username: "daftar_bot", name: "دفتر", chatId: "123456", chatTitle: "من" } satisfies BotInfo;
    case "backup.now":
      return {
        at: now(),
        size: 48_213,
        sent: true,
        message: "نسخه پشتیبان به تلگرام رفت",
      } satisfies BackupInfo;
    case "update.check":
      return {
        current: "0.1.1",
        latest: "0.1.2",
        behind: true,
        apkUrl: "https://example.test/Daftar-0.1.2.apk",
        notes: "نسخه جدید در دسترس است",
      };

    case "update.download":
      throw "در این نسخه نصب نمی‌شود";

    case "backup.status":
      return {
        at: s.settings.lastBackupAt ?? 0,
        size: 48_213,
        sent: true,
        message: "آخرین نسخه پشتیبان",
      } satisfies BackupInfo;

    case "share.invoiceText": {
      const doc = s.docs.find((d) => d.id === Number(a.docId)) ?? s.docs[0]!;
      const party = s.parties.find((p) => p.id === doc.partyId);
      const lines = (s.lines[doc.id] ?? []).filter((l) => !l.voided);
      const out: string[] = [];
      out.push(`سلام ${party?.name ?? ""} عزیز،`.replace("  ", " "));
      out.push(
        `${doc.kind === "purchase" ? "خرید" : "فاکتور"} شماره ${digits(doc.number)} · ${longDate(
          doc.day,
        )}`,
      );
      if (lines.length) {
        out.push("");
        for (const l of lines) {
          out.push(
            `• ${l.name} — ${fmtQty(l.qtyMilli)} × ${fmtAmount(l.price)} = ${fmtAmount(l.total)}`,
          );
        }
      }
      out.push("");
      out.push(`جمع کل: ${fmtAmount(doc.total)} تومان`);
      if (doc.paid > 0) out.push(`پرداخت شده: ${fmtAmount(doc.paid)} تومان`);
      if (doc.remaining > 0) out.push(`مانده: ${fmtAmount(doc.remaining)} تومان`);
      else if (doc.total > 0) out.push("تسویه شده است.");
      if (s.settings.cardNumber) {
        out.push("");
        if (s.settings.cardHolder) out.push(`به نام ${s.settings.cardHolder}`);
        if (s.settings.bankName) out.push(s.settings.bankName);
        out.push(`کارت: ${s.settings.cardNumber}`);
      }
      if (s.settings.invoiceNote) {
        out.push("");
        out.push(s.settings.invoiceNote);
      }
      return { text: out.join("\n"), title: `فاکتور ${digits(doc.number)}` };
    }

    case "share.statementText": {
      const party = s.parties.find((p) => p.id === Number(a.partyId)) ?? s.parties[0]!;
      const bal = balanceOf(party.id);
      const open = s.docs
        .filter((d) => d.partyId === party.id && d.status !== "void" && d.remaining > 0)
        .sort((x, y) => x.day - y.day);
      const out: string[] = [];
      out.push(`سلام ${party.name} عزیز،`);
      out.push("صورت‌حساب شما:");
      out.push("");
      if (!open.length) {
        out.push("حسابی باز نیست.");
      } else {
        for (const d of open.slice(0, 12)) {
          out.push(
            `• ${d.kind === "purchase" ? "خرید" : "فاکتور"} ${digits(d.number)} · ${longDate(
              d.day,
            )} · مانده ${fmtAmount(d.remaining)}`,
          );
        }
        out.push("");
      }
      if (bal > 0) {
        out.push(`مانده حساب: ${fmtAmount(bal)} تومان`);
        out.push("طلب ما از شماست.");
      } else if (bal < 0) {
        out.push(`مانده حساب: ${fmtAmount(-bal)} تومان`);
        out.push("طلب شما از ماست.");
      } else {
        out.push("حساب تسویه است. ممنون.");
      }
      if (s.settings.invoiceNote) {
        out.push("");
        out.push(s.settings.invoiceNote);
      }
      return { text: out.join("\n"), title: "صورت‌حساب" };
    }

    default:
      throw `دستور ناشناخته: ${op}`;
  }
}

/** Only used by the mock so the browser preview shows today's date. */
export const mockToday = () => g2d(new Date().getFullYear(), new Date().getMonth() + 1, new Date().getDate());

/**
 * A stand-in for the page the Rust renderer produces, so the invoice screen can
 * be looked at in a browser. Same shape as a real فاکتور: shop at the top, the
 * customer, the table, and the sums on the left.
 */
function mockInvoiceSvg(docId: number): string {
  const s = db();
  const doc = s.docs.find((d) => d.id === docId) ?? s.docs[0]!;
  const party = s.parties.find((p) => p.id === doc.partyId);
  const lines = s.lines[doc.id] ?? [];
  const W = 420;
  const H = 595;
  const ink = "#1b1c14";
  const muted = "#6d6b60";
  const line = "#ddd8ca";

  const money = (n: number) =>
    Math.round(n)
      .toString()
      .replace(/[0-9]/g, (d) => "۰۱۲۳۴۵۶۷۸۹"[Number(d)]!)
      .replace(/\B(?=(\d{3})+(?!\d))/g, "٬");

  // Column edges, right to left. Every cell is anchored at its right edge, the
  // way an RTL table reads, and each column is wide enough for the longest
  // number it will hold (nine digits at 11px is about 62pt).
  const COL = { pos: 396, name: 372, qty: 250, price: 156, total: 60 };

  const rows = lines
    .slice(0, 14)
    .map((l, i) => {
      const y = 268 + i * 26;
      return `<text x="${COL.pos}" y="${y}" font-size="11" fill="${muted}" text-anchor="end">${money(i + 1)}</text>
      <text x="${COL.name}" y="${y}" font-size="11" fill="${ink}" text-anchor="end">${l.name}</text>
      <text x="${COL.qty}" y="${y}" font-size="11" fill="${ink}" text-anchor="end">${money(l.qtyMilli / 1000)} ${l.unit}</text>
      <text x="${COL.price}" y="${y}" font-size="11" fill="${ink}" text-anchor="end">${money(l.price)}</text>
      <text x="${COL.total}" y="${y}" font-size="11" fill="${ink}" text-anchor="end">${money(l.total)}</text>
      <line x1="24" y1="${y + 8}" x2="396" y2="${y + 8}" stroke="${line}" stroke-width="0.6"/>`;
    })
    .join("\n");

  const totalY = 268 + Math.min(lines.length, 14) * 26 + 30;
  const sumLabel = COL.total + 120;

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">
  <rect width="${W}" height="${H}" fill="#ffffff"/>
  <text x="396" y="52" font-size="19" font-weight="bold" fill="${ink}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">${s.settings.shopName || "فروشگاه"}</text>
  <text x="396" y="70" font-size="10" fill="${muted}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">${s.settings.shopPhone}</text>
  <text x="396" y="84" font-size="10" fill="${muted}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">${s.settings.shopAddress}</text>
  <line x1="24" y1="102" x2="396" y2="102" stroke="${ink}" stroke-width="1"/>
  <text x="396" y="128" font-size="14" font-weight="bold" fill="${ink}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">فاکتور فروش ${money(doc.number)}</text>
  <text x="396" y="148" font-size="10" fill="${muted}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">${longDate(doc.day)}</text>
  <text x="396" y="176" font-size="11" fill="${ink}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">خریدار: ${party?.name ?? ""}</text>
  <text x="${COL.pos}" y="212" font-size="10" fill="${muted}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">ردیف</text>
  <text x="${COL.name}" y="212" font-size="10" fill="${muted}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">شرح کالا</text>
  <text x="${COL.qty}" y="212" font-size="10" fill="${muted}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">مقدار</text>
  <text x="${COL.price}" y="212" font-size="10" fill="${muted}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">فی</text>
  <text x="${COL.total}" y="212" font-size="10" fill="${muted}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">جمع</text>
  <line x1="24" y1="224" x2="396" y2="224" stroke="${ink}" stroke-width="0.8"/>
  ${rows}
  <line x1="24" y1="${totalY - 12}" x2="396" y2="${totalY - 12}" stroke="${ink}" stroke-width="1"/>
  <text x="${sumLabel}" y="${totalY + 12}" font-size="11" fill="${muted}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">جمع کل</text>
  <text x="${COL.total}" y="${totalY + 12}" font-size="13" font-weight="bold" fill="${ink}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">${money(doc.total)}</text>
  ${
    doc.remaining > 0
      ? `<text x="${sumLabel}" y="${totalY + 36}" font-size="11" fill="${muted}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">مانده</text>
  <text x="${COL.total}" y="${totalY + 36}" font-size="13" font-weight="bold" fill="#a3382a" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">${money(doc.remaining)}</text>`
      : `<text x="${sumLabel}" y="${totalY + 36}" font-size="11" fill="#17614f" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">تسویه شد</text>`
  }
  <text x="396" y="${H - 28}" font-size="9" fill="${muted}" text-anchor="end" font-family="Vazirmatn, Tahoma, sans-serif">${s.settings.invoiceNote}</text>
</svg>`;
}
