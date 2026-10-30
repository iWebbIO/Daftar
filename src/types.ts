// The shapes the Rust side sends back. Kept in step with src-tauri/src/dto.rs.

export interface Party {
  id: number;
  name: string;
  phone: string | null;
  kind: "customer" | "supplier" | "both";
  address: string | null;
  note: string | null;
  opening: number;
  archived: boolean;
  createdAt: number;
  updatedAt: number;
}

export interface PartyCard {
  party: Party;
  balance: number;
  lastDay: number | null;
  openDocs: number;
}

export interface Product {
  id: number;
  name: string;
  unit: string;
  salePrice: number;
  buyPrice: number;
  active: boolean;
  createdAt: number;
  updatedAt: number;
}

export interface PricePoint {
  price: number;
  day: number;
}

export interface DocLine {
  id: number;
  docId: number;
  pos: number;
  productId: number | null;
  name: string;
  qtyMilli: number;
  unit: string;
  price: number;
  total: number;
  overrideTotal: number | null;
  note: string | null;
  voided: boolean;
}

export interface LineInput {
  id?: number | null;
  productId?: number | null;
  name: string;
  qtyMilli: number;
  unit: string;
  price: number;
  overrideTotal?: number | null;
  note?: string | null;
  voided?: boolean;
}

export interface DocInput {
  id?: number | null;
  kind: "sale" | "purchase";
  partyId: number;
  day: number;
  discount: number;
  roundStep: number;
  note?: string | null;
  final: boolean;
  lines: LineInput[];
  receivedNow: number;
  receivedMethod: string;
  updatePrices: boolean;
}

export interface Doc {
  id: number;
  kind: "sale" | "purchase";
  jyear: number;
  number: number;
  partyId: number;
  day: number;
  status: "open" | "settled" | "void";
  discount: number;
  rounding: number;
  note: string | null;
  total: number;
  paid: number;
  remaining: number;
  final: boolean;
  voidReason: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface DocRow {
  doc: Doc;
  partyName: string;
}

export interface DocFull {
  doc: Doc;
  party: Party;
  lines: DocLine[];
}

export interface Payment {
  id: number;
  partyId: number;
  direction: "in" | "out";
  amount: number;
  day: number;
  method: string;
  ref: string | null;
  note: string | null;
  voided: boolean;
  voidReason: string | null;
  createdAt: number;
}

export interface AllocationInput {
  docId: number;
  amount: number;
}

export interface PaymentInput {
  id?: number | null;
  partyId: number;
  direction: "in" | "out";
  amount: number;
  day: number;
  method: string;
  ref?: string | null;
  note?: string | null;
  allocations: AllocationInput[];
}

export interface PaymentRow {
  payment: Payment;
  partyName: string;
  allocated: number;
  unallocated: number;
}

export interface Cheque {
  id: number;
  partyId: number;
  direction: "in" | "out";
  number: string | null;
  bank: string | null;
  amount: number;
  dueDay: number;
  status: "pending" | "cleared" | "bounced" | "returned";
  note: string | null;
  paymentId: number | null;
  createdAt: number;
}

export interface ChequeRow {
  cheque: Cheque;
  partyName: string;
}

export interface Task {
  id: number;
  text: string;
  day: number | null;
  partyId: number | null;
  done: boolean;
  createdAt: number;
}

export interface TaskRow {
  task: Task;
  partyName: string | null;
}

export interface Note {
  id: number;
  day: number;
  title: string | null;
  body: string;
  createdAt: number;
  updatedAt: number;
}

export interface SumRow {
  id: number;
  sheetId: number;
  pos: number;
  label: string;
  amount: number;
  partyId: number | null;
  struck: boolean;
}

export interface SumRowInput {
  id?: number;
  label: string;
  amount: number;
  partyId?: number | null;
  struck: boolean;
}

export interface SumSheet {
  id: number;
  day: number;
  title: string | null;
  createdAt: number;
  rows: SumRow[];
  total: number;
}

export interface TimelineItem {
  kind: string;
  id: number;
  day: number;
  title: string;
  amount: number;
  remaining: number;
  status: string;
}

export interface PartyBook {
  party: Party;
  balance: number;
  items: TimelineItem[];
  docs: Doc[];
  payments: Payment[];
}

export interface Totals {
  owesMe: number;
  iOwe: number;
}

export interface TodayView {
  day: number;
  jalali: string;
  jalaliLong: string;
  weekday: string;
  sold: number;
  bought: number;
  received: number;
  paid: number;
  docs: DocRow[];
  tasks: TaskRow[];
  cheques: ChequeRow[];
  totals: Totals;
  lastBackupAt: number | null;
}

export interface Settings {
  shopName: string;
  shopPhone: string;
  shopAddress: string;
  /** Printed on the invoice so a customer can pay by card. */
  bankName: string;
  cardHolder: string;
  cardNumber: string;
  invoiceNote: string;
  roundStep: number;
  botToken: string;
  chatId: string;
  currency: "toman" | "rial";
  backupHour: number;
  lastBackupAt: number | null;
  theme: "light" | "dark";
}

export interface SearchHit {
  kind: string;
  id: number;
  title: string;
  subtitle: string;
  amount: number | null;
}

export interface ReportDays {
  day: number;
  sold: number;
  bought: number;
  received: number;
  paid: number;
  count: number;
}

export interface ProductReport {
  productId: number | null;
  name: string;
  qtyMilli: number;
  unit: string;
  revenue: number;
  lastPrice: number;
}

export interface AgeBucket {
  label: string;
  amount: number;
  count: number;
}

export interface BotInfo {
  username: string;
  name: string;
  chatId: string | null;
  chatTitle: string | null;
}

export interface BackupInfo {
  at: number;
  size: number;
  sent: boolean;
  message: string;
}
