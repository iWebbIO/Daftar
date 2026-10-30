// فاکتور: the page he writes twenty times a day.
//
// The shape of the screen follows the shape of the job. A line is written in
// three moves — the thing, how much, at what price — and while it is being
// written the keypad owns the bottom of the screen and the total never leaves
// the top of it.

import { useEffect, useMemo, useRef, useState } from "react";
import { api, errorText } from "../api";
import { currencyLabel, fa, fmtAmount, fmtQty, lineTotal, parseAmount, parseQty } from "../format";
import { relativeDay, today } from "../jalali";
import { useApp, useNav } from "../nav";
import type { Doc, LineInput, Party, PartyCard, Product } from "../types";
import {
  Btn,
  Card,
  Empty,
  Field,
  Head,
  Icon,
  Loading,
  NumPad,
  Pill,
  Row,
  Screen,
  Segmented,
  Sheet,
} from "../ui/kit";
import "./order.css";

/** One line while it is being written, or after it is written. */
interface Line {
  /** The row in the book, once it has one. `null` means "not saved yet". */
  id: number | null;
  /** Only React's key: a new line has no row and must never borrow one. */
  key: number;
  productId: number | null;
  name: string;
  unit: string;
  qtyMilli: number;
  price: number;
  overrideTotal: number | null;
  voided: boolean;
}

type Pad =
  | { mode: "idle" }
  | { mode: "qty"; line: Line }
  | { mode: "price"; line: Line }
  | { mode: "override"; key: number; name: string }
  | { mode: "discount" }
  | { mode: "money" };

let nextKey = 1;
const newKey = () => (nextKey += 1);

/** The same rounding the Rust side does: nearest step, half up. */
function roundTo(amount: number, step: number): number {
  if (step <= 1) return amount;
  const half = Math.floor(step / 2);
  return Math.floor((amount + half) / step) * step;
}

const plain = (digits: string) => (digits === "" ? 0 : Number(digits));

export default function Order(props: { kind: "sale" | "purchase"; id?: number; partyId?: number }) {
  const nav = useNav();
  const { toast, settings } = useApp();
  const roundStep = settings?.roundStep ?? 0;
  const selling = props.kind === "sale";

  const [party, setParty] = useState<Party | null>(null);
  const [catalog, setCatalog] = useState<Product[]>([]);
  const [usual, setUsual] = useState<Product[]>([]);
  const [lines, setLines] = useState<Line[]>([]);
  const [discount, setDiscount] = useState(0);
  const [final, setFinal] = useState(false);
  const [doc, setDoc] = useState<Doc | null>(null);
  /** Which day this paper belongs to: forgetting yesterday is normal. */
  const [day, setDay] = useState<number>(() => today());

  const [previous, setPrevious] = useState<number | null>(null);
  const [ready, setReady] = useState(false);

  // picking the customer
  const [pq, setPq] = useState("");
  const [hits, setHits] = useState<PartyCard[]>([]);
  const [adding, setAdding] = useState(false);
  const [newName, setNewName] = useState("");

  // writing a line
  const [itemQ, setItemQ] = useState("");
  const [pad, setPad] = useState<Pad>({ mode: "idle" });
  const [digits, setDigits] = useState("");
  const [fresh, setFresh] = useState(true);

  // looking at a written line
  const [look, setLook] = useState<Line | null>(null);

  const [busy, setBusy] = useState(false);
  const [method, setMethod] = useState("cash");

  /** The last price this customer paid, asked for as soon as the thing is known. */
  const priceRef = useRef<Map<number, number>>(new Map());
  const linesRef = useRef<HTMLDivElement>(null);
  const id = props.id;

  // ---------------------------------------------------------------- loading

  useEffect(() => {
    void (async () => {
      try {
        setCatalog(await api.products());
        if (id) {
          const full = await api.doc(id);
          setParty(full.party);
          setDoc(full.doc);
        setDay(full.doc.day);
          setDiscount(full.doc.discount);
          setFinal(full.doc.final);
          setLines(
            full.lines.map((l) => ({
              id: l.id,
              key: l.id,
              productId: l.productId,
              name: l.name,
              unit: l.unit,
              qtyMilli: l.qtyMilli,
              price: l.price,
              overrideTotal: l.overrideTotal,
              voided: l.voided,
            })),
          );
        } else if (props.partyId) {
          setParty(await api.party(props.partyId));
        }
      } catch (e) {
        toast(errorText(e), "bad");
      } finally {
        setReady(true);
      }
    })();
  }, [id, props.partyId, toast]);

  // What this customer always takes, and the last paper he was given.
  useEffect(() => {
    if (!party) return;
    void (async () => {
      try {
        setUsual(await api.usualItems(party.id));
        const recent = await api.docs({ partyId: party.id, kind: props.kind, limit: 1 });
        setPrevious(recent[0]?.doc.id ?? null);
      } catch {
        // the order can still be written without either list
      }
    })();
  }, [party, props.kind]);

  // Recent customers while nobody is chosen yet: the six people the book saw
  // last, in the order `parties.suggest` knows them — the person he is writing
  // for now is almost always the person he wrote for last, whatever his kind.
  useEffect(() => {
    if (party) return;
    let alive = true;
    void (async () => {
      try {
        const rows = await api.partiesSuggest(pq.trim());
        if (alive) setHits(rows);
      } catch {
        if (alive) setHits([]);
      }
    })();
    return () => {
      alive = false;
    };
  }, [pq, party]);

  // Keep the newest line in sight as the page fills.
  useEffect(() => {
    const el = linesRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines.length]);

  // ----------------------------------------------------------------- totals

  const lineSum = useMemo(
    () =>
      lines
        .filter((l) => !l.voided)
        .reduce((n, l) => n + (l.overrideTotal ?? lineTotal(l.qtyMilli, l.price)), 0),
    [lines],
  );

  // What the line being written will add, so the total breathes while he types.
  const pending =
    pad.mode === "qty"
      ? lineTotal(parseQty(digits) ?? 0, pad.line.price)
      : pad.mode === "price"
        ? lineTotal(pad.line.qtyMilli, parseAmount(digits) ?? 0)
        : 0;

  const sum = lineSum + pending;
  const beforeRounding = sum - discount;
  const total = roundTo(beforeRounding, roundStep);
  const rounding = total - beforeRounding;

  const entered = parseAmount(digits) ?? 0;
  const left = Math.max(0, total - entered);

  // ---------------------------------------------------------------- writing

  /** The price he last charged this customer, when there is one. */
  const rememberPrice = (product: Product, key: number) => {
    if (!selling) return;
    void (async () => {
      try {
        const points = await api.priceHistory(product.id, party?.id);
        const last = points[0];
        if (last) priceRef.current.set(key, last.price);
      } catch {
        // the catalog price is already a good answer
      }
    })();
  };

  const startLine = (product: Product | null, typed?: string) => {
    const name = product?.name ?? (typed ?? "").trim();
    if (!name) return;
    const price = product ? (selling ? product.salePrice : product.buyPrice) : 0;
    const line: Line = {
      id: null,
      key: newKey(),
      productId: product?.id ?? null,
      name,
      unit: product?.unit ?? "کیلو",
      qtyMilli: 0,
      price,
      overrideTotal: null,
      voided: false,
    };
    if (product) rememberPrice(product, line.key);
    setItemQ("");
    setPad({ mode: "qty", line });
    setDigits("");
    setFresh(true);
  };

  const pressKey = (k: string) => {
    setDigits((d) => {
      if (k === "⌫") return d.slice(0, -1);
      const base = fresh ? "" : d;
      if (k === "٫") return base === "" || base.includes("٫") ? base : `${base}٫`;
      const next = base + k;
      return next.length > 12 ? d : next;
    });
    setFresh(false);
  };

  const submitPad = () => {
    switch (pad.mode) {
      case "qty": {
        const qty = parseQty(digits);
        if (!qty) return;
        const price = priceRef.current.get(pad.line.key) ?? pad.line.price;
        setPad({ mode: "price", line: { ...pad.line, qtyMilli: qty, price } });
        setDigits(String(price));
        setFresh(true);
        return;
      }
      case "price": {
        const price = parseAmount(digits) ?? 0;
        setLines((all) => [...all, { ...pad.line, price }]);
        setPad({ mode: "idle" });
        setDigits("");
        setFresh(true);
        return;
      }
      case "override": {
        const value = digits === "" ? null : parseAmount(digits);
        setLines((all) => all.map((l) => (l.key === pad.key ? { ...l, overrideTotal: value } : l)));
        setPad({ mode: "idle" });
        setDigits("");
        return;
      }
      case "discount": {
        setDiscount(digits === "" ? 0 : parseAmount(digits) ?? 0);
        setPad({ mode: "idle" });
        setDigits("");
        return;
      }
      case "money":
        void save(entered);
        return;
      default:
        return;
    }
  };

  const openPad = (next: Pad, start: string) => {
    setPad(next);
    setDigits(start);
    setFresh(start !== "");
  };

  const closePad = () => {
    setPad({ mode: "idle" });
    setDigits("");
    setFresh(true);
  };

  // ----------------------------------------------------------------- saving

  const buildLines = (): LineInput[] =>
    lines.map((l) => ({
      id: l.id,
      productId: l.productId,
      name: l.name,
      qtyMilli: l.qtyMilli,
      unit: l.unit,
      price: l.price,
      overrideTotal: l.overrideTotal,
      note: null,
      voided: l.voided,
    }));

  /** `received` is the money that actually changed hands in this save: what he
      typed at the counter on a new paper, nothing at all on an edit — a
      correction never moves cash. */
  const save = async (received: number) => {
    if (!party || busy) return;
    setBusy(true);
    try {
      const saved = await api.saveDoc({
        id: id ?? null,
        kind: props.kind,
        partyId: party.id,
        day,
        discount,
        roundStep,
        note: doc?.note ?? null,
        final,
        lines: buildLines(),
        receivedNow: received,
        receivedMethod: method,
        updatePrices: false,
      });
      const paper = selling ? "فاکتور" : "خرید";
      toast(id ? `${paper} ذخیره شد` : `${paper} ثبت شد`, "ok");
      // The invoice is not finished until it is in the customer's hands: land him
      // on the page that has the send button, not back on the day's list. An
      // edit goes back where it came from, because that paper is already out.
      if (!id) nav.replace({ name: "invoice", docId: saved.id });
      else if (nav.canGoBack) nav.back();
      else nav.replace({ name: "today" });
    } catch (e) {
      toast(errorText(e), "bad");
      setBusy(false);
    }
  };

  const repeatPrevious = async () => {
    if (!previous) return;
    try {
      const input = await api.repeatDoc(previous);
      setLines(
        input.lines.map((l) => ({
          id: l.id ?? null,
          key: newKey(),
          productId: l.productId ?? null,
          name: l.name,
          unit: l.unit,
          qtyMilli: l.qtyMilli,
          price: l.price,
          overrideTotal: l.overrideTotal ?? null,
          voided: l.voided ?? false,
        })),
      );
      setDiscount(input.discount);
      toast(selling ? "سطرهای فاکتور قبلی آمد" : "سطرهای خرید قبلی آمد", "ok");
    } catch (e) {
      toast(errorText(e), "bad");
    }
  };

  const createParty = async () => {
    const name = newName.trim();
    if (!name) return;
    setBusy(true);
    try {
      setParty(await api.saveParty({ name, kind: selling ? "customer" : "supplier" }));
      setAdding(false);
      setNewName("");
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  /** Which line the keypad is holding: «سطر ۲ · عدس», or «سطر تازه» for one
      that has not landed on the page yet. Two lines of the same thing are
      otherwise indistinguishable while a number is being typed. */
  const lineLabel = (key: number, name: string) => {
    const at = lines.findIndex((l) => l.key === key);
    return at < 0 ? `سطر تازه · ${name}` : `سطر ${fa(at + 1)} · ${name}`;
  };

  // -------------------------------------------------------------- rendering

  if (!ready) {
    return (
      <Screen className="ord">
        <Head title={selling ? "فاکتور" : "خرید"} onBack={nav.back} />
        <div className="ord-mid ord-mid--pad">
          <Loading />
        </div>
      </Screen>
    );
  }

  if (doc?.status === "void") {
    return (
      <Screen className="ord">
        <Head title={selling ? "فاکتور" : "خرید"} onBack={nav.back} />
        <div className="ord-mid ord-mid--pad">
          <Empty
            title="این فاکتور خط خورده است"
            hint={doc.voidReason ?? "دیگر قابل تغییر نیست."}
            action={<Btn onClick={nav.back}>برگشت</Btn>}
          />
        </div>
      </Screen>
    );
  }

  const typing = pad.mode === "qty" || pad.mode === "price";
  const asked = pad.mode === "money";
  const writing = pad.mode !== "idle";
  const query = itemQ.trim();
  const matches = query ? catalog.filter((p) => p.name.includes(query)).slice(0, 6) : [];
  const exact = matches.some((p) => p.name === query);

  return (
    <Screen className="ord">
      <Head
        title={selling ? "فاکتور" : "خرید"}
        sub={party ? party.name : selling ? "مشتری را انتخاب کن" : "فروشنده را انتخاب کن"}
        onBack={nav.back}
        right={
          doc ? (
            <Pill tone="brand">شماره {fa(doc.number)}</Pill>
          ) : party ? (
            <>
              {/* Forgetting yesterday is normal, so the date is one tap away:
                  each tap steps back a day, and «امروز» brings it home. */}
              <button
                className="ord-when"
                onClick={() => setDay((d) => Math.max(today() - 30, d - 1))}
                aria-label="تاریخ فاکتور"
              >
                {relativeDay(day)}
              </button>
              {day !== today() && (
                <button className="ord-change" onClick={() => setDay(today())}>
                  امروز
                </button>
              )}
              <button className="ord-change" onClick={() => setParty(null)}>
                تغییر
              </button>
            </>
          ) : undefined
        }
      />

      {/* ---- the customer, before anything else ---- */}
      {!party ? (
        <div className="ord-mid ord-mid--pad scroll">
          {/* The first tap is the name he is about to type, so the field is
              already holding the cursor when the screen arrives. */}
          <Field
            value={pq}
            onChange={setPq}
            placeholder={selling ? "نام مشتری…" : "نام فروشنده…"}
            autoFocus
          />
          <Btn variant="ghost" wide onClick={() => setAdding(true)}>
            <Icon name="plus" size={18} />
            {selling ? "مشتری جدید" : "فروشنده جدید"}
          </Btn>
          {hits.length > 0 && (
            <Card tight>
              {hits.map((h) => (
                <Row
                  key={h.party.id}
                  title={h.party.name}
                  sub={h.balance === 0 ? "حساب صفر" : h.balance > 0 ? "طلب شما" : "بدهی شما"}
                  value={h.balance === 0 ? undefined : fmtAmount(Math.abs(h.balance))}
                  tone={h.balance > 0 ? "pos" : h.balance < 0 ? "neg" : "plain"}
                  onClick={() => setParty(h.party)}
                />
              ))}
            </Card>
          )}
        </div>
      ) : asked ? (
        /* ---- how much of it he paid at the counter ---- */
        <div className="ord-mid ord-ask">
          <div className="ord-ask__mid">
            <div className="ord-ask__q">{selling ? "حالا پول گرفتی؟" : "حالا پول دادی؟"}</div>
            <div className="ord-ask__num num">{fmtAmount(entered)}</div>
            <div className={`ord-ask__left ${left === 0 ? "is-clear" : ""}`}>
              {left > 0 ? `مانده ${fmtAmount(left)}` : "تسویه شد"}
            </div>
            {entered > total && (
              <div className="ord-ask__over">
                {selling ? "اضافه‌اش به حسابش می‌ماند" : "اضافه‌اش طلبکاریت می‌شود"}
              </div>
            )}
            <Segmented
              options={[
                { value: "cash", label: "نقد" },
                { value: "card", label: "کارت" },
                { value: "cheque", label: "چک" },
              ]}
              value={method}
              onChange={setMethod}
            />
          </div>
        </div>
      ) : (
        /* ---- the lines, and the picker that adds the next one ---- */
        <>
          <div className={`ord-mid scroll ${typing ? "is-typing" : ""}`} ref={linesRef}>
            <div className="ord-stack">
              {lines.length > 0 ? (
                <Card tight className="ord-lines">
                  {lines.map((l) => (
                    <Row
                      key={l.key}
                      title={l.name}
                      sub={`${fmtQty(l.qtyMilli)} ${l.unit} × ${fmtAmount(l.price)}`}
                      // A crossed-out line shows no money at all: the amount is
                      // what the eye lands on first, and it is not being charged.
                      value={
                        l.voided
                          ? undefined
                          : fmtAmount(l.overrideTotal ?? lineTotal(l.qtyMilli, l.price))
                      }
                      note={l.voided ? "خط خورده" : l.overrideTotal !== null ? "جمع دستی" : undefined}
                      struck={l.voided}
                      onClick={() => setLook(l)}
                    />
                  ))}
                </Card>
              ) : (
                <>
                  {previous !== null && (
                    <Card tight>
                      <Row
                        leading={<Icon name="book" size={19} />}
                        title={selling ? "تکرار فاکتور قبلی" : "تکرار خرید قبلی"}
                        sub="همان سطرها، با همان فی"
                        onClick={() => void repeatPrevious()}
                      />
                    </Card>
                  )}
                  {usual.length > 0 && (
                    <>
                      <div className="section-head">
                        <span>کالاهای همیشگی</span>
                        <span className="faint">{selling ? "این مشتری" : "این فروشنده"}</span>
                      </div>
                      <Card tight>
                        {usual.map((p) => (
                          <Row
                            key={p.id}
                            title={p.name}
                            sub={p.unit}
                            value={fmtAmount(selling ? p.salePrice : p.buyPrice)}
                            onClick={() => startLine(p)}
                          />
                        ))}
                      </Card>
                    </>
                  )}
                </>
              )}
            </div>
          </div>

          {!writing && (
            <div className="ord-pick">
              {query !== "" && (
                <Card tight>
                  {matches.map((p) => (
                    <Row
                      key={p.id}
                      title={p.name}
                      sub={p.unit}
                      value={fmtAmount(selling ? p.salePrice : p.buyPrice)}
                      onClick={() => startLine(p)}
                    />
                  ))}
                  {!exact && (
                    <Row
                      leading={<Icon name="plus" size={17} />}
                      title={`«${query}» بنویس`}
                      sub="کالای جدید"
                      onClick={() => startLine(null, query)}
                    />
                  )}
                </Card>
              )}
              <Field value={itemQ} onChange={setItemQ} placeholder="کالای جدید…" />
              {query === "" && lines.length > 0 && usual.length > 0 && (
                <div className="ord-chips">
                  {usual.slice(0, 4).map((p) => (
                    <button key={p.id} className="ord-chip" onClick={() => startLine(p)}>
                      {p.name}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </>
      )}

      {/* ---- the total, always in sight ---- */}
      <div className={`ord-total ${final ? "is-final" : ""} ${writing ? "is-compact" : ""}`}>
        <div className="ord-total__main">
          <span className="ord-total__label">جمع</span>
          <span className="ord-total__num num">{fmtAmount(total)}</span>
          <span className="ord-total__unit">{currencyLabel()}</span>
        </div>
        {(discount !== 0 || rounding !== 0 || asked) && (
          <div className="ord-total__meta">
            {discount !== 0 && <span>تخفیف {fmtAmount(-discount)}</span>}
            {rounding !== 0 && <span>رند {fmtAmount(rounding)}</span>}
            {/* The money step covers the middle of the page, so what is still
                owed stays here, under the total, until it is settled. */}
            {asked && <span>{left > 0 ? `مانده ${fmtAmount(left)}` : "تسویه شد"}</span>}
          </div>
        )}
        {!writing && (
          <>
            {/* تخفیف and قطعی act on a total that does not exist yet, so they
                wait until there is something to discount. */}
            {lines.some((l) => !l.voided) && (
              <div className="ord-total__row">
                <button
                  className={`ord-chip ord-chip--soft ${discount !== 0 ? "is-on" : ""}`}
                  onClick={() => openPad({ mode: "discount" }, discount ? String(discount) : "")}
                >
                  تخفیف
                </button>
                <button
                  className={`ord-chip ord-chip--soft ${final ? "is-on" : ""}`}
                  onClick={() => setFinal(!final)}
                  aria-pressed={final}
                >
                  قطعی
                </button>
              </div>
            )}
            <Btn
              wide
              size="lg"
              busy={busy}
              disabled={!party || lines.every((l) => l.voided)}
              onClick={() => {
                // A paper already in the book is being corrected, not written:
                // no money changes hands now, so «حفظ» saves straight away.
                if (id) {
                  void save(0);
                  return;
                }
                setDigits("");
                setFresh(true);
                setPad({ mode: "money" });
              }}
            >
              {id ? "حفظ" : "ثبت"}
            </Btn>
          </>
        )}
      </div>

      {/* ---- the keypad, part of the screen ---- */}
      {pad.mode !== "idle" && (
        <div className="ord-pad">
          {pad.mode !== "money" && (
            <div className="ord-pad__shot">
              {pad.mode !== "discount" && (
                <span className="ord-pad__who">
                  {pad.mode === "override"
                    ? lineLabel(pad.key, pad.name)
                    : lineLabel(pad.line.key, pad.line.name)}
                </span>
              )}
              <div className="ord-pad__nums">
                {pad.mode === "qty" && (
                  <>
                    <span className="ord-pad__label">مقدار</span>
                    <span className="ord-pad__num num">{digits === "" ? "۰" : fa(digits)}</span>
                    <span className="ord-pad__unit">{pad.line.unit}</span>
                  </>
                )}
                {pad.mode === "price" && (
                  <>
                    <span className="ord-pad__label">فی</span>
                    <span className="ord-pad__num num">{fmtAmount(plain(digits))}</span>
                    <span className="ord-pad__side">
                      جمع {fmtAmount(lineTotal(pad.line.qtyMilli, plain(digits)))}
                    </span>
                  </>
                )}
                {pad.mode === "override" && (
                  <>
                    <span className="ord-pad__label">جمع سطر</span>
                    <span className="ord-pad__num num">{fmtAmount(plain(digits))}</span>
                  </>
                )}
                {pad.mode === "discount" && (
                  <>
                    <span className="ord-pad__label">تخفیف</span>
                    <span className="ord-pad__num num">{fmtAmount(plain(digits))}</span>
                    <span className="ord-pad__side">کم می‌شود</span>
                  </>
                )}
              </div>
            </div>
          )}
          <NumPad
            onKey={pressKey}
            onSubmit={submitPad}
            submitLabel={
              pad.mode === "qty"
                ? "بعدی"
                : pad.mode === "price"
                  ? "سطر شد"
                  : pad.mode === "money"
                    ? id
                      ? "حفظ"
                      : "ثبت"
                    : "تایید"
            }
          />
          {pad.mode !== "qty" && pad.mode !== "price" && (
            <button className="ord-pad__back" onClick={closePad}>
              برگشت
            </button>
          )}
        </div>
      )}

      {/* ---- a written line, in the hand ---- */}
      <Sheet open={look !== null} onClose={() => setLook(null)} title={look?.name}>
        {look && (
          <>
            <div className="ord-look">
              <span className="ord-look__calc">
                {fmtQty(look.qtyMilli)} {look.unit} × {fmtAmount(look.price)}
              </span>
              <span className="ord-look__total num">
                {fmtAmount(look.overrideTotal ?? lineTotal(look.qtyMilli, look.price))}
              </span>
            </div>
            <Card tight>
              <Row
                leading={<Icon name="plus" size={18} />}
                title="تکرار این سطر"
                sub="همان مقدار و همان فی"
                onClick={() => {
                  setLines((all) => [...all, { ...look, id: null, key: newKey() }]);
                  setLook(null);
                }}
              />
              <Row
                leading={<Icon name="edit" size={18} />}
                title="جمع را دستی بزن"
                sub="مقدار × فی را کنار بگذار"
                onClick={() => {
                  const start = look.overrideTotal ?? lineTotal(look.qtyMilli, look.price);
                  setLook(null);
                  openPad({ mode: "override", key: look.key, name: look.name }, String(start));
                }}
              />
              {look.overrideTotal !== null && (
                <Row
                  leading={<Icon name="calc" size={18} />}
                  title="جمع دستی برداشته شود"
                  sub="برگردد به مقدار × فی"
                  onClick={() => {
                    setLines((all) =>
                      all.map((l) => (l.key === look.key ? { ...l, overrideTotal: null } : l)),
                    );
                    setLook(null);
                  }}
                />
              )}
              <Row
                leading={<Icon name="close" size={18} />}
                title={look.voided ? "برگرداندن این سطر" : "خط زدن این سطر"}
                sub={look.voided ? "دوباره حساب می‌شود" : "در فاکتور می‌ماند، حساب نمی‌شود"}
                onClick={() => {
                  setLines((all) =>
                    all.map((l) => (l.key === look.key ? { ...l, voided: !l.voided } : l)),
                  );
                  setLook(null);
                }}
              />
            </Card>
          </>
        )}
      </Sheet>

      {/* ---- a new customer, with nothing but a name ---- */}
      <Sheet
        open={adding}
        onClose={() => setAdding(false)}
        title={selling ? "مشتری جدید" : "فروشنده جدید"}
      >
        <Field
          label="نام"
          value={newName}
          onChange={setNewName}
          autoFocus
          placeholder={selling ? "مثلاً مادر زهرا" : "مثلاً حاج آقا"}
        />
        <Btn wide busy={busy} disabled={!newName.trim()} onClick={() => void createParty()}>
          بساز
        </Btn>
      </Sheet>
    </Screen>
  );
}
