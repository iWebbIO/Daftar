// One person's page: the balance, the two things you can do for him, and the
// history the way a notebook page reads — newest line at the top.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api, errorText } from "../api";
import { currencyLabel, fmtAmount, fa, latin, parseAmount, toInput } from "../format";
import { dayMonth } from "../jalali";
import { useApp, useNav } from "../nav";
import type { Payment, PartyBook, TimelineItem } from "../types";
import {
  Balance,
  Body,
  Btn,
  Card,
  Divider,
  Empty,
  Field,
  Head,
  Icon,
  Loading,
  Row,
  Screen,
  Sheet,
} from "../ui/kit";
import "./party.css";

const METHOD_WORD: Record<string, string> = {
  cash: "نقد",
  card: "کارت",
  cheque: "چک",
};

const KIND_WORD: Record<string, string> = {
  customer: "مشتری",
  supplier: "فروشنده",
  both: "مشتری و فروشنده",
};

export default function Party(props: { id: number }) {
  const nav = useNav();
  const { toast } = useApp();
  const [book, setBook] = useState<PartyBook | null>(null);
  const [error, setError] = useState<string | null>(null);

  // a payment, opened in a sheet
  const [pay, setPay] = useState<Payment | null>(null);
  const [asking, setAsking] = useState(false);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);

  // editing the party
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const [phone, setPhone] = useState("");
  const [note, setNote] = useState("");
  const [opening, setOpening] = useState("");

  // sending the statement, two ways
  const [choosing, setChoosing] = useState(false);
  const [which, setWhich] = useState<"tg" | "share" | null>(null);

  const id = props.id;

  const load = useCallback(async () => {
    try {
      setBook(await api.partyBook(id));
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, [id]);

  useEffect(() => {
    void load();
  }, [load]);

  if (error) {
    return (
      <Screen>
        <Head title="حساب" onBack={nav.back} />
        <Body>
          <Empty title={error} action={<Btn onClick={load}>دوباره</Btn>} />
        </Body>
      </Screen>
    );
  }

  if (!book) {
    return (
      <Screen>
        <Head title="حساب" onBack={nav.back} />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  const { party } = book;
  const supplier = party.kind === "supplier";
  const tel = party.phone ? `tel:${latin(party.phone)}` : null;

  const openEdit = () => {
    setName(party.name);
    setPhone(party.phone ?? "");
    setNote(party.note ?? "");
    setOpening(party.opening ? toInput(party.opening) : "");
    setEditing(true);
  };

  const openPayment = (payment: Payment) => {
    setPay(payment);
    setAsking(false);
    setReason("");
  };

  const saveEdit = async () => {
    setBusy(true);
    try {
      await api.saveParty({
        ...party,
        name: name.trim(),
        phone: phone.trim() || null,
        note: note.trim() || null,
        opening: opening.trim() ? parseAmount(opening) ?? 0 : 0,
      });
      setEditing(false);
      toast("ذخیره شد", "ok");
      await load();
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  // A statement goes out two ways: the bot he already has, or the phone's own
  // share sheet. The words themselves are written on the Rust side.
  const sendStatement = async () => {
    if (which !== null) return;
    setWhich("tg");
    try {
      const answer = await api.statementSend(id);
      toast(answer.message, answer.ok ? "ok" : "bad");
      setChoosing(false);
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setWhich(null);
    }
  };

  const shareStatement = async () => {
    if (which !== null) return;
    setWhich("share");
    setChoosing(false);
    let text: string;
    try {
      text = (await api.statementText(id)).text;
    } catch {
      toast("متن صورت حساب آماده نشد", "bad");
      setWhich(null);
      return;
    }
    try {
      await invoke("plugin:share|share", { text });
    } catch {
      // No share sheet here: the words are still worth having on the clipboard.
      try {
        await navigator.clipboard.writeText(text);
        toast("متن کپی شد", "ok");
      } catch {
        toast("اشتراک‌گذاری روی این گوشی نیست", "bad");
      }
    }
    setWhich(null);
  };

  const voidThePayment = async () => {
    if (!pay) return;
    setBusy(true);
    try {
      await api.voidPayment(pay.id, reason.trim());
      setPay(null);
      setAsking(false);
      setReason("");
      toast("پول خط خورد", "ok");
      await load();
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  const paymentOf = (item: TimelineItem): Payment | undefined =>
    item.kind === "payment" ? book.payments.find((p) => p.id === item.id) : undefined;

  return (
    <Screen className="party-page">
      <Head
        title={party.name}
        sub={KIND_WORD[party.kind] ?? "مشتری"}
        onBack={nav.back}
        right={
          tel && (
            <a className="phead-tel" href={tel} aria-label="زنگ زدن">
              <Icon name="phone" size={21} />
            </a>
          )
        }
      />

      <Body>
        <Card className="pv">
          <Balance value={book.balance} />
          {party.note && <div className="pv__note">{party.note}</div>}
        </Card>

        {book.items.length > 0 ? (
          <>
            <div className="section-head">
              <span>دفتر حساب</span>
              <span className="faint">{fa(book.items.length)} برگه</span>
            </div>
            <Card tight>
              {book.items.map((item) => {
                const payment = paymentOf(item);
                const isPay = item.kind === "payment";
                const incoming = payment?.direction === "in";
                return (
                  <Row
                    key={`${item.kind}-${item.id}`}
                    title={fa(item.title)}
                    sub={dayMonth(item.day)}
                    value={fmtAmount(item.amount)}
                    tone={isPay ? (incoming ? "pos" : "neg") : "plain"}
                    struck={!isPay && item.status === "settled"}
                    note={
                      isPay ? (
                        METHOD_WORD[payment?.method ?? "cash"] ?? "نقد"
                      ) : item.remaining > 0 ? (
                        <span className="pv-rest">مانده {fmtAmount(item.remaining)}</span>
                      ) : (
                        <span className="pv-paid">تسویه شد</span>
                      )
                    }
                    onClick={() =>
                      isPay
                        ? payment && openPayment(payment)
                        : nav.go({ name: "invoice", docId: item.id })
                    }
                  />
                );
              })}
            </Card>
          </>
        ) : (
          <Empty title="حساب خالی است" />
        )}

        <Card tight>
          <Row
            leading={<Icon name="send" />}
            title="فرستادن صورت حساب"
            sub="به تلگرام یا برنامه دیگر"
            onClick={() => setChoosing(true)}
            value={which ? <span className="faint tiny">می‌رود…</span> : undefined}
          />
          <Row
            leading={<Icon name="edit" />}
            title="ویرایش حساب"
            onClick={openEdit}
          />
        </Card>
      </Body>

      {/* The next thing he does with this person lives under his thumb. */}
      <div className="pacts">
        <Btn
          size="lg"
          onClick={() => nav.go({ name: "order", kind: supplier ? "purchase" : "sale", partyId: id })}
        >
          {supplier ? "خرید" : "فاکتور جدید"}
        </Btn>
        <Btn size="lg" variant="ghost" onClick={() => nav.go({ name: "payment", partyId: id })}>
          {supplier ? "پول دادم" : "پول گرفتم"}
        </Btn>
      </div>

      <Sheet
        open={pay !== null}
        onClose={() => {
          setPay(null);
          setAsking(false);
          setReason("");
        }}
        title={pay?.direction === "out" ? "پول دادم" : "پول گرفتم"}
      >
        {pay && (
          <>
            <div className="sheet-money">
              <span className="sheet-money__label">
                {pay.direction === "out" ? "به این حساب دادی" : "از این حساب گرفتی"}
              </span>
              <span className={`sheet-money__num num tone-${pay.direction === "out" ? "neg" : "pos"}`}>
                {fmtAmount(pay.amount)}
              </span>
              <span className="sheet-money__unit">{currencyLabel()}</span>
            </div>
            <Divider />
            <Row title="تاریخ" value={dayMonth(pay.day)} />
            <Row title="روش پرداخت" value={METHOD_WORD[pay.method] ?? "نقد"} />
            {pay.ref && <Row title="شماره" value={pay.ref} />}
            {pay.note && <Row title="یادداشت" value={pay.note} />}

            {asking ? (
              <div className="pv__void">
                <Field
                  label="چرا خط می‌خورد؟"
                  value={reason}
                  onChange={setReason}
                  placeholder="مثلاً اشتباه زده شد"
                  autoFocus
                />
                <div className="pv__void-acts">
                  <Btn variant="quiet" onClick={() => setAsking(false)}>
                    برگشت
                  </Btn>
                  <Btn variant="danger" busy={busy} disabled={!reason.trim()} onClick={voidThePayment}>
                    خط بزن
                  </Btn>
                </div>
              </div>
            ) : (
              <div className="pv__sheet-foot">
                <Btn variant="danger" wide onClick={() => setAsking(true)}>
                  خط زدن
                </Btn>
              </div>
            )}
          </>
        )}
      </Sheet>

      <Sheet open={choosing} onClose={() => setChoosing(false)} title="فرستادن صورت حساب">
        <Card tight>
          <Row
            leading={<Icon name="send" />}
            title="فرستادن به تلگرام"
            sub="به ربات خودت"
            value={which === "tg" ? <span className="faint tiny">می‌رود…</span> : undefined}
            onClick={() => void sendStatement()}
          />
          <Row
            leading={<Icon name="more" />}
            title="اشتراک‌گذاری"
            sub="به هر برنامه دیگری"
            value={which === "share" ? <span className="faint tiny">می‌رود…</span> : undefined}
            onClick={() => void shareStatement()}
          />
        </Card>
      </Sheet>

      <Sheet open={editing} onClose={() => setEditing(false)} title="ویرایش حساب">
        <Field label="نام" value={name} onChange={setName} autoFocus />
        <Field label="تلفن" value={phone} onChange={setPhone} inputMode="tel" placeholder="۰۹۱۲…" />
        <Field
          label="یادداشت"
          value={note}
          onChange={setNote}
          multiline
          rows={3}
          placeholder="مثلاً همسایه بازار"
        />
        <Field
          label="مانده از قبل"
          value={opening}
          onChange={setOpening}
          inputMode="numeric"
          placeholder="۰"
          suffix={currencyLabel()}
        />
        <Btn wide busy={busy} disabled={!name.trim()} onClick={saveEdit}>
          ذخیره
        </Btn>
      </Sheet>
    </Screen>
  );
}
