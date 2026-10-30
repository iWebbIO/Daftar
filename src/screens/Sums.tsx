// جمع — the numbered columns from his notebook.
//
// `#sums` is the pile of pages he already wrote; tap one to open it, or start a
// new one. A row is a name and an amount, the total under the lines keeps up on
// its own, and struck rows stop counting but stay on the page.

import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorText } from "../api";
import { currencyLabel, fa, fmtAmount, latin, parseAmount, toInput } from "../format";
import { longDate, today } from "../jalali";
import { useApp, useNav } from "../nav";
import {
  Body,
  Btn,
  Card,
  Empty,
  Field,
  Head,
  Icon,
  Loading,
  NumPad,
  Row,
  Screen,
  Sheet,
} from "../ui/kit";
import type { SumRow, SumSheet } from "../types";
import "./sums.css";

interface DraftRow {
  key: number;
  id: number;
  label: string;
  amount: number;
  struck: boolean;
}

interface Draft {
  id?: number;
  day: number;
  title: string;
  rows: DraftRow[];
}

let KEY = 1;
const nextKey = (): number => KEY++;

export default function Sums(props: { id?: number }) {
  return props.id === undefined ? <SheetStart /> : <SheetEditor id={props.id} />;
}

/**
 * Without an id: the sheets he already has. With nothing to list, the blank
 * page is the whole screen — there is nothing to choose between.
 */
function SheetStart() {
  const nav = useNav();
  const [sheets, setSheets] = useState<SumSheet[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const load = useCallback(async () => {
    try {
      const list = await api.sums();
      setSheets([...list].sort((a, b) => b.day - a.day || b.id - a.id));
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (creating) {
    return <SheetEditor onBack={() => setCreating(false)} />;
  }

  if (error && !sheets) {
    return (
      <Screen className="sums">
        <Head title="جمع" onBack={nav.back} />
        <Body>
          <Empty title={error} action={<Btn onClick={load}>دوباره</Btn>} />
        </Body>
      </Screen>
    );
  }

  if (!sheets) {
    return (
      <Screen className="sums">
        <Head title="جمع" onBack={nav.back} />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  if (sheets.length === 0) {
    return <SheetEditor />;
  }

  return (
    <Screen className="sums">
      <Head title="جمع" onBack={nav.back} sub={`${fa(sheets.length)} برگه`} />

      <Body>
        <Card tight>
          {sheets.map((sheet) => (
            <Row
              key={sheet.id}
              title={sheet.title?.trim() ? sheet.title : longDate(sheet.day)}
              sub={`${fa(sheet.rows.length)} ردیف`}
              value={fmtAmount(sheet.total)}
              note={sheet.title?.trim() ? longDate(sheet.day) : undefined}
              onClick={() => nav.go({ name: "sums", id: sheet.id })}
            />
          ))}
        </Card>
      </Body>

      <div className="sm-foot">
        <Btn wide onClick={() => setCreating(true)}>
          برگه جدید
        </Btn>
      </div>
    </Screen>
  );
}

function SheetEditor({ id, onBack }: { id?: number; onBack?: () => void }) {
  const nav = useNav();
  const { toast } = useApp();
  const [draft, setDraft] = useState<Draft | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<{ index: number | null } | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      if (id === undefined) {
        setDraft({ day: today(), title: "", rows: [] });
      } else {
        setDraft(toDraft(await api.sumSheet(id)));
      }
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, [id]);

  useEffect(() => {
    void load();
  }, [load]);

  const total = useMemo(
    () => (draft ? draft.rows.reduce((n, r) => (r.struck ? n : n + r.amount), 0) : 0),
    [draft],
  );
  const struck = useMemo(() => (draft ? draft.rows.filter((r) => r.struck).length : 0), [draft]);

  const back = onBack ?? nav.back;

  const put = (index: number | null, row: { label: string; amount: number; struck: boolean }) => {
    setDraft((old) => {
      if (!old) return old;
      if (index === null) {
        return { ...old, rows: [...old.rows, { key: nextKey(), id: 0, ...row }] };
      }
      return { ...old, rows: old.rows.map((r, i) => (i === index ? { ...r, ...row } : r)) };
    });
    setEditing(null);
  };

  const save = async () => {
    if (!draft || busy) return;
    setBusy(true);
    try {
      const saved = await api.saveSumSheet({
        id: draft.id,
        day: draft.day,
        title: draft.title.trim() === "" ? null : draft.title.trim(),
        rows: draft.rows.map((r, i) => ({
          id: r.id,
          sheetId: draft.id ?? 0,
          pos: i + 1,
          label: r.label,
          amount: r.amount,
          partyId: null,
          struck: r.struck,
        })),
      });
      setDraft(toDraft(saved));
      toast("جمع ذخیره شد", "ok");
      if (draft.id === undefined) {
        // A new page gets its own address, so it can be found again.
        nav.replace({ name: "sums", id: saved.id });
      } else if (nav.canGoBack) {
        back();
      }
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  if (error && !draft) {
    return (
      <Screen className="sums">
        <Head title="جمع" onBack={back} />
        <Body>
          <Empty title={error} action={<Btn onClick={load}>دوباره</Btn>} />
        </Body>
      </Screen>
    );
  }

  if (!draft) {
    return (
      <Screen className="sums">
        <Head title="جمع" onBack={back} />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  const index = editing ? editing.index : null;

  return (
    <Screen className="sums">
      <Head title="جمع" onBack={back} sub={longDate(draft.day)} />

      <Body>
        <Field
          label="عنوان برگه"
          value={draft.title}
          onChange={(title) => setDraft({ ...draft, title })}
          placeholder="مثلاً جمع هفته"
        />

        <Card tight>
          {draft.rows.map((row, i) => (
            <button key={row.key} className="sm-row" onClick={() => setEditing({ index: i })}>
              <span className="sm-row__pos num">{fa(i + 1)}</span>
              <span className={`sm-row__label ${row.struck ? "is-struck" : ""}`}>
                {row.label === "" ? "بی‌نام" : row.label}
              </span>
              <span className={`sm-row__amount num ${row.struck ? "is-struck" : ""}`}>
                {fmtAmount(row.amount)}
              </span>
            </button>
          ))}

          <button className="sm-add" onClick={() => setEditing({ index: null })}>
            <Icon name="plus" size={18} />
            <span>ردیف جدید</span>
          </button>
        </Card>

        {draft.rows.length > 0 && (
          <>
            <div className="sm-box">
              <div className="sm-box__label">جمع</div>
              <div className="sm-box__num num">{fmtAmount(total)}</div>
              <div className="sm-box__unit">{currencyLabel()}</div>
            </div>
            {struck > 0 && <div className="sm-note">{fa(struck)} ردیف خط خورده</div>}
          </>
        )}
      </Body>

      <div className="sm-foot">
        <Btn wide busy={busy} disabled={draft.rows.length === 0} onClick={() => void save()}>
          ذخیره
        </Btn>
      </div>

      {editing && (
        <RowSheet
          initial={index === null ? null : draft.rows[index] ?? null}
          onClose={() => setEditing(null)}
          onPut={(row) => put(index, row)}
        />
      )}
    </Screen>
  );
}

function RowSheet({
  initial,
  onClose,
  onPut,
}: {
  initial: DraftRow | null;
  onClose: () => void;
  onPut: (row: { label: string; amount: number; struck: boolean }) => void;
}) {
  const [label, setLabel] = useState(initial ? initial.label : "");
  const [typed, setTyped] = useState(initial && initial.amount > 0 ? toInput(initial.amount) : "");
  // The pad collects digits in whichever unit the shop uses, so they come back
  // through parseAmount. Treating them as toman directly is off by ten the
  // moment he chooses ریال.
  const amount = typed === "" ? 0 : (parseAmount(typed) ?? 0);
  const struck = initial ? initial.struck : false;
  const existing = initial !== null;

  const key = (k: string) => {
    if (k === "⌫") {
      setTyped((t) => t.slice(0, -1));
      return;
    }
    if (k === "٫") return; // تومان is whole; the decimal key has nothing to write here.
    setTyped((t) => {
      const next = (t + latin(k)).replace(/^0+(?=\d)/, "");
      return next.length > 12 ? t : next;
    });
  };

  const commit = (turn: boolean) => {
    if (!existing) {
      if (label.trim() === "" && amount <= 0) return;
      onPut({ label: label.trim(), amount, struck: false });
      return;
    }
    onPut({ label: label.trim(), amount, struck: turn ? !struck : struck });
  };

  return (
    <Sheet open onClose={onClose} title={existing ? "این ردیف" : "ردیف جدید"}>
      <Field
        label="چه چیزی؟"
        value={label}
        onChange={setLabel}
        placeholder="مثلاً شنبه یا کرایه"
        onEnter={() => commit(false)}
      />

      <div className="sm-amount">
        <div className="sm-amount__label">مبلغ</div>
        <div className="sm-amount__num num">{fmtAmount(amount)}</div>
        <div className="sm-amount__unit">{currencyLabel()}</div>
      </div>

      <NumPad onKey={key} onSubmit={() => commit(false)} submitLabel={existing ? "ثبت" : "افزودن"} />

      {existing && (
        <div className="sm-strike">
          <Btn wide variant={struck ? "ghost" : "quiet"} onClick={() => commit(true)}>
            {struck ? "به جمع برگردان" : "از جمع خط بزن"}
          </Btn>
        </div>
      )}
    </Sheet>
  );
}

function toDraft(sheet: SumSheet): Draft {
  return {
    id: sheet.id,
    day: sheet.day,
    title: sheet.title ?? "",
    rows: sheet.rows.map(toDraftRow),
  };
}

function toDraftRow(row: SumRow): DraftRow {
  return {
    key: nextKey(),
    id: row.id,
    label: row.label,
    amount: row.amount,
    struck: row.struck,
  };
}
