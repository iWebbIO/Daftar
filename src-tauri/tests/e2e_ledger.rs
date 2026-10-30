//! The ten flows the ledger has to survive, driven through the interface.
//!
//! Every test here speaks the same language the app speaks: an operation name
//! and a JSON argument object, on the other side of `daftar_lib::route`. Not one
//! assertion is made against a module, a struct or a table, because the question
//! is never "does `orders::save` work" — it is "is the number on the screen the
//! number the shopkeeper would write in the notebook".
//!
//! Dates are relative to `home.today`, so nothing here depends on the day the
//! suite happens to run. Nothing touches the network.

mod support;

use serde_json::{json, Value};
use support::*;

// ---------------------------------------------------------------------------
// 1. the notebook page, end to end
// ---------------------------------------------------------------------------

/// The 30 June page: three lines, their three totals, the circled grand total,
/// and the one number the party's page shows — all read back through the
/// interface, in the order the shopkeeper would check them.
#[test]
fn the_notebook_page_adds_up_from_the_order_screen_to_the_party_page() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let customer = party(&ledger, "لیلا امینی");

    let saved = save_doc(
        &ledger,
        doc_input(
            customer,
            day,
            vec![
                line("لوبیا چیتی", 15_000, 345), // 15 kg × 345 = 5,175
                line("عدس", 20_000, 285),        // 20 kg × 285 = 5,700
                line("برنج", 35_000, 720),       // 35 kg × 720 = 25,200
            ],
        ),
    );

    assert_eq!(n(&saved, "total"), 36_075, "the circled total: {saved}");
    assert_eq!(n(&saved, "remaining"), 36_075, "nothing paid yet: {saved}");
    assert_eq!(n(&saved, "paid"), 0, "nothing paid yet: {saved}");
    assert_eq!(text(&saved, "status"), "open", "{saved}");
    assert_eq!(text(&saved, "kind"), "sale", "{saved}");
    assert_eq!(n(&saved, "number"), 1, "the first invoice of the year");
    assert_eq!(n(&saved, "discount"), 0);
    assert_eq!(n(&saved, "rounding"), 0);

    // Reading it back is the invoice screen's own read: the party, the lines,
    // the totals.
    let page = get_doc(&ledger, n(&saved, "id"));
    let lines = page["lines"].as_array().expect("the lines of the page");
    assert_eq!(lines.len(), 3, "three lines: {page}");

    assert_eq!(text(&lines[0], "name"), "لوبیا چیتی");
    assert_eq!(n(&lines[0], "qtyMilli"), 15_000);
    assert_eq!(n(&lines[0], "price"), 345);
    assert_eq!(n(&lines[0], "total"), 5_175, "15 kg × 345");

    assert_eq!(n(&lines[1], "total"), 5_700, "20 kg × 285");
    assert_eq!(n(&lines[2], "total"), 25_200, "35 kg × 720");

    let positions: Vec<i64> = lines.iter().map(|line| n(line, "pos")).collect();
    assert_eq!(positions, vec![0, 1, 2], "the lines keep the written order");

    let line_totals: i64 = lines.iter().map(|line| n(line, "total")).sum();
    assert_eq!(
        n(&page["doc"], "total"),
        line_totals,
        "the invoice adds up in public: {} != {line_totals}",
        n(&page["doc"], "total")
    );
    assert_eq!(text(&page["party"], "name"), "لیلا امینی");
    assert_eq!(n(&page["doc"], "id"), n(&saved, "id"));

    // And the two numbers the shop actually looks at agree with the page.
    assert_eq!(balance(&ledger, customer), 36_075, "what she owes");
    assert_eq!(totals(&ledger), (36_075, 0), "the home banner");
}

// ---------------------------------------------------------------------------
// 2. the returning customer
// ---------------------------------------------------------------------------

/// `orders.repeat` brings back what *this* customer paid last, not the catalogue
/// price that has moved since — and saving the repeat leaves the first invoice
/// exactly as it was.
#[test]
fn a_repeat_charges_this_customer_his_own_last_price_and_leaves_the_first_invoice_alone() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let catalogue = ledger.call(
        "products.save",
        json!({ "product": { "name": "عدس", "unit": "kg", "salePrice": 400 } }),
    );
    let product = n(&catalogue, "id");

    let leila = party(&ledger, "لیلا امینی");
    let mut first_input = doc_input(leila, day - 5, vec![line("عدس", 10_000, 345)]);
    first_input["lines"][0]["productId"] = json!(product);
    first_input["updatePrices"] = json!(true);
    let first = save_doc(&ledger, first_input);
    assert_eq!(n(&first, "total"), 3_450, "10 kg × 345");

    // The source's price rises: somebody else buys at 400 and the catalogue
    // follows. لیلا's own last price is still 345.
    let reza = party(&ledger, "رضا");
    let mut other_input = doc_input(reza, day - 1, vec![line("عدس", 5_000, 400)]);
    other_input["lines"][0]["productId"] = json!(product);
    other_input["updatePrices"] = json!(true);
    save_doc(&ledger, other_input);
    assert_eq!(
        n(&ledger.call("products.list", json!({ "q": "عدس" }))[0], "salePrice"),
        400,
        "the catalogue now says 400"
    );

    // «همان فاکتور قبلی» on today's date.
    let again = ledger.call("orders.repeat", json!({ "id": n(&first, "id") }));
    let repeat_line = &again["lines"][0];
    assert_eq!(
        n(repeat_line, "price"),
        345,
        "لیلا pays what she paid last time, not the catalogue: {again}"
    );
    assert_eq!(text(repeat_line, "name"), "عدس");
    assert_eq!(n(repeat_line, "qtyMilli"), 10_000, "the quantity comes back too");
    assert_eq!(n(&again, "partyId"), leila);
    assert_eq!(text(&again, "kind"), "sale");
    assert_eq!(n(&again, "day"), day, "the repeat is dated today");
    assert!(again["id"].is_null(), "a repeat is a new document: {again}");
    assert_eq!(again["updatePrices"], json!(false), "a repeat does not rewrite prices");

    // Saving it is a second invoice, and the first one is untouched.
    let second = save_doc(&ledger, again);
    assert_ne!(n(&second, "id"), n(&first, "id"), "a second document");
    assert_eq!(n(&second, "total"), 3_450, "same goods, same price");

    let first_now = get_doc(&ledger, n(&first, "id"));
    assert_eq!(n(&first_now["doc"], "total"), 3_450, "the first invoice is unchanged");
    assert_eq!(n(&first_now["doc"], "paid"), 0);
    assert_eq!(n(&first_now["doc"], "remaining"), 3_450);
    assert_eq!(text(&first_now["doc"], "status"), "open");
    assert_eq!(first_now["lines"].as_array().expect("lines").len(), 1);
    assert_eq!(n(&first_now["lines"][0], "price"), 345);

    // The catalogue is still the source's price: the repeat did not touch it.
    assert_eq!(
        n(&ledger.call("products.list", json!({ "q": "عدس" }))[0], "salePrice"),
        400,
        "a repeat must not rewrite the catalogue"
    );

    // Both invoices are on her page and both count.
    assert_eq!(docs_of(&ledger, leila).len(), 2);
    assert_eq!(balance(&ledger, leila), 6_900);
    assert_eq!(balance(&ledger, reza), 2_000);
}

// ---------------------------------------------------------------------------
// 3. part payment, then settlement
// ---------------------------------------------------------------------------

/// Half the money leaves the invoice `open` with an exact remainder; the rest
/// flips it to `settled` and empties the party's balance.
#[test]
fn a_part_payment_leaves_an_exact_remainder_and_the_second_payment_settles_it() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let customer = party(&ledger, "لیلا امینی");
    let invoice = save_doc(
        &ledger,
        doc_input(
            customer,
            day,
            vec![
                line("لوبیا چیتی", 15_000, 345),
                line("عدس", 20_000, 285),
                line("برنج", 35_000, 720),
            ],
        ),
    );
    let id = n(&invoice, "id");

    let first = pay(&ledger, customer, "in", 10_000, day);
    assert_eq!(n(&first, "amount"), 10_000);

    let half = get_doc(&ledger, id);
    assert_eq!(n(&half["doc"], "paid"), 10_000, "{half}");
    assert_eq!(
        n(&half["doc"], "remaining"),
        26_075,
        "36,075 − 10,000 is exact, not rounded: {half}"
    );
    assert_eq!(text(&half["doc"], "status"), "open", "still owed: {half}");

    let rows = payments_of(&ledger, customer);
    assert_eq!(rows.len(), 1);
    assert_eq!(n(&rows[0], "allocated"), 10_000, "the money landed on the invoice");
    assert_eq!(n(&rows[0], "unallocated"), 0, "no loose money");

    // The rest, to the toman.
    pay(&ledger, customer, "in", 26_075, day);

    let done = get_doc(&ledger, id);
    assert_eq!(n(&done["doc"], "paid"), 36_075, "{done}");
    assert_eq!(n(&done["doc"], "remaining"), 0, "{done}");
    assert_eq!(
        text(&done["doc"], "status"),
        "settled",
        "fully paid is settled, not open: {done}"
    );

    assert_eq!(balance(&ledger, customer), 0, "the page says zero");
    assert_eq!(totals(&ledger), (0, 0), "and so does the home banner");

    let rows = payments_of(&ledger, customer);
    assert_eq!(rows.len(), 2);
    let allocated: i64 = rows.iter().map(|row| n(row, "allocated")).sum();
    assert_eq!(allocated, 36_075, "the two payments add up to the invoice");
    assert!(
        rows.iter().all(|row| n(row, "unallocated") == 0),
        "nothing was left loose: {rows:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. overpayment is credit, not a discount
// ---------------------------------------------------------------------------

/// Handing over more than is owed leaves the excess on the customer's side of
/// the book — and that credit is what closes the gap on the next invoice.
#[test]
fn an_overpayment_stays_as_credit_and_offsets_the_next_invoice() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let customer = party(&ledger, "لیلا امینی");

    let first = save_doc(
        &ledger,
        doc_input(customer, day, vec![line("لوبیا چیتی", 15_000, 345)]),
    );
    assert_eq!(n(&first, "total"), 5_175);

    // 6,000 handed over against an invoice of 5,175.
    pay(&ledger, customer, "in", 6_000, day);

    let settled = get_doc(&ledger, n(&first, "id"));
    assert_eq!(n(&settled["doc"], "paid"), 5_175, "only the invoice was paid: {settled}");
    assert_eq!(n(&settled["doc"], "remaining"), 0, "{settled}");
    assert_eq!(text(&settled["doc"], "status"), "settled");

    assert_eq!(
        balance(&ledger, customer),
        -825,
        "the extra 825 is the shop's debt to her, to the toman"
    );
    assert_eq!(
        totals(&ledger),
        (0, 825),
        "it is reported as money I owe, never as money owed to me"
    );

    let rows = payments_of(&ledger, customer);
    assert_eq!(n(&rows[0], "allocated"), 5_175);
    assert_eq!(n(&rows[0], "unallocated"), 825, "the excess is credit");

    // A later invoice for 2,000. She now owes 2,000 − 825.
    let later = save_doc(
        &ledger,
        doc_input(customer, day, vec![line("عدس", 1_000, 2_000)]),
    );
    assert_eq!(n(&later, "total"), 2_000);
    assert_eq!(
        balance(&ledger, customer),
        1_175,
        "2,000 of goods less the 825 she already paid"
    );

    // The screen promises the spread before it happens: only the open invoice.
    let plan = ledger.call(
        "payments.suggest",
        json!({ "partyId": customer, "amount": 1_175 }),
    );
    let plan = plan.as_array().expect("an allocation plan");
    assert_eq!(plan.len(), 1, "the 825 credit is not re-allocated: {plan:?}");
    assert_eq!(
        n(&plan[0], "docId"),
        n(&later, "id"),
        "only the new invoice is still open"
    );
    assert_eq!(n(&plan[0], "amount"), 1_175);

    pay(&ledger, customer, "in", 1_175, day);

    let after = get_doc(&ledger, n(&later, "id"));
    assert_eq!(n(&after["doc"], "paid"), 1_175, "{after}");
    assert_eq!(
        n(&after["doc"], "remaining"),
        825,
        "the invoice itself is still short: {after}"
    );
    assert_eq!(text(&after["doc"], "status"), "open", "{after}");
    assert_eq!(
        balance(&ledger, customer),
        0,
        "but with her 825 of credit, nothing is owed: 825 − 825"
    );
    assert_eq!(totals(&ledger), (0, 0));

    // The credit never became a discount: the first invoice still says 5,175
    // and exactly 5,175 was applied to it.
    let first_now = get_doc(&ledger, n(&first, "id"));
    assert_eq!(n(&first_now["doc"], "total"), 5_175);
    assert_eq!(n(&first_now["doc"], "paid"), 5_175);
}

// ---------------------------------------------------------------------------
// 5. voiding restores the world
// ---------------------------------------------------------------------------

/// A crossed-out invoice and a crossed-out payment each put the balance back to
/// precisely what it was before they were written — with a reason, or not at all.
#[test]
fn voiding_an_invoice_and_a_payment_puts_the_balance_back_where_it_was() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let customer = party(&ledger, "لیلا امینی");

    // -- the invoice -------------------------------------------------------
    let before = balance(&ledger, customer);
    assert_eq!(before, 0, "a new party owes nothing");

    let invoice = save_doc(
        &ledger,
        doc_input(
            customer,
            day,
            vec![
                line("لوبیا چیتی", 15_000, 345),
                line("عدس", 20_000, 285),
                line("برنج", 35_000, 720),
            ],
        ),
    );
    assert_eq!(balance(&ledger, customer), 36_075);

    // No reason, no crossing out — and the refusal is a sentence, not a crash.
    let refused = ledger
        .attempt("orders.void", json!({ "id": n(&invoice, "id"), "reason": "  " }))
        .expect_err("a void without a reason must be refused");
    assert!(!refused.is_empty(), "the refusal is explained");
    assert_eq!(
        balance(&ledger, customer),
        36_075,
        "a refused void changes nothing"
    );

    let crossed = ledger.call(
        "orders.void",
        json!({ "id": n(&invoice, "id"), "reason": "فاکتور غلط بود" }),
    );
    assert_eq!(text(&crossed, "status"), "void");
    assert_eq!(text(&crossed, "voidReason"), "فاکتور غلط بود");
    assert_eq!(
        balance(&ledger, customer),
        before,
        "the balance is exactly what it was before the invoice"
    );
    assert_eq!(totals(&ledger), (0, 0));

    // The page still shows what was written: a crossed-out line is readable.
    let page = get_doc(&ledger, n(&invoice, "id"));
    assert_eq!(n(&page["doc"], "total"), 36_075, "the paper keeps its total");
    assert_eq!(page["lines"].as_array().expect("lines").len(), 3);
    let listed = docs_of(&ledger, customer);
    assert_eq!(listed.len(), 1, "it is still in the book");
    assert_eq!(text(&listed[0], "status"), "void", "marked, not erased");

    // -- the payment -------------------------------------------------------
    let second = save_doc(
        &ledger,
        doc_input(customer, day, vec![line("لوبیا چیتی", 15_000, 345)]),
    );
    let after_invoice = balance(&ledger, customer);
    assert_eq!(after_invoice, 5_175, "the second invoice counts again");

    let payment = pay(&ledger, customer, "in", 2_000, day);
    assert_eq!(balance(&ledger, customer), 3_175);

    let crossed = ledger.call(
        "payments.void",
        json!({ "id": n(&payment, "id"), "reason": "اشتباه ثبت شد" }),
    );
    assert_eq!(crossed["voided"], json!(true));
    assert_eq!(text(&crossed, "voidReason"), "اشتباه ثبت شد");
    assert_eq!(
        balance(&ledger, customer),
        after_invoice,
        "the balance is exactly what it was before the payment"
    );

    let reopened = get_doc(&ledger, n(&second, "id"));
    assert_eq!(n(&reopened["doc"], "paid"), 0, "the payment stopped counting");
    assert_eq!(n(&reopened["doc"], "remaining"), 5_175);
    assert_eq!(text(&reopened["doc"], "status"), "open", "open again, not settled");

    // A payment that was crossed out is refused a second time, in Persian.
    let again = ledger
        .attempt("payments.void", json!({ "id": n(&payment, "id"), "reason": "دوباره" }))
        .expect_err("the same payment must not be voided twice");
    assert!(!again.is_empty());

    // -- the money that was on a cancelled invoice -------------------------
    // Cash handed over against an invoice that is then cancelled does not
    // vanish and is not a discount: it becomes credit on the party.
    let third = save_doc(
        &ledger,
        doc_input(customer, day, vec![line("عدس", 1_000, 1_000)]),
    );
    pay(&ledger, customer, "in", 1_000, day);
    assert_eq!(
        balance(&ledger, customer),
        5_175,
        "the third invoice was paid off"
    );
    ledger.call(
        "orders.void",
        json!({ "id": n(&third, "id"), "reason": "مشتری پشیمان شد" }),
    );
    assert_eq!(
        balance(&ledger, customer),
        5_175 - 1_000,
        "the 1,000 she paid is now unapplied credit, not a discount"
    );
}

// ---------------------------------------------------------------------------
// 6. buying from the source
// ---------------------------------------------------------------------------

/// A purchase, part paid, is money I owe: it shows as a negative balance and in
/// `i_owe`, never in `owes_me`.
#[test]
fn a_purchase_paid_in_part_is_reported_as_money_i_owe() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let supplier = party_of_kind(&ledger, "حاج‌آقا", Some("supplier"));

    let bought = save_doc(
        &ledger,
        purchase_input(supplier, day, vec![line("برنج", 40_000, 1_000)]),
    );
    assert_eq!(text(&bought, "kind"), "purchase");
    assert_eq!(n(&bought, "total"), 40_000, "40 kg × 1,000");
    assert_eq!(n(&bought, "number"), 1, "purchases have their own series");

    assert_eq!(balance(&ledger, supplier), -40_000, "I owe him the whole lot");
    assert_eq!(
        totals(&ledger),
        (0, 40_000),
        "the debt is i_owe, and owes_me stays empty"
    );

    // 15,000 paid out to him, spread over the purchase.
    let paid = pay(&ledger, supplier, "out", 15_000, day);
    assert_eq!(text(&paid, "direction"), "out");

    let now = get_doc(&ledger, n(&bought, "id"));
    assert_eq!(n(&now["doc"], "paid"), 15_000, "{now}");
    assert_eq!(n(&now["doc"], "remaining"), 25_000, "{now}");
    assert_eq!(text(&now["doc"], "status"), "open", "{now}");

    assert_eq!(balance(&ledger, supplier), -25_000);
    assert_eq!(
        totals(&ledger),
        (0, 25_000),
        "25,000 still owed to the source, all of it in i_owe"
    );

    // And a customer who owes the shop is counted on the other side, at the
    // same time, without the two ever meeting.
    let customer = party(&ledger, "لیلا امینی");
    save_doc(
        &ledger,
        doc_input(customer, day, vec![line("لوبیا چیتی", 15_000, 345)]),
    );
    assert_eq!(
        totals(&ledger),
        (5_175, 25_000),
        "5,175 owed to me and 25,000 owed by me, kept apart"
    );

    // The list says the same thing, filter by filter: the supplier is under
    // «بدهی من» and absent from «طلب من».
    let owes = ledger.call("parties.list", json!({ "filter": "owes" }));
    let owes = owes.as_array().expect("the parties who owe me");
    assert!(
        owes.iter().all(|card| n(&card["party"], "id") != supplier),
        "the source I owe is never listed as someone who owes me: {owes:?}"
    );
    assert!(owes.iter().any(|card| n(&card["party"], "id") == customer));

    let i_owe = ledger.call("parties.list", json!({ "filter": "iowe" }));
    let i_owe = i_owe.as_array().expect("the parties I owe");
    let supplier_card = i_owe
        .iter()
        .find(|card| n(&card["party"], "id") == supplier)
        .unwrap_or_else(|| panic!("the supplier is missing from iowe: {i_owe:?}"));
    assert_eq!(n(supplier_card, "balance"), -25_000, "{supplier_card}");
}

// ---------------------------------------------------------------------------
// 7. the cheque round trip
// ---------------------------------------------------------------------------

/// A cheque is not money until it clears, clears exactly once even when the
/// button is pressed twice, and bouncing it puts the balance back.
#[test]
fn a_cheque_moves_the_balance_once_when_it_clears_and_back_when_it_bounces() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let customer = party(&ledger, "اکبر قصاب");
    let invoice = save_doc(
        &ledger,
        doc_input(customer, day, vec![line("گوشت", 10_000, 1_000)]),
    );
    assert_eq!(n(&invoice, "total"), 10_000);
    assert_eq!(balance(&ledger, customer), 10_000);

    let cheque = save_cheque(&ledger, customer, 3_000, day + 5);
    assert_eq!(text(&cheque, "status"), "pending");
    assert!(
        cheque["paymentId"].is_null(),
        "a pending cheque is not a payment: {cheque}"
    );
    assert_eq!(
        balance(&ledger, customer),
        10_000,
        "pending paper does not move the balance"
    );

    let cleared = cheque_status(&ledger, n(&cheque, "id"), "cleared");
    assert_eq!(text(&cleared, "status"), "cleared");
    let payment_id = n(&cleared, "paymentId");
    assert!(payment_id > 0, "clearing records a payment: {cleared}");
    assert_eq!(
        balance(&ledger, customer),
        7_000,
        "3,000 of the 10,000 arrived"
    );
    let paid_down = get_doc(&ledger, n(&invoice, "id"));
    assert_eq!(
        n(&paid_down["doc"], "paid"),
        3_000,
        "the money reached the invoice it was for: {paid_down}"
    );
    assert_eq!(n(&paid_down["doc"], "remaining"), 7_000, "{paid_down}");

    // Pressing «cleared» a second time must not count the money twice.
    let again = cheque_status(&ledger, n(&cheque, "id"), "cleared");
    assert_eq!(n(&again, "paymentId"), payment_id, "the same payment");
    assert_eq!(balance(&ledger, customer), 7_000, "and the same balance");

    let rows = payments_of(&ledger, customer);
    assert_eq!(rows.len(), 1, "one cheque, one payment: {rows:?}");
    assert_eq!(n(&rows[0]["payment"], "amount"), 3_000);
    assert_eq!(text(&rows[0]["payment"], "method"), "cheque");
    assert_eq!(
        text(&rows[0]["payment"], "ref"),
        "۱۲۳۴۵۶",
        "the slip and the payment agree"
    );

    // Bounced: the money never arrived, so the balance goes back to 10,000.
    let bounced = cheque_status(&ledger, n(&cheque, "id"), "bounced");
    assert_eq!(text(&bounced, "status"), "bounced");
    assert!(
        bounced["paymentId"].is_null(),
        "a bounced cheque forgets its payment: {bounced}"
    );
    assert_eq!(
        balance(&ledger, customer),
        10_000,
        "exactly where it was before the cheque"
    );
    let restored = get_doc(&ledger, n(&invoice, "id"));
    assert_eq!(
        n(&restored["doc"], "paid"),
        0,
        "the invoice it had paid is unpaid again: {restored}"
    );
    assert_eq!(n(&restored["doc"], "remaining"), 10_000, "{restored}");
    assert_eq!(
        text(&restored["doc"], "status"),
        "open",
        "a bounced cheque does not leave an invoice looking settled: {restored}"
    );
    let rows = payments_of(&ledger, customer);
    assert_eq!(rows.len(), 1, "the payment is voided, not deleted: {rows:?}");
    assert_eq!(rows[0]["payment"]["voided"], json!(true), "{rows:?}");
    assert!(
        rows[0]["payment"]["voidReason"].as_str().is_some_and(|r| !r.is_empty()),
        "the reason is kept on the paper: {rows:?}"
    );
    assert_eq!(totals(&ledger), (10_000, 0));
}

// ---------------------------------------------------------------------------
// 8. search finds what a shopkeeper would type
// ---------------------------------------------------------------------------

/// The amount on the invoice, typed with Persian digits and the Persian
/// thousands separator, typed as plain digits, and the customer's own name.
#[test]
fn search_finds_the_invoice_by_its_amount_in_every_style_and_by_the_name() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let customer = party(&ledger, "لیلا امینی");
    let invoice = save_doc(
        &ledger,
        doc_input(customer, day, vec![line("لوبیا چیتی", 15_000, 345)]),
    );
    let id = n(&invoice, "id");

    let typed_fa = fa_amount(5_175);
    assert_eq!(typed_fa, "۵٬۱۷۵", "the harness types what the brief says");

    for typed in [typed_fa.as_str(), "5175", "5,175"] {
        let hits = ledger.call("search.all", json!({ "q": typed }));
        let hits = hits.as_array().expect("a list of hits");
        let sale = hits
            .iter()
            .find(|hit| text(hit, "kind") == "sale")
            .unwrap_or_else(|| panic!("«{typed}» did not find the invoice: {hits:?}"));
        assert_eq!(n(sale, "id"), id, "«{typed}» found the wrong invoice");
        assert_eq!(
            n(sale, "amount"),
            5_175,
            "«{typed}» found the invoice, with its amount"
        );
    }

    // The customer's name, whole and in pieces — either finds her.
    for typed in ["لیلا", "امینی", "لیلا امینی"] {
        let hits = ledger.call("search.all", json!({ "q": typed }));
        let hits = hits.as_array().expect("a list of hits");
        let found = hits
            .iter()
            .find(|hit| text(hit, "kind") == "party" && n(hit, "id") == customer);
        assert!(
            found.is_some(),
            "«{typed}» did not find the customer: {hits:?}"
        );
    }

    // A single character is not a search, and an amount nobody wrote finds
    // nothing rather than everything.
    assert!(
        ledger
            .call("search.all", json!({ "q": "۵" }))
            .as_array()
            .expect("a list")
            .is_empty(),
        "one digit is the whole book"
    );
    assert!(
        ledger
            .call("search.all", json!({ "q": "8888" }))
            .as_array()
            .expect("a list")
            .is_empty(),
        "an amount nobody wrote finds nothing"
    );
}

// ---------------------------------------------------------------------------
// 9. nothing is lost across a restart
// ---------------------------------------------------------------------------

/// A day's work, the backup the app makes, and the snapshot opened again as a
/// fresh database: the totals and every document are identical.
#[test]
fn the_backup_snapshot_reopens_with_the_same_totals_and_the_same_documents() {
    let ledger = Ledger::on_disk("restart");
    let day = today(&ledger);
    let customer = party(&ledger, "لیلا امینی");
    let supplier = party_of_kind(&ledger, "حاج‌آقا", Some("supplier"));

    let invoice = save_doc(
        &ledger,
        doc_input(
            customer,
            day - 1,
            vec![
                line("لوبیا چیتی", 15_000, 345),
                line("عدس", 20_000, 285),
                line("برنج", 35_000, 720),
            ],
        ),
    );
    pay(&ledger, customer, "in", 10_000, day);
    let cheque = save_cheque(&ledger, customer, 3_000, day + 5);
    cheque_status(&ledger, n(&cheque, "id"), "cleared");

    let bought = save_doc(
        &ledger,
        purchase_input(supplier, day - 2, vec![line("برنج", 40_000, 1_000)]),
    );
    pay(&ledger, supplier, "out", 15_000, day);

    // What the screens show now.
    let totals_before = totals(&ledger);
    let customer_before = balance(&ledger, customer);
    let supplier_before = balance(&ledger, supplier);
    let customer_docs = fingerprints(&ledger, customer);
    let supplier_docs = fingerprints(&ledger, supplier);
    let customer_payments = payment_fingerprints(&ledger, customer);
    let invoice_before = get_doc(&ledger, n(&invoice, "id"));

    assert_eq!(customer_before, 36_075 - 10_000 - 3_000);
    assert_eq!(supplier_before, -25_000);
    assert_eq!(totals_before, (23_075, 25_000));

    // The backup the app takes at start-up. No bot is wired, so nothing leaves
    // the phone — and the copy is still made.
    let backup = ledger.call("backup.now", json!({}));
    assert_eq!(
        backup["sent"],
        json!(false),
        "no telegram, nothing sent: {backup}"
    );
    assert!(n(&backup, "size") > 0, "the zip has bytes in it: {backup}");
    assert!(
        text(&backup, "message").chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)),
        "the shopkeeper is told in Persian: {backup}"
    );

    let zips = zips_in(&ledger.files_dir());
    assert_eq!(zips.len(), 1, "one zip, named for the day: {zips:?}");

    // The database the zip carries, opened as a fresh ledger — a new phone, a
    // restart, a restore.
    let restored_path = ledger.files_dir().join("restored.db");
    unpack_database(&zips[0], &restored_path);
    let restored = ledger.reopen(&restored_path);

    assert_eq!(
        totals(&restored),
        totals_before,
        "the home banner survives the restart"
    );
    assert_eq!(balance(&restored, customer), customer_before);
    assert_eq!(balance(&restored, supplier), supplier_before);
    assert_eq!(fingerprints(&restored, customer), customer_docs);
    assert_eq!(fingerprints(&restored, supplier), supplier_docs);
    assert_eq!(payment_fingerprints(&restored, customer), customer_payments);

    let invoice_after = get_doc(&restored, n(&invoice, "id"));
    assert_eq!(
        invoice_after["lines"], invoice_before["lines"],
        "every line, price and quantity, survives"
    );
    assert_eq!(n(&invoice_after["doc"], "total"), n(&invoice_before["doc"], "total"));
    assert_eq!(n(&invoice_after["doc"], "paid"), n(&invoice_before["doc"], "paid"));
    assert_eq!(
        n(&invoice_after["doc"], "remaining"),
        n(&invoice_before["doc"], "remaining")
    );
    assert_eq!(text(&invoice_after["doc"], "status"), text(&invoice_before["doc"], "status"));
    assert_eq!(n(&get_doc(&restored, n(&bought, "id"))["doc"], "remaining"), 25_000);

    // And the restored ledger is a working ledger, not a read-only photograph.
    let fresh = save_doc(
        &restored,
        doc_input(customer, day, vec![line("عدس", 1_000, 1_000)]),
    );
    assert_eq!(n(&fresh, "total"), 1_000);
    assert_eq!(
        balance(&restored, customer),
        customer_before + 1_000,
        "the restored copy takes new invoices"
    );
}

// ---------------------------------------------------------------------------
// 10. correcting a saved invoice
// ---------------------------------------------------------------------------

/// Changing an invoice the shopkeeper already wrote: the total and the balance
/// follow it, the number in the corner does not move, and the invoice written
/// beside it keeps its own number.
#[test]
fn editing_a_saved_invoice_changes_the_money_and_never_the_number() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let customer = party(&ledger, "لیلا امینی");

    let first = save_doc(
        &ledger,
        doc_input(
            customer,
            day - 1,
            vec![
                line("لوبیا چیتی", 15_000, 345),
                line("عدس", 20_000, 285),
                line("برنج", 35_000, 720),
            ],
        ),
    );
    let first_id = n(&first, "id");
    assert_eq!(n(&first, "total"), 36_075);
    assert_eq!(n(&first, "number"), 1);

    // The page beside it.
    let neighbour = save_doc(
        &ledger,
        doc_input(customer, day, vec![line("خرما", 2_000, 500)]),
    );
    assert_eq!(n(&neighbour, "number"), 2, "the second invoice of the page");
    assert_eq!(n(&neighbour, "total"), 1_000);
    assert_eq!(balance(&ledger, customer), 37_075);

    // He notices the first line was written as 15 kg and it was 20. The screen
    // sends the whole document back — same id, same line ids, one line changed.
    let page = get_doc(&ledger, first_id);
    let mut lines: Vec<Value> = page["lines"]
        .as_array()
        .expect("the lines of the page")
        .iter()
        .map(|line| {
            json!({
                "id": n(line, "id"),
                "name": text(line, "name"),
                "qtyMilli": n(line, "qtyMilli"),
                "unit": text(line, "unit"),
                "price": n(line, "price"),
                "voided": false
            })
        })
        .collect();
    lines[0]["qtyMilli"] = json!(20_000); // 20 kg × 345 = 6,900, not 5,175

    let mut edit = doc_input(customer, n(&page["doc"], "day"), lines);
    edit["id"] = json!(first_id);
    let edited = save_doc(&ledger, edit);

    assert_eq!(n(&edited, "id"), first_id, "the same document, corrected");
    assert_eq!(n(&edited, "total"), 37_800, "6,900 + 5,700 + 25,200");
    assert_eq!(n(&edited, "remaining"), 37_800);
    assert_eq!(
        n(&edited, "number"),
        1,
        "correcting an invoice never renumbers it: {edited}"
    );
    assert_eq!(n(&edited, "jyear"), n(&first, "jyear"));
    assert_eq!(
        n(&edited, "createdAt"),
        n(&first, "createdAt"),
        "the day it was written is part of the record"
    );
    assert!(n(&edited, "updatedAt") >= n(&edited, "createdAt"));

    // The line was corrected in place: three lines, not six with three crossed
    // out, because every line came back with its own id.
    let after = get_doc(&ledger, first_id);
    let after_lines = after["lines"].as_array().expect("the lines");
    assert_eq!(after_lines.len(), 3, "the lines were corrected, not rewritten");
    assert_eq!(n(&after_lines[0], "qtyMilli"), 20_000);
    assert_eq!(n(&after_lines[0], "total"), 6_900);

    // The money follows the correction, to the toman.
    assert_eq!(
        balance(&ledger, customer),
        38_800,
        "37,800 + the 1,000 invoice beside it"
    );
    assert_eq!(totals(&ledger), (38_800, 0));

    // The page beside it is untouched, and the next invoice still gets 3.
    let neighbour_now = get_doc(&ledger, n(&neighbour, "id"));
    assert_eq!(n(&neighbour_now["doc"], "number"), 2, "no neighbour was renumbered");
    assert_eq!(n(&neighbour_now["doc"], "total"), 1_000);
    let third = save_doc(
        &ledger,
        doc_input(customer, day, vec![line("خرما", 1_000, 500)]),
    );
    assert_eq!(
        n(&third, "number"),
        3,
        "a correction did not hand out a number twice"
    );

    let numbers: std::collections::HashMap<i64, i64> = docs_of(&ledger, customer)
        .iter()
        .map(|doc| (n(doc, "id"), n(doc, "number")))
        .collect();
    assert_eq!(numbers[&first_id], 1);
    assert_eq!(numbers[&n(&neighbour, "id")], 2);
    assert_eq!(numbers[&n(&third, "id")], 3);
}

// ---------------------------------------------------------------------------
// 11. three screens, one number
// ---------------------------------------------------------------------------

/// A statement-shaped read: three invoices of different ages and two payments.
/// The party's page, the home banner and the debt report are three doors to one
/// number, and they must all open on the same figure.
#[test]
fn the_party_page_the_home_banner_and_the_debt_report_agree_on_one_number() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let customer = party(&ledger, "لیلا امینی");

    // 10,000 written 40 days ago, 20,000 twenty days ago, 30,000 three days ago.
    save_doc(
        &ledger,
        doc_input(customer, day - 40, vec![line("کالا", 1_000, 10_000)]),
    );
    save_doc(
        &ledger,
        doc_input(customer, day - 20, vec![line("کالا", 1_000, 20_000)]),
    );
    save_doc(
        &ledger,
        doc_input(customer, day - 3, vec![line("کالا", 1_000, 30_000)]),
    );

    // 15,000 five weeks ago: the oldest invoice, then what is left over of the
    // second. 5,000 ten days ago: the rest of the second.
    pay(&ledger, customer, "in", 15_000, day - 35);
    pay(&ledger, customer, "in", 5_000, day - 10);

    let docs = docs_of(&ledger, customer);
    assert_eq!(docs.len(), 3);
    let remaining: Vec<i64> = docs.iter().map(|doc| n(doc, "remaining")).collect();
    // Newest first: the 3-day 30,000 untouched, the 20-day 10,000 left, the
    // 40-day one paid off.
    assert_eq!(remaining, vec![30_000, 10_000, 0], "{docs:?}");
    assert_eq!(text(&docs[0], "status"), "open");
    assert_eq!(text(&docs[1], "status"), "open");
    assert_eq!(text(&docs[2], "status"), "settled", "the oldest was paid first");

    // One: the party's page.
    let page = ledger.call("parties.book", json!({ "id": customer }));
    let balance = n(&page, "balance");
    assert_eq!(balance, 40_000, "60,000 of goods less 20,000 paid");

    // Two: the home banner. She is the only party, so her balance is the whole
    // of what the shop is owed.
    assert_eq!(
        totals(&ledger),
        (balance, 0),
        "the banner and the page are one number"
    );
    assert_eq!(
        n(&ledger.call("parties.list", json!({}))[0], "balance"),
        balance,
        "and the list agrees too"
    );

    // Three: the debt report, aged invoice by invoice.
    let ageing = ledger.call("reports.ageing", json!({}));
    let buckets = ageing.as_array().expect("three buckets");
    assert_eq!(buckets.len(), 3);
    let amounts: Vec<i64> = buckets.iter().map(|bucket| n(bucket, "amount")).collect();
    let counts: Vec<i64> = buckets.iter().map(|bucket| n(bucket, "count")).collect();
    assert_eq!(
        amounts,
        vec![30_000, 10_000, 0],
        "3 days old in the first bucket, 20 days in the second, nothing older: {ageing}"
    );
    assert_eq!(counts, vec![1, 1, 0], "one invoice per bucket: {ageing}");
    assert_eq!(
        amounts.iter().sum::<i64>(),
        balance,
        "the debt report and the party's page say the same number: {} != {balance}",
        amounts.iter().sum::<i64>()
    );

    // And the statement itself reads like a page: newest first, invoices and
    // money interleaved.
    let kinds: Vec<&str> = page["items"]
        .as_array()
        .expect("the statement")
        .iter()
        .map(|item| text(item, "kind"))
        .collect();
    assert_eq!(
        kinds,
        vec!["sale", "payment", "sale", "payment", "sale"],
        "the newest line first: {kinds:?}"
    );
}

// ---------------------------------------------------------------------------
// the seam between cheques and invoices
// ---------------------------------------------------------------------------

/// A cheque is money a customer handed over against what he owes. The moment it
/// clears, the invoice it was for has to show less on it — otherwise the invoice
/// says he still owes 10,000 while his own page says 7,000, and the debt report
/// sends the shopkeeper after a man who has already paid.
///
/// Cash taken at the counter and cash taken by the payment screen are both
/// spread over the oldest unpaid invoice; a cheque has to be too.
#[test]
fn a_cleared_cheque_pays_down_the_invoice_it_was_for() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let customer = party(&ledger, "اکبر قصاب");
    let invoice = save_doc(
        &ledger,
        doc_input(customer, day, vec![line("گوشت", 10_000, 1_000)]),
    );
    let id = n(&invoice, "id");

    // A cheque for 3,000 of the 10,000 he owes, paid at the counter.
    let cheque = save_cheque(&ledger, customer, 3_000, day + 5);
    cheque_status(&ledger, n(&cheque, "id"), "cleared");

    // The party's page is right: he owes 7,000.
    assert_eq!(balance(&ledger, customer), 7_000);

    // So the invoice must be right too — the money was handed over for it.
    let after = get_doc(&ledger, id);
    assert_eq!(
        n(&after["doc"], "paid"),
        3_000,
        "the cleared cheque never reached the invoice: {after}"
    );
    assert_eq!(
        n(&after["doc"], "remaining"),
        7_000,
        "the invoice still claims the whole 10,000: {after}"
    );

    // And the report that answers «چقدر پول دست مردم مانده» must agree with
    // the customer's page, not contradict it.
    let ageing = ledger.call("reports.ageing", json!({}));
    let outstanding: i64 = ageing
        .as_array()
        .expect("the ageing buckets")
        .iter()
        .map(|bucket| n(bucket, "amount"))
        .sum();
    assert_eq!(
        outstanding, 7_000,
        "the debt report says {outstanding}, the customer's page says 7,000: {ageing}"
    );
}

// ---------------------------------------------------------------------------
// the interface's own contract
// ---------------------------------------------------------------------------

/// `types.ts` marks every line field except the four the shopkeeper must type as
/// optional (`id`, `productId`, `overrideTotal`, `note`, `voided`). A screen is
/// therefore allowed to send a line with only those four — and the save must
/// accept it rather than refusing the whole invoice.
#[test]
fn an_invoice_line_may_leave_the_optional_fields_out() {
    let ledger = Ledger::memory();
    let day = today(&ledger);
    let customer = party(&ledger, "لیلا امینی");

    let mut input = doc_input(customer, day, vec![]);
    input["lines"] = json!([
        { "name": "لوبیا چیتی", "qtyMilli": 15_000, "unit": "kg", "price": 345 }
    ]);

    let saved = ledger.attempt("orders.save", json!({ "input": input }));
    match saved {
        Ok(doc) => {
            assert_eq!(n(&doc, "total"), 5_175, "{doc}");
            assert_eq!(balance(&ledger, customer), 5_175);
        }
        Err(e) => panic!(
            "an invoice line without the optional fields «{}» was refused: {e}",
            "voided"
        ),
    }
}

/// Every operation name `api.ts` can send is answered by some module. A name the
/// router forgot is a button that can never work, and the shopkeeper only finds
/// out when he presses it.
#[test]
fn every_operation_the_interface_can_call_is_answered() {
    // A real ledger with real folders, because a couple of these write files.
    let ledger = Ledger::on_disk("router");

    // The names exactly as `src/api.ts` spells them. The four that would reach
    // the network (telegram.check, telegram.link, invoice.send,
    // invoice.sendStatement) are left out on purpose: a test may not dial out.
    let interface = [
        "home.today",
        "settings.get",
        "settings.save",
        "parties.list",
        "parties.get",
        "parties.save",
        "parties.book",
        "parties.suggest",
        "parties.totals",
        "products.list",
        "products.save",
        "products.history",
        "products.usual",
        "orders.get",
        "orders.save",
        "orders.void",
        "orders.setFinal",
        "orders.repeat",
        "orders.list",
        "payments.save",
        "payments.void",
        "payments.list",
        "payments.suggest",
        "cheques.list",
        "cheques.save",
        "cheques.status",
        "tasks.list",
        "tasks.save",
        "tasks.toggle",
        "tasks.remove",
        "notes.list",
        "notes.save",
        "notes.remove",
        "sums.list",
        "sums.get",
        "sums.save",
        "reports.days",
        "reports.products",
        "reports.ageing",
        "search.all",
        "invoice.svg",
        "invoice.preview",
        "backup.now",
        "backup.status",
    ];

    for op in interface {
        // No arguments: the operation should refuse the empty call in words
        // that belong to it, not claim that nobody knows its name.
        if let Err(e) = ledger.attempt(op, json!({})) {
            assert!(
                !e.contains("دستور ناشناخته"),
                "«{op}» is in api.ts but no module answers it: {e}"
            );
            assert!(
                !e.trim().is_empty(),
                "«{op}» refused the call without saying why"
            );
        }
    }

    // The check is not vacuous: a name nobody answers is refused the way the
    // router refuses one, and that is the sentence the test looks for.
    let unknown = ledger
        .attempt("orders.archive", json!({}))
        .expect_err("an operation nobody implements must be refused");
    assert!(
        unknown.contains("دستور ناشناخته"),
        "an unknown operation was not refused as one: {unknown}"
    );
}

// ---------------------------------------------------------------------------
// small helpers that belong to these tests, not to the app
// ---------------------------------------------------------------------------

/// Every document of a party, reduced to the numbers that must survive a
/// restart. Timestamps are left out on purpose: this is about the money.
fn fingerprints(ledger: &Ledger, party: i64) -> Vec<String> {
    docs_of(ledger, party)
        .iter()
        .map(|doc| {
            format!(
                "{} {} {} {} day={} total={} paid={} remaining={} status={}",
                n(doc, "id"),
                text(doc, "kind"),
                n(doc, "jyear"),
                n(doc, "number"),
                n(doc, "day"),
                n(doc, "total"),
                n(doc, "paid"),
                n(doc, "remaining"),
                text(doc, "status")
            )
        })
        .collect()
}

fn payment_fingerprints(ledger: &Ledger, party: i64) -> Vec<String> {
    payments_of(ledger, party)
        .iter()
        .map(|row| {
            format!(
                "{} amount={} allocated={} unallocated={} voided={}",
                n(&row["payment"], "id"),
                n(&row["payment"], "amount"),
                n(row, "allocated"),
                n(row, "unallocated"),
                row["payment"]["voided"]
            )
        })
        .collect()
}

/// The zips the backup module left in the files folder.
fn zips_in(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .expect("the files folder")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().map(|e| e == "zip").unwrap_or(false))
        .collect()
}

/// Takes the database out of a backup zip and writes it where a fresh ledger
/// can be opened on it.
fn unpack_database(zip: &std::path::Path, to: &std::path::Path) {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).expect("the zip opens"))
        .expect("a readable zip");
    let mut database = archive
        .by_name("daftar.db")
        .expect("the zip carries daftar.db");
    let mut bytes = Vec::new();
    database.read_to_end(&mut bytes).expect("the database bytes");
    assert!(
        bytes.starts_with(b"SQLite format 3\0"),
        "the snapshot really is a database file"
    );
    std::fs::write(to, &bytes).expect("the restored database file");
}
