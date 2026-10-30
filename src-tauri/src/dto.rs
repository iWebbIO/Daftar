//! The shapes the app speaks in.
//!
//! Every struct here is serialised as-is to the interface, so field names are
//! `camelCase` and dates are Jalali while `day` stays the integer the database
//! stores. Money is integer toman; quantities are thousandths.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Party {
    pub id: i64,
    pub name: String,
    pub phone: Option<String>,
    pub kind: String,
    pub address: Option<String>,
    pub note: Option<String>,
    /// Positive: they owe me. Negative: I owe them.
    pub opening: i64,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A party plus the one number the shopkeeper actually looks at.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PartyCard {
    pub party: Party,
    pub balance: i64,
    pub last_day: Option<i64>,
    pub open_docs: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub unit: String,
    pub sale_price: i64,
    pub buy_price: i64,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PricePoint {
    pub price: i64,
    pub day: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct DocLine {
    pub id: i64,
    pub doc_id: i64,
    pub pos: i64,
    pub product_id: Option<i64>,
    pub name: String,
    pub qty_milli: i64,
    pub unit: String,
    pub price: i64,
    pub total: i64,
    pub override_total: Option<i64>,
    pub note: Option<String>,
    pub voided: bool,
}

/// What the order screen sends to be saved. `id` is absent for a new document.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LineInput {
    pub id: Option<i64>,
    pub product_id: Option<i64>,
    pub name: String,
    pub qty_milli: i64,
    pub unit: String,
    pub price: i64,
    pub override_total: Option<i64>,
    pub note: Option<String>,
    pub voided: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DocInput {
    pub id: Option<i64>,
    pub kind: String,
    pub party_id: i64,
    pub day: i64,
    pub discount: i64,
    pub round_step: i64,
    pub note: Option<String>,
    #[serde(rename = "final")]
    pub is_final: bool,
    pub lines: Vec<LineInput>,
    /// Cash handed over at the moment of saving.
    pub received_now: i64,
    pub received_method: String,
    pub update_prices: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Doc {
    pub id: i64,
    pub kind: String,
    pub jyear: i64,
    pub number: i64,
    pub party_id: i64,
    pub day: i64,
    pub status: String,
    pub discount: i64,
    pub rounding: i64,
    pub note: Option<String>,
    pub total: i64,
    pub paid: i64,
    pub remaining: i64,
    /// The circled total from the notebook: this invoice is final.
    #[serde(rename = "final")]
    pub is_final: bool,
    pub void_reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DocFull {
    pub doc: Doc,
    pub party: Party,
    pub lines: Vec<DocLine>,
}

/// A document as it appears in a list, with the name that makes it readable.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DocRow {
    pub doc: Doc,
    pub party_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Payment {
    pub id: i64,
    pub party_id: i64,
    pub direction: String,
    pub amount: i64,
    pub day: i64,
    pub method: String,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub note: Option<String>,
    pub voided: bool,
    pub void_reason: Option<String>,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PaymentInput {
    pub id: Option<i64>,
    pub party_id: i64,
    pub direction: String,
    pub amount: i64,
    pub day: i64,
    pub method: String,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub note: Option<String>,
    /// When empty, the amount is spread over the oldest unpaid documents first.
    pub allocations: Vec<AllocationInput>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AllocationInput {
    pub doc_id: i64,
    pub amount: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRow {
    pub payment: Payment,
    pub party_name: String,
    pub allocated: i64,
    pub unallocated: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Cheque {
    pub id: i64,
    pub party_id: i64,
    pub direction: String,
    pub number: Option<String>,
    pub bank: Option<String>,
    pub amount: i64,
    pub due_day: i64,
    pub status: String,
    pub note: Option<String>,
    pub payment_id: Option<i64>,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChequeRow {
    pub cheque: Cheque,
    pub party_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub text: String,
    pub day: Option<i64>,
    pub party_id: Option<i64>,
    pub done: bool,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskRow {
    pub task: Task,
    pub party_name: Option<String>,
}

// ---- inputs -----------------------------------------------------------------
//
// What the interface sends when it creates or edits something. `id` absent means
// "make a new one"; every other field may be absent so a screen that changes one
// thing does not have to send the whole record back.

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PartyInput {
    pub id: Option<i64>,
    pub name: String,
    pub phone: Option<String>,
    pub kind: Option<String>,
    pub address: Option<String>,
    pub note: Option<String>,
    pub opening: Option<i64>,
    pub archived: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProductInput {
    pub id: Option<i64>,
    pub name: String,
    pub unit: Option<String>,
    pub sale_price: Option<i64>,
    pub buy_price: Option<i64>,
    pub active: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub id: Option<i64>,
    pub text: String,
    pub day: Option<i64>,
    pub party_id: Option<i64>,
    pub done: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct NoteInput {
    pub id: Option<i64>,
    pub day: Option<i64>,
    pub title: Option<String>,
    pub body: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChequeInput {
    pub id: Option<i64>,
    pub party_id: i64,
    pub direction: Option<String>,
    pub number: Option<String>,
    pub bank: Option<String>,
    pub amount: i64,
    pub due_day: i64,
    pub note: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SumSheetInput {
    pub id: Option<i64>,
    pub day: Option<i64>,
    pub title: Option<String>,
    pub rows: Vec<SumRowInput>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SumRowInput {
    pub id: Option<i64>,
    pub label: String,
    pub amount: i64,
    pub party_id: Option<i64>,
    pub struck: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: i64,
    pub day: i64,
    pub title: Option<String>,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SumRow {
    pub id: i64,
    pub sheet_id: i64,
    pub pos: i64,
    pub label: String,
    pub amount: i64,
    pub party_id: Option<i64>,
    pub struck: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SumSheet {
    pub id: i64,
    pub day: i64,
    pub title: Option<String>,
    pub created_at: i64,
    pub rows: Vec<SumRow>,
    pub total: i64,
}

/// A timeline entry on a party's page: an invoice, a payment or a purchase.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItem {
    pub kind: String,
    pub id: i64,
    pub day: i64,
    pub title: String,
    pub amount: i64,
    pub remaining: i64,
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PartyBook {
    pub party: Party,
    pub balance: i64,
    pub items: Vec<TimelineItem>,
    pub docs: Vec<Doc>,
    pub payments: Vec<Payment>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub owes_me: i64,
    pub i_owe: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TodayView {
    pub day: i64,
    pub jalali: String,
    pub jalali_long: String,
    pub weekday: String,
    pub sold: i64,
    pub bought: i64,
    pub received: i64,
    pub paid: i64,
    pub docs: Vec<DocRow>,
    pub tasks: Vec<TaskRow>,
    pub cheques: Vec<ChequeRow>,
    pub totals: Totals,
    pub last_backup_at: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub shop_name: String,
    pub shop_phone: String,
    pub shop_address: String,
    /// Printed on the invoice so a customer can pay by card.
    pub bank_name: String,
    pub card_holder: String,
    pub card_number: String,
    pub invoice_note: String,
    pub round_step: i64,
    pub bot_token: String,
    pub chat_id: String,
    /// تومن or ریال — a display choice; the ledger is always in toman.
    pub currency: String,
    pub backup_hour: i64,
    pub last_backup_at: Option<i64>,
    pub theme: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub kind: String,
    pub id: i64,
    pub title: String,
    pub subtitle: String,
    pub amount: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReportDays {
    pub day: i64,
    pub sold: i64,
    pub bought: i64,
    pub received: i64,
    pub paid: i64,
    pub count: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProductReport {
    pub product_id: Option<i64>,
    pub name: String,
    pub qty_milli: i64,
    pub unit: String,
    pub revenue: i64,
    pub last_price: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgeBucket {
    pub label: String,
    pub amount: i64,
    pub count: i64,
}

/// What a bot token turned out to be, once Telegram answered.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BotInfo {
    pub username: String,
    pub name: String,
    pub chat_id: Option<String>,
    pub chat_title: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub at: i64,
    pub size: i64,
    pub sent: bool,
    pub message: String,
}
