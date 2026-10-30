// The only door to the Rust side.
//
// One command, `call`, with an operation name and arguments. Every operation
// the app can perform is listed here with its real types, so a screen never
// writes a stringly-typed call by hand twice.

import { invoke } from "@tauri-apps/api/core";
import { mockCall } from "./mock";
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
  SumRowInput,
  SumSheet,
  Task,
  TaskRow,
  TodayView,
  UpdateInfo,
  Totals,
} from "./types";

/** True inside the app; false when the interface is open in a plain browser. */
export const inApp = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(op: string, args: unknown = {}): Promise<T> {
  if (!inApp()) {
    // Designing in a browser has no Rust behind it, so a small ledger in memory
    // answers instead. It is never reached inside the built app.
    return (await mockCall(op, (args ?? {}) as Record<string, unknown>)) as T;
  }
  return (await invoke<T>("call", { op, args })) as T;
}

/**
 * Errors arrive already written in Persian from Rust, ready to show.
 *
 * Anything that is *not* Persian is a bug or a browser error, and a shopkeeper
 * must never see one: falling through to the same sentence the app uses for an
 * unknown failure beats showing him «Cannot read properties of undefined». The
 * original goes to the console for whoever is debugging.
 */
export function errorText(e: unknown): string {
  const guard = (detail: string): string => {
    console.warn(detail);
    return "مشکلی پیش آمد";
  };
  const persian = /[\u0600-\u06FF]/;
  if (typeof e === "string") return persian.test(e) ? e : guard(e);
  if (e && typeof e === "object" && "message" in e) {
    const message = String((e as Error).message);
    return persian.test(message) ? message : guard(message);
  }
  return "مشکلی پیش آمد";
}

export const api = {
  // ---- today -------------------------------------------------------------
  today: (day?: number) => call<TodayView>("home.today", { day: day ?? null }),

  // ---- settings ----------------------------------------------------------
  settings: () => call<Settings>("settings.get"),
  saveSettings: (settings: Settings) => call<boolean>("settings.save", { settings }),

  // ---- parties -----------------------------------------------------------
  parties: (opts: { q?: string; filter?: string; sort?: string } = {}) =>
    call<PartyCard[]>("parties.list", opts),
  party: (id: number) => call<Party>("parties.get", { id }),
  saveParty: (party: Partial<Party> & { name: string }) =>
    call<Party>("parties.save", { party }),
  partyBook: (id: number) => call<PartyBook>("parties.book", { id }),
  partiesSuggest: (q: string) => call<PartyCard[]>("parties.suggest", { q }),
  totals: () => call<Totals>("parties.totals"),

  // ---- products ----------------------------------------------------------
  products: (q?: string) => call<Product[]>("products.list", { q: q ?? null }),
  saveProduct: (product: Partial<Product> & { name: string }) =>
    call<Product>("products.save", { product }),
  priceHistory: (productId: number, partyId?: number) =>
    call<PricePoint[]>("products.history", { productId, partyId: partyId ?? null }),
  usualItems: (partyId: number) => call<Product[]>("products.usual", { partyId }),

  // ---- invoices ----------------------------------------------------------
  doc: (id: number) => call<DocFull>("orders.get", { id }),
  saveDoc: (input: DocInput) => call<Doc>("orders.save", { input }),
  voidDoc: (id: number, reason: string) => call<Doc>("orders.void", { id, reason }),
  setFinal: (id: number, final: boolean) => call<Doc>("orders.setFinal", { id, final }),
  repeatDoc: (id: number) => call<DocInput>("orders.repeat", { id }),
  docs: (opts: { kind?: string; partyId?: number; from?: number; to?: number; limit?: number } = {}) =>
    call<DocRow[]>("orders.list", opts),

  // ---- payments ----------------------------------------------------------
  savePayment: (input: PaymentInput) => call<Payment>("payments.save", { input }),
  voidPayment: (id: number, reason: string) => call<Payment>("payments.void", { id, reason }),
  payments: (opts: { partyId?: number; from?: number; to?: number; limit?: number } = {}) =>
    call<PaymentRow[]>("payments.list", opts),
  suggestAllocations: (partyId: number, amount: number) =>
    call<{ docId: number; amount: number }[]>("payments.suggest", { partyId, amount }),

  // ---- cheques -----------------------------------------------------------
  cheques: (opts: { status?: string } = {}) => call<ChequeRow[]>("cheques.list", opts),
  saveCheque: (cheque: Partial<Cheque> & { partyId: number; amount: number; dueDay: number }) =>
    call<Cheque>("cheques.save", { cheque }),
  setChequeStatus: (id: number, status: string) =>
    call<Cheque>("cheques.status", { id, status }),

  // ---- tasks -------------------------------------------------------------
  tasks: (opts: { includeDone?: boolean } = {}) => call<TaskRow[]>("tasks.list", opts),
  saveTask: (task: Partial<Task> & { text: string }) => call<Task>("tasks.save", { task }),
  toggleTask: (id: number) => call<Task>("tasks.toggle", { id }),
  removeTask: (id: number) => call<boolean>("tasks.remove", { id }),

  // ---- notes -------------------------------------------------------------
  notes: (opts: { q?: string; limit?: number } = {}) => call<Note[]>("notes.list", opts),
  saveNote: (note: Partial<Note> & { body: string }) => call<Note>("notes.save", { note }),
  removeNote: (id: number) => call<boolean>("notes.remove", { id }),

  // ---- sum sheets --------------------------------------------------------
  sums: () => call<SumSheet[]>("sums.list", {}),
  sumSheet: (id: number) => call<SumSheet>("sums.get", { id }),
  saveSumSheet: (sheet: { id?: number; day?: number; title?: string | null; rows: SumRowInput[] }) =>
    call<SumSheet>("sums.save", { sheet }),

  // ---- reports -----------------------------------------------------------
  reportDays: (from: number, to: number) =>
    call<ReportDays[]>("reports.days", { from, to }),
  reportProducts: (from: number, to: number) =>
    call<ProductReport[]>("reports.products", { from, to }),
  reportAgeing: () => call<AgeBucket[]>("reports.ageing", {}),

  // ---- search ------------------------------------------------------------
  search: (q: string) => call<SearchHit[]>("search.all", { q }),

  // ---- invoice and statement --------------------------------------------
  invoiceSvg: (docId: number) => call<{ svg: string }>("invoice.svg", { docId }),
  invoiceSend: (docId: number) =>
    call<{ ok: boolean; message: string }>("invoice.send", { docId }),
  statementSend: (partyId: number) =>
    call<{ ok: boolean; message: string }>("invoice.sendStatement", { partyId }),
  invoicePreview: (docId: number) => call<{ path: string }>("invoice.preview", { docId }),

  // ---- sharing -----------------------------------------------------------
  /** The message he would type himself, addressed to the customer by name. */
  invoiceText: (docId: number) =>
    call<{ text: string; title: string }>("share.invoiceText", { docId }),
  statementText: (partyId: number) =>
    call<{ text: string; title: string }>("share.statementText", { partyId }),

  // ---- telegram and backup ----------------------------------------------
  botCheck: (token: string) => call<BotInfo>("telegram.check", { token }),
  botLink: (token: string) => call<BotInfo>("telegram.link", { token }),
  backupNow: () => call<BackupInfo>("backup.now", {}),

  // ---- updates -----------------------------------------------------------
  updateCheck: () => call<UpdateInfo>("update.check", {}),
  updateDownload: () => call<{ path: string; size: number }>("update.download", {}),
  backupStatus: () => call<BackupInfo>("backup.status", {}),
};
