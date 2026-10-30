// One person's page: the balance, the two things you can do for him, and the
// history the way a notebook page reads — newest line at the top.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api, errorText } from "../api";
import { fmtAmount, fa, latin, parseAmount } from "../format";
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
  const [sending, setSending] = useState(false);

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
    setOpening(party.opening ? fmtAmount(party.opening) : "");
    setEditing(true);
  };

  const openPayment = (payment: Payment) => {
    setPay(payment);
    setAsking(false);
    setReason("");
  };

  const saveEdit = async () => {    setBusy(true);
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

  const sendStatement = async () => {
    setSending(true);
    try {
      const answer = await api.statementSend(id);
      toast(answer.message, answer.ok ? "ok" : "bad");
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setSending(false);
    }
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
    <Screen>
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
          <div className="pv__acts">
            <Btn onClick={() => nav.go({ name: "order", kind: supplier ? "purchase" : "sale", partyId: id })}>
              {supplier ? "خرید" : "فاکتور نو"}
            </Btn>
            <Btn variant="ghost" onClick={() => nav.go({ name: "payment", partyId: id })}>
              {supplier ? "پول دادم" : "پول گرفتم"}
            </Btn>
          </div>
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
                    title={item.title}
                    sub={dayMonth(item.day)}
                    value={fmtAmount(item.amount)}
                    tone={isPay ? (incoming ? "pos" : "neg") : "plain"}
                    struck={!isPay && item.status === "settled"}
                    note={
                      isPay
                        ? METHOD_WORD[payment?.method ?? "cash"] ?? "نقد"
                        : item.remaining > 0
                          ? `مانده ${fmtAmount(item.remaining)}`
                          : undefined
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
          <Empty title="حساب خالی است" hint="هنوز چیزی برای این نام نوشته نشده." />
        )}

        <Card tight>
          <Row
            leading={<Icon name="send" />}
            title="فرستادن صورت حساب"
            sub="به تلگرام"
            onClick={() => void sendStatement()}
            value={sending ? <span className="faint tiny">می‌رود…</span> : undefined}
          />
          <Row
            leading={<Icon name="edit" />}
            title="ویرایش حساب"
            sub="نام، تلفن، یادداشت، حساب قبلی"
            onClick={openEdit}
          />
        </Card>
      </Body>

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
              <span className="sheet-money__unit">تومان</span>
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
          suffix="تومان"
        />
        <Btn wide busy={busy} disabled={!name.trim()} onClick={saveEdit}>
          ذخیره
        </Btn>
      </Sheet>
    </Screen>
  );
}
