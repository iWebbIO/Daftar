// پول گرفتم: one person, one amount, one keypad — and a promise in plain words
// about which invoice the money lands on before it is saved.

import { useEffect, useMemo, useState } from "react";
import { api, errorText } from "../api";
import { currencyLabel, currencyScale, fa, fmtAmount, parseAmount } from "../format";
import { today } from "../jalali";
import { useApp, useNav } from "../nav";
import type { AllocationInput, Party, PartyBook, PartyCard } from "../types";
import {
  Card,
  Empty,
  Field,
  Head,
  Loading,
  NumPad,
  Row,
  Screen,
  Segmented,
} from "../ui/kit";
import "./order.css";
import "./payment.css";

export default function Payment(props: { partyId?: number }) {
  const nav = useNav();
  const { toast } = useApp();

  const [party, setParty] = useState<Party | null>(null);
  const [book, setBook] = useState<PartyBook | null>(null);
  const [ready, setReady] = useState(false);

  const [hits, setHits] = useState<PartyCard[]>([]);
  const [q, setQ] = useState("");

  const [digits, setDigits] = useState("");
  const [method, setMethod] = useState("cash");
  const [plan, setPlan] = useState<AllocationInput[]>([]);
  const [busy, setBusy] = useState(false);

  const id = props.partyId;

  useEffect(() => {
    void (async () => {
      try {
        if (id) {
          const loaded = await api.partyBook(id);
          setParty(loaded.party);
          setBook(loaded);
        }
      } catch (e) {
        toast(errorText(e), "bad");
      } finally {
        setReady(true);
      }
    })();
  }, [id, toast]);

  // Without a person, the recent ones are the fastest way in.
  useEffect(() => {
    if (party) return;
    let alive = true;
    void (async () => {
      try {
        const rows = await api.partiesSuggest(q.trim());
        if (alive) setHits(rows);
      } catch {
        if (alive) setHits([]);
      }
    })();
    return () => {
      alive = false;
    };
  }, [q, party]);

  const direction = useMemo<"in" | "out">(() => {
    if (!party) return "in";
    if (party.kind === "supplier") return "out";
    if (party.kind === "customer") return "in";
    return (book?.balance ?? 0) < 0 ? "out" : "in";
  }, [party, book]);

  const amount = parseAmount(digits) ?? 0;

  // What the money will do, asked again on every keystroke.
  useEffect(() => {
    if (!party || amount <= 0) {
      setPlan([]);
      return;
    }
    let alive = true;
    void (async () => {
      try {
        const rows = await api.suggestAllocations(party.id, amount);
        if (alive) setPlan(rows);
      } catch {
        if (alive) setPlan([]);
      }
    })();
    return () => {
      alive = false;
    };
  }, [party, amount]);

  const pressKey = (k: string) => {
    setDigits((d) => {
      if (k === "⌫") return d.slice(0, -1);
      if (k === "٫") return d === "" || d.includes("٫") ? d : `${d}٫`;
      const next = d + k;
      return next.length > 12 ? d : next;
    });
  };

  const pick = async (card: PartyCard) => {
    setParty(card.party);
    setReady(false);
    try {
      setBook(await api.partyBook(card.party.id));
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setReady(true);
    }
  };

  const save = async () => {
    if (!party || busy) return;
    if (amount <= 0) {
      toast("مبلغ را بزن", "bad");
      return;
    }
    setBusy(true);
    try {
      // The last word on which papers this money settles, asked again at the
      // moment of saving so a fast keystroke can never save a stale plan.
      const planNow = await api.suggestAllocations(party.id, amount);
      await api.savePayment({
        partyId: party.id,
        direction,
        amount,
        day: today(),
        method,
        allocations: planNow,
      });
      toast(direction === "in" ? "پول ثبت شد" : "پرداخت ثبت شد", "ok");
      if (nav.canGoBack) nav.back();
      else nav.replace({ name: "today" });
    } catch (e) {
      toast(errorText(e), "bad");
      setBusy(false);
    }
  };

  // ----------------------------------------------------------------- words

  // What is left over is a fact about the balance, not about the plan: a debt
  // that came from «مانده از قبل» has no paper to land on, and the money still
  // pays it down.
  const after = book ? book.balance + (direction === "in" ? -amount : amount) : 0;
  const over = direction === "in" ? Math.max(0, -after) : Math.max(0, after);
  const first = plan[0] ? book?.docs.find((d) => d.id === plan[0]!.docId) : undefined;
  // Before a number is typed the line still has something worth saying: where
  // the account stands right now.
  const shownBalance = amount === 0 ? (book?.balance ?? 0) : after;

  // Rial, a big debt, or both: the amount steps down before it can be cut off.
  const amountText = fmtAmount(amount);
  const amountSize =
    amountText.length > 13 ? "is-xlong" : amountText.length > 10 ? "is-long" : "";

  // What is actually owed to him (or by him). «همه» types exactly this, in the
  // shop's own unit, so the same parse path runs as if he had pressed the keys.
  const owed = direction === "in" ? Math.max(0, book?.balance ?? 0) : Math.max(0, -(book?.balance ?? 0));
  const settleAll = () => setDigits(String(Math.round(owed * currencyScale())));

  if (!ready) {
    return (
      <Screen className="pay">
        <Head
          title={direction === "in" ? "پول گرفتم" : "پول دادم"}
          sub={party?.name}
          onBack={nav.back}
        />
        <div className="pay-body">
          <Loading />
        </div>
      </Screen>
    );
  }

  return (
    <Screen className="pay">
      <Head
        title={direction === "in" ? "پول گرفتم" : "پول دادم"}
        sub={party ? party.name : direction === "in" ? "از کی؟" : "به کی؟"}
        onBack={nav.back}
        right={
          party ? (
            <button
              className="ord-change"
              onClick={() => {
                setParty(null);
                setBook(null);
                setDigits("");
              }}
            >
              تغییر
            </button>
          ) : undefined
        }
      />

      {!party ? (
        <div className="pay-body scroll">
          <Field value={q} onChange={setQ} placeholder="نام مشتری…" />
          {hits.length > 0 ? (
            <Card tight className="pay-hits">
              {hits.map((h) => (
                <Row
                  key={h.party.id}
                  title={h.party.name}
                  sub={h.balance === 0 ? "حساب صفر" : h.balance > 0 ? "طلب شما" : "بدهی شما"}
                  value={h.balance === 0 ? undefined : fmtAmount(Math.abs(h.balance))}
                  tone={h.balance > 0 ? "pos" : h.balance < 0 ? "neg" : "plain"}
                  onClick={() => void pick(h)}
                />
              ))}
            </Card>
          ) : (
            <Empty title="پیدا نشد" />
          )}
        </div>
      ) : (
        <>
          <div className="pay-body">
            <div className="pay-quote">
              <div className="pay-amount">
                <span className="pay-amount__label">مبلغ</span>
                <span className={`pay-amount__num num ${amountSize}`}>{amountText}</span>
                <span className="pay-amount__unit">{currencyLabel()}</span>
                {owed > 0 && (
                  <button className="pay-all" onClick={settleAll}>
                    همه
                  </button>
                )}
              </div>

              <div
                className={`pay-after ${
                  shownBalance > 0 ? "is-pos" : shownBalance < 0 ? "is-neg" : "is-clear"
                }`}
              >
                {!book
                  ? ""
                  : amount === 0
                    ? book.balance === 0
                      ? "حساب صفر است"
                      : book.balance > 0
                        ? `الآن ${fmtAmount(book.balance)} طلب شما`
                        : `الآن ${fmtAmount(-book.balance)} بدهی شما`
                    : after === 0
                      ? "حساب صفر می‌شود"
                      : after > 0
                        ? `بعد از این ${fmtAmount(after)} طلب شما`
                        : `بعد از این ${fmtAmount(-after)} بدهی شما`}
              </div>

              <div className="pay-say">
                {amount > 0 && first && (
                  <div className="pay-say__line">
                    اول از {first.kind === "purchase" ? "خرید" : "فاکتور"} {fa(first.number)} کم
                    می‌شود
                    {plan.length > 1 ? ` و ${fa(plan.length - 1)} برگه دیگر` : ""}
                  </div>
                )}
                {over > 0 && (
                  <div className="pay-say__line pay-say__line--credit">
                    {fmtAmount(over)} بیشتر است و به حسابش می‌ماند
                  </div>
                )}
              </div>
            </div>

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

          <div className="ord-pad">
            <NumPad
              onKey={pressKey}
              onSubmit={() => (busy ? undefined : void save())}
              submitLabel={busy ? "…" : "ثبت"}
            />
          </div>
        </>
      )}
    </Screen>
  );
}
