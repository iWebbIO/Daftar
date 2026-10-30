//! The stress pass: two hundred invoices, forty parties, a handful of payments,
//! cross-outs, and then the one invariant a shop cannot survive without.
//!
//! For every party, from the interface alone:
//!
//! ```text
//! the balance the app reports
//!   == everything their open invoices still say they owe
//!    − everything their open purchases still say I owe
//!    − money received that no invoice has used up
//!    + money paid out that no purchase has used up
//! ```
//!
//! And no document may say `settled` while still showing money on it.
//!
//! Every amount and date comes from a seeded generator, so the ledger is the
//! same on every machine and every run — and a failure here is a real defect,
//! not luck.

mod support;

use serde_json::json;
use support::*;

/// xorshift64*: seeded, tiny, deterministic. No crate, no clock, no network.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        assert!(seed != 0, "a seed of zero is a stuck generator");
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `lo..=hi`.
    fn between(&mut self, lo: i64, hi: i64) -> i64 {
        assert!(hi >= lo, "an empty range");
        let span = (hi - lo + 1) as u64;
        lo + (self.next() % span) as i64
    }

    fn one_in(&mut self, n: u64) -> bool {
        self.next() % n == 0
    }
}

/// One customer in eight is a supplier the shop buys from.
const SUPPLIER_EVERY: usize = 8;
const PARTIES: usize = 40;
const DOCUMENTS: usize = 200;
const PAYMENTS: usize = 60;

#[test]
fn two_hundred_invoices_keep_every_party_in_balance() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    // A seed that is written down, so this ledger can be rebuilt exactly.
    let mut rng = Rng::new(0x0DAF_7A12_0000_0001);

    // -- forty parties -----------------------------------------------------
    let mut parties = Vec::with_capacity(PARTIES);
    for i in 0..PARTIES {
        let kind = if i % SUPPLIER_EVERY == 0 {
            Some("supplier")
        } else {
            None
        };
        parties.push(party_of_kind(&ledger, &format!("طرف حساب {i}"), kind));
    }

    // -- two hundred documents ---------------------------------------------
    // Quantities in thousandths, prices in toman, discount always smaller than
    // the cheapest possible line, so no total can be negative.
    let mut documents: Vec<(i64, i64, String)> = Vec::new(); // (id, party, kind)
    for _ in 0..DOCUMENTS {
        let who = rng.between(0, PARTIES as i64 - 1) as usize;
        let party_id = parties[who];
        let is_supplier = who % SUPPLIER_EVERY == 0;
        let on = day - rng.between(0, 59);
        let lines: Vec<_> = (0..rng.between(1, 4))
            .map(|_| line("کالا", rng.between(1_000, 20_000), rng.between(1_000, 5_000)))
            .collect();
        let mut input = if is_supplier {
            purchase_input(party_id, on, lines)
        } else {
            doc_input(party_id, on, lines)
        };
        input["discount"] = json!(rng.between(0, 3) * 100); // 0, 100, 200 or 300
        input["roundStep"] = json!(if rng.one_in(3) { 100 } else { 0 });
        let doc = save_doc(&ledger, input);
        documents.push((
            n(&doc, "id"),
            party_id,
            text(&doc, "kind").to_string(),
        ));
    }
    assert_eq!(documents.len(), DOCUMENTS);

    // -- sixty payments, some of them too much on purpose ------------------
    // Overpaying is the interesting case: it must become credit on the party,
    // never a discount on the invoice.
    let mut payments: Vec<(i64, i64)> = Vec::new(); // (id, party)
    for _ in 0..PAYMENTS {
        let who = rng.between(0, PARTIES as i64 - 1) as usize;
        let party_id = parties[who];
        let direction = if who % SUPPLIER_EVERY == 0 { "out" } else { "in" };
        let amount = rng.between(1, 80) * 1_000;
        let on = day - rng.between(0, 30);
        let payment = pay(&ledger, party_id, direction, amount, on);
        payments.push((n(&payment, "id"), party_id));
    }

    // -- a few crossed-out lines, as a shop does every week -----------------
    // Fixed indices, not drawn from the generator: crossing out the same
    // document twice is refused, and this test must not depend on luck.
    for index in [3usize, 17, 42, 99, 123] {
        let (doc_id, _, _) = documents[index];
        let action = ledger.attempt(
            "orders.void",
            json!({ "id": doc_id, "reason": "فاکتور غلط بود" }),
        );
        assert!(action.is_ok(), "a document could not be crossed out: {action:?}");
    }
    for index in [5usize, 21] {
        let (payment_id, _) = payments[index];
        let action = ledger.attempt(
            "payments.void",
            json!({ "id": payment_id, "reason": "اشتباه ثبت شد" }),
        );
        assert!(action.is_ok(), "a payment could not be crossed out: {action:?}");
    }

    // -- the books, party by party -----------------------------------------
    // The list first: the party page and the party list are two doors to the
    // same number, and they must never disagree.
    let cards = ledger.call("parties.list", json!({}));
    let mut listed = std::collections::HashMap::new();
    for card in cards.as_array().expect("the list of parties") {
        listed.insert(n(&card["party"], "id"), n(card, "balance"));
    }
    assert_eq!(listed.len(), PARTIES, "every party is in the list");

    let mut booked = 0i64;
    let mut owed_to_me = 0i64;
    let mut owed_by_me = 0i64;
    let mut seen_documents = 0usize;
    // The states this pass has to reach to be worth anything.
    let mut in_credit = 0usize;
    let mut settled_documents = 0usize;
    let mut open_documents = 0usize;
    let mut void_documents = 0usize;
    let mut loose_payments = 0usize;
    let mut void_payments = 0usize;

    for &party_id in &parties {
        let page = ledger.call("parties.book", json!({ "id": party_id }));
        let reported = n(&page, "balance");

        // What the paper says: every live document's remainder.
        let mut sales_left = 0i64;
        let mut purchases_left = 0i64;
        for doc in docs_of(&ledger, party_id) {
            seen_documents += 1;
            let status = text(&doc, "status");
            let total = n(&doc, "total");
            let paid = n(&doc, "paid");
            let remaining = n(&doc, "remaining");

            assert!(
                paid + remaining == total,
                "a document must always add up: paid {paid} + remaining {remaining} != total {total}: {doc}"
            );
            assert!(paid >= 0, "money applied can never be negative: {doc}");
            assert!(
                remaining >= 0,
                "a document can never be overpaid into a negative remainder: {doc}"
            );

            match status {
                "void" => {
                    void_documents += 1;
                    continue;
                }
                "settled" => {
                    settled_documents += 1;
                    assert_eq!(
                        remaining, 0,
                        "a document marked settled still shows {remaining} on it: {doc}"
                    );
                }
                "open" => {
                    open_documents += 1;
                    assert!(
                        remaining > 0,
                        "a document marked open has nothing left on it: {doc}"
                    );
                }
                other => panic!("a document with an impossible status «{other}»: {doc}"),
            }

            if text(&doc, "kind") == "purchase" {
                purchases_left += remaining;
            } else {
                sales_left += remaining;
            }
        }

        // What the money says: the part of every live payment that no document
        // has used up. Money received is credit; money paid out is the mirror.
        let mut received_not_applied = 0i64;
        let mut paid_not_applied = 0i64;
        for row in payments_of(&ledger, party_id) {
            if row["payment"]["voided"] == json!(true) {
                void_payments += 1;
                continue;
            }
            let amount = n(&row["payment"], "amount");
            let allocated = n(&row, "allocated");
            let loose = n(&row, "unallocated");
            assert!(
                allocated + loose == amount,
                "a payment must always add up: {row}"
            );
            assert!(loose >= 0, "a payment can never apply more than it is: {row}");
            if loose > 0 {
                loose_payments += 1;
            }
            if text(&row["payment"], "direction") == "in" {
                received_not_applied += loose;
            } else {
                paid_not_applied += loose;
            }
        }

        let expected = sales_left - purchases_left - received_not_applied + paid_not_applied;
        assert_eq!(
            reported, expected,
            "party {party_id}: the page says {reported}, but the paper says\n  \
             open sales {sales_left} − open purchases {purchases_left} \
             − unapplied received {received_not_applied} \
             + unapplied paid out {paid_not_applied} = {expected}"
        );

        assert_eq!(
            listed.get(&party_id).copied(),
            Some(reported),
            "the list and the party's own page disagree about party {party_id}"
        );

        booked += reported;
        if reported > 0 {
            owed_to_me += reported;
        } else {
            owed_by_me += -reported;
            in_credit += 1;
        }
    }

    // Every document was read back, and the home banner adds the parties up.
    assert_eq!(seen_documents, DOCUMENTS, "no document went missing");
    let (owes_me, i_owe) = totals(&ledger);
    assert_eq!(owes_me, owed_to_me, "owes_me is the sum of what I am owed");
    assert_eq!(i_owe, owed_by_me, "i_owe is the sum of what I owe");
    assert_eq!(
        booked,
        owes_me - i_owe,
        "the parties' own balances add up to the banner"
    );

    // A pass that never reached the hard states would prove nothing: every one
    // of these existed at the end of this ledger.
    assert!(open_documents > 0, "no invoice was left open");
    assert!(settled_documents > 0, "no invoice was paid in full");
    assert!(void_documents >= 5, "the crossed-out documents were counted: {void_documents}");
    assert!(void_payments >= 2, "the crossed-out payments were counted: {void_payments}");
    assert!(
        loose_payments > 0,
        "no payment was left partly unapplied, so credit was never tested"
    );
    assert!(
        in_credit > 0,
        "no party ended up in credit, so a negative balance was never tested"
    );
}

