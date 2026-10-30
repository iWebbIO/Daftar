// چک‌ها — the paper slips that become money.
//
// Grouped by when they come due, because that is the only question a cheque
// asks. An incoming cheque is money he will get; an outgoing one is money he
// must find, and both say so in words as well as colour.

import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorText } from "../api";
import { fa, fmtAmount, parseAmount } from "../format";
import { dayMonth, longDate, relativeDay, today, weekday } from "../jalali";
import { useApp, useNav } from "../nav";
import {
  Body,
  Btn,
  Card,
  Empty,
  Field,
  Head,
  Loading,
  Pill,
  Row,
  Screen,
  Segmented,
  Sheet,
} from "../ui/kit";
import type { Cheque, ChequeRow, PartyCard } from "../types";
import "./cheques.css";

const WORDS: Record<Cheque["status"], { word: string; tone: "plain" | "pos" | "neg" }> = {
  pending: { word: "در جریان", tone: "plain" },
  cleared: { word: "پاس شد", tone: "pos" },
  bounced: { word: "برگشت خورد", tone: "neg" },
  returned: { word: "برگشت خورد", tone: "neg" },
};

export default function Cheques() {
  const nav = useNav();
  const { toast } = useApp();
  const [rows, setRows] = useState<ChequeRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picked, setPicked] = useState<ChequeRow | null>(null);
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setRows(await api.cheques({}));
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const afterSave = useCallback(() => {
    setAdding(false);
    void load();
  }, [load]);

  const groups = useMemo(() => split(rows ?? []), [rows]);

  const setStatus = async (cheque: Cheque, status: Cheque["status"]) => {
    setBusy(true);
    try {
      await api.setChequeStatus(cheque.id, status);
      toast(status === "cleared" ? "چک پاس شد" : "چک برگشت خورد", status === "cleared" ? "ok" : "bad");
      setPicked(null);
      await load();
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  if (error && !rows) {
    return (
      <Screen className="cheques">
        <Head title="چک‌ها" onBack={nav.back} />
        <Body>
          <Empty title={error} action={<Btn onClick={load}>دوباره</Btn>} />
        </Body>
      </Screen>
    );
  }

  if (!rows) {
    return (
      <Screen className="cheques">
        <Head title="چک‌ها" onBack={nav.back} />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  const pending = rows.filter((r) => r.cheque.status === "pending");
  const incoming = pending.filter((r) => r.cheque.direction === "in");
  const outgoing = pending.filter((r) => r.cheque.direction === "out");

  return (
    <Screen className="cheques">
      <Head
        title="چک‌ها"
        onBack={nav.back}
        sub={pending.length > 0 ? `${fa(pending.length)} چک در جریان` : "چکی در جریان نیست"}
      />

      <Body>
        {pending.length > 0 && (
          <Card className="ch-sum">
            <div className="ch-sum__side">
              <div className="ch-sum__label">پول می‌گیرم</div>
              <div className="ch-sum__num num ch-sum__num--pos">{fmtAmount(sum(incoming))}</div>
              <div className="ch-sum__meta">{fa(incoming.length)} چک</div>
            </div>
            <div className="ch-sum__split" />
            <div className="ch-sum__side">
              <div className="ch-sum__label">پول می‌دهم</div>
              <div className="ch-sum__num num ch-sum__num--neg">{fmtAmount(sum(outgoing))}</div>
              <div className="ch-sum__meta">{fa(outgoing.length)} چک</div>
            </div>
          </Card>
        )}

        {rows.length === 0 && <Empty title="چکی ثبت نشده" />}

        <Group title="سررسید گذشته" rows={groups.overdue} onPick={setPicked} urgent />
        <Group title="این هفته" rows={groups.thisWeek} onPick={setPicked} />
        <Group title="بعد از این هفته" rows={groups.later} onPick={setPicked} />

        {groups.closed.length > 0 && (
          <>
            <div className="section-head">
              <span>بسته‌شده</span>
              <span className="faint">{fa(groups.closed.length)}</span>
            </div>
            <div className="ch-closed">
              <Card tight>
                {groups.closed.map((row) => (
                  <ChequeRowView key={row.cheque.id} row={row} />
                ))}
              </Card>
            </div>
          </>
        )}
      </Body>

      <div className="ch-foot">
        <Btn wide onClick={() => setAdding(true)}>
          چک تازه
        </Btn>
      </div>

      <ChequeSheet
        row={picked}
        busy={busy}
        onClose={() => setPicked(null)}
        onSet={(cheque, status) => void setStatus(cheque, status)}
      />
      <AddCheque open={adding} onClose={() => setAdding(false)} onSaved={afterSave} />
    </Screen>
  );
}

function Group({
  title,
  rows,
  onPick,
  urgent = false,
}: {
  title: string;
  rows: ChequeRow[];
  onPick: (row: ChequeRow) => void;
  urgent?: boolean;
}) {
  if (rows.length === 0) return null;
  return (
    <>
      <div className={`section-head ${urgent ? "ch-urgent" : ""}`}>
        <span>{title}</span>
        <span className="faint">{fa(rows.length)}</span>
      </div>
      <Card tight>
        {rows.map((row) => (
          <ChequeRowView key={row.cheque.id} row={row} onPick={onPick} />
        ))}
      </Card>
    </>
  );
}

function ChequeRowView({ row, onPick }: { row: ChequeRow; onPick?: (row: ChequeRow) => void }) {
  const { cheque, partyName } = row;
  const meta = WORDS[cheque.status];
  const incoming = cheque.direction === "in";
  return (
    <Row
      title={partyName}
      sub={`${incoming ? "می‌گیرم" : "می‌دهم"} · ${cheque.bank ?? "چک"} · ${when(cheque.dueDay)}`}
      value={fmtAmount(cheque.amount)}
      tone={incoming ? "pos" : "neg"}
      note={<Pill tone={meta.tone}>{meta.word}</Pill>}
      onClick={onPick ? () => onPick(row) : undefined}
    />
  );
}

function ChequeSheet({
  row,
  busy,
  onClose,
  onSet,
}: {
  row: ChequeRow | null;
  busy: boolean;
  onClose: () => void;
  onSet: (cheque: Cheque, status: Cheque["status"]) => void;
}) {
  if (!row) return null;
  const { cheque, partyName } = row;
  const incoming = cheque.direction === "in";
  return (
    <Sheet open onClose={onClose} title={partyName}>
      <div className="ch-big">
        <div className="ch-big__word">{incoming ? "می‌گیرم" : "می‌دهم"}</div>
        <div className={`ch-big__num num ${incoming ? "is-pos" : "is-neg"}`}>
          {fmtAmount(cheque.amount)}
        </div>
        <div className="ch-big__unit">تومان</div>
        <div className="ch-big__meta">
          {cheque.bank ?? "چک"} · سررسید {longDate(cheque.dueDay)}
        </div>
      </div>

      <div className="ch-actions">
        <Btn wide variant="danger" busy={busy} onClick={() => onSet(cheque, "bounced")}>
          برگشت خورد
        </Btn>
        <Btn wide busy={busy} onClick={() => onSet(cheque, "cleared")}>
          پاس شد
        </Btn>
      </div>
    </Sheet>
  );
}

function AddCheque({
  open,
  onClose,
  onSaved,
}: {
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { toast } = useApp();
  const [direction, setDirection] = useState<"in" | "out">("in");
  const [amount, setAmount] = useState("");
  const [bank, setBank] = useState("");
  const [offset, setOffset] = useState(0);
  const [q, setQ] = useState("");
  const [people, setPeople] = useState<PartyCard[]>([]);
  const [party, setParty] = useState<PartyCard | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setDirection("in");
    setAmount("");
    setBank("");
    setOffset(0);
    setQ("");
    setParty(null);
  }, [open]);

  useEffect(() => {
    if (!open || party) return;
    let live = true;
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const list = await api.partiesSuggest(q.trim());
          if (live) setPeople(list);
        } catch {
          if (live) setPeople([]);
        }
      })();
    }, 220);
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [q, open, party]);

  const value = parseAmount(amount);
  const due = today() + offset;

  const save = async () => {
    if (busy) return;
    if (!party) {
      toast("طرف حساب را انتخاب کن", "bad");
      return;
    }
    if (value === null || value <= 0) {
      toast("مبلغ را درست بنویس", "bad");
      return;
    }
    setBusy(true);
    try {
      await api.saveCheque({
        partyId: party.party.id,
        direction,
        amount: value,
        dueDay: due,
        bank: bank.trim() === "" ? null : bank.trim(),
      });
      toast("چک ثبت شد", "ok");
      onSaved();
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Sheet open={open} onClose={onClose} title="چک تازه" full>
      <div className="label">چه چکی؟</div>
      <Segmented
        options={[
          { value: "in", label: "چک گرفتم" },
          { value: "out", label: "چک دادم" },
        ]}
        value={direction}
        onChange={(v) => setDirection(v)}
      />

      <div className="label">طرف حساب</div>
      {party ? (
        <div className="ch-picked">
          <Pill tone="brand">{party.party.name}</Pill>
          <button className="ch-link" onClick={() => setParty(null)}>
            برداشتن
          </button>
        </div>
      ) : (
        <>
          <Field value={q} onChange={setQ} placeholder="نام را بنویس" autoFocus />
          {people.length > 0 && (
            <div className="ch-picks">
              {people.map((p) => (
                <button key={p.party.id} className="ch-pick" onClick={() => setParty(p)}>
                  {p.party.name}
                </button>
              ))}
            </div>
          )}
        </>
      )}

      <Field
        label="مبلغ"
        value={amount}
        onChange={setAmount}
        inputMode="numeric"
        placeholder="۰"
        suffix="تومان"
      />
      {value !== null && value > 0 && <div className="ch-echo num">{fmtAmount(value)} تومان</div>}

      <Field label="بانک" value={bank} onChange={setBank} placeholder="مثلاً ملت" />

      <div className="label">سررسید</div>
      <div className="ch-chips">
        {[
          { days: 0, label: "امروز" },
          { days: 7, label: "۷ روز" },
          { days: 15, label: "۱۵ روز" },
          { days: 30, label: "۱ ماه" },
        ].map((c) => (
          <button
            key={c.days}
            className={`ch-chip ${offset === c.days ? "is-on" : ""}`}
            onClick={() => setOffset(c.days)}
          >
            {c.label}
          </button>
        ))}
      </div>
      <div className="ch-due">سررسید {longDate(due)}</div>

      <div className="ch-submit">
        <Btn wide busy={busy} onClick={() => void save()}>
          ثبت چک
        </Btn>
      </div>
    </Sheet>
  );
}

function split(rows: ChequeRow[]) {
  const base = today();
  const weekEnd = base + (6 - weekday(base));
  const live = rows
    .filter((r) => r.cheque.status === "pending")
    .sort((a, b) => a.cheque.dueDay - b.cheque.dueDay);
  return {
    overdue: live.filter((r) => r.cheque.dueDay < base),
    thisWeek: live.filter((r) => r.cheque.dueDay >= base && r.cheque.dueDay <= weekEnd),
    later: live.filter((r) => r.cheque.dueDay > weekEnd),
    closed: rows
      .filter((r) => r.cheque.status !== "pending")
      .sort((a, b) => b.cheque.dueDay - a.cheque.dueDay),
  };
}

const sum = (rows: ChequeRow[]): number => rows.reduce((n, r) => n + r.cheque.amount, 0);

/** Near dates the way he says them; far ones with the month. */
function when(day: number): string {
  const diff = day - today();
  return diff >= -7 && diff <= 7 ? relativeDay(day) : dayMonth(day);
}
