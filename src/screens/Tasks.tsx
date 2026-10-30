// کارها — the things he must not forget, out of his head and onto paper.
//
// One list, undone at the top, done below and quieted. A tick is one tap; a new
// task is a name, a day and maybe a person, and nothing else.

import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorText } from "../api";
import { fa } from "../format";
import { relativeDay, today } from "../jalali";
import { useApp } from "../nav";
import {
  Body,
  Btn,
  Card,
  Empty,
  Field,
  Head,
  Icon,
  Loading,
  Pill,
  Row,
  Screen,
  Segmented,
  Sheet,
} from "../ui/kit";
import type { PartyCard, Task, TaskRow } from "../types";
import "./tasks.css";

export default function Tasks() {
  const { toast } = useApp();
  const [rows, setRows] = useState<TaskRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [busyId, setBusyId] = useState<number | null>(null);

  const load = useCallback(async () => {
    try {
      setRows(await api.tasks({ includeDone: true }));
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

  const undone = useMemo(() => (rows ?? []).filter((r) => !r.task.done).sort(byDay), [rows]);
  const done = useMemo(() => (rows ?? []).filter((r) => r.task.done).sort(byNewest), [rows]);

  const toggle = async (task: Task) => {
    setBusyId(task.id);
    try {
      await api.toggleTask(task.id);
      await load();
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusyId(null);
    }
  };

  if (error && !rows) {
    return (
      <Screen className="tasks">
        <Head title="کارها" />
        <Body>
          <Empty title={error} action={<Btn onClick={load}>دوباره</Btn>} />
        </Body>
        <div className="tk-foot">
          <Btn wide onClick={() => setAdding(true)}>
            کار تازه
          </Btn>
        </div>
        <AddTask open={adding} onClose={() => setAdding(false)} onSaved={afterSave} />
      </Screen>
    );
  }

  if (!rows) {
    return (
      <Screen className="tasks">
        <Head title="کارها" />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  const nothingLeft = undone.length === 0 && done.length === 0;

  return (
    <Screen className="tasks">
      <Head
        title="کارها"
        sub={
          undone.length > 0
            ? `${fa(undone.length)} کار مانده`
            : done.length > 0
              ? "همه انجام شده"
              : "کاری نیست"
        }
      />

      <Body>
        {nothingLeft ? (
          <Empty
            title="هنوز کاری ننوشته‌ای"
            hint="هر چیزی که باید یادت بماند را بنویس؛ بعد یک ضرب خطش بزن."
          />
        ) : undone.length > 0 ? (
          <Card tight>
            {undone.map(({ task, partyName }) => (
              <Row
                key={task.id}
                leading={
                  <Tick
                    done={false}
                    disabled={busyId === task.id}
                    label={task.text}
                    onTap={() => void toggle(task)}
                  />
                }
                title={task.text}
                sub={taskSub(task, partyName)}
              />
            ))}
          </Card>
        ) : (
          <div className="tk-clear">
            <Icon name="check" size={18} />
            <span>کاری نمانده. دستت درد نکنه.</span>
          </div>
        )}

        {done.length > 0 && (
          <>
            <div className="section-head">
              <span>انجام‌شده</span>
              <span className="faint">{fa(done.length)}</span>
            </div>
            <div className="tk-done">
              <Card tight>
                {done.map(({ task, partyName }) => (
                  <Row
                    key={task.id}
                    struck
                    leading={
                      <Tick
                        done
                        disabled={busyId === task.id}
                        label={task.text}
                        onTap={() => void toggle(task)}
                      />
                    }
                    title={task.text}
                    sub={taskSub(task, partyName)}
                  />
                ))}
              </Card>
            </div>
          </>
        )}
      </Body>

      <div className="tk-foot">
        <Btn wide onClick={() => setAdding(true)}>
          کار تازه
        </Btn>
      </div>

      <AddTask open={adding} onClose={() => setAdding(false)} onSaved={afterSave} />
    </Screen>
  );
}

function byDay(a: TaskRow, b: TaskRow): number {
  const da = a.task.day ?? Number.MAX_SAFE_INTEGER;
  const db = b.task.day ?? Number.MAX_SAFE_INTEGER;
  return da - db || a.task.id - b.task.id;
}

function byNewest(a: TaskRow, b: TaskRow): number {
  return b.task.id - a.task.id;
}

/** «امروز · حسین رضایی» — the two things he needs beside the words. */
function taskSub(task: Task, partyName: string | null): string | undefined {
  const bits: string[] = [];
  if (task.day !== null) bits.push(relativeDay(task.day));
  if (partyName) bits.push(partyName);
  return bits.length > 0 ? bits.join(" · ") : undefined;
}

/** A box big enough to hit without looking, with a line through the words instead. */
function Tick({
  done,
  disabled,
  label,
  onTap,
}: {
  done: boolean;
  disabled: boolean;
  label: string;
  onTap: () => void;
}) {
  return (
    <button
      className={`tk-tick ${done ? "is-on" : ""}`}
      onClick={onTap}
      disabled={disabled}
      aria-label={done ? `برگردان: ${label}` : `انجام شد: ${label}`}
    >
      <span className="tk-tick__box">
        <Icon name="check" size={17} />
      </span>
    </button>
  );
}

function AddTask({
  open,
  onClose,
  onSaved,
}: {
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { toast } = useApp();
  const [text, setText] = useState("");
  const [offset, setOffset] = useState(0);
  const [q, setQ] = useState("");
  const [people, setPeople] = useState<PartyCard[]>([]);
  const [party, setParty] = useState<PartyCard | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setText("");
    setOffset(0);
    setQ("");
    setParty(null);
  }, [open]);

  // Names he has dealt with lately, narrowed by whatever he types.
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

  const save = async () => {
    const words = text.trim();
    if (words === "" || busy) return;
    setBusy(true);
    try {
      await api.saveTask({
        text: words,
        day: offset < 0 ? null : today() + offset,
        partyId: party ? party.party.id : null,
      });
      toast("ثبت شد", "ok");
      onSaved();
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Sheet open={open} onClose={onClose} title="کار تازه">
      <Field
        label="چه کاری؟"
        value={text}
        onChange={setText}
        placeholder="مثلاً به حسین بار بده"
        autoFocus
        onEnter={() => void save()}
      />

      <div className="tk-label">چه روزی؟</div>
      <Segmented
        options={[
          { value: 0, label: "امروز" },
          { value: 1, label: "فردا" },
          { value: -1, label: "بعداً" },
        ]}
        value={offset}
        onChange={setOffset}
      />

      <div className="tk-label">برای کی؟ (اختیاری)</div>
      {party ? (
        <div className="tk-picked">
          <Pill tone="brand">{party.party.name}</Pill>
          <button className="tk-link" onClick={() => setParty(null)}>
            برداشتن
          </button>
        </div>
      ) : (
        <>
          <Field value={q} onChange={setQ} placeholder="نام را بنویس" />
          {people.length > 0 && (
            <div className="tk-picks">
              {people.map((p) => (
                <button key={p.party.id} className="tk-pick" onClick={() => setParty(p)}>
                  {p.party.name}
                </button>
              ))}
            </div>
          )}
        </>
      )}

      <Btn wide busy={busy} disabled={text.trim() === ""} onClick={() => void save()}>
        افزودن
      </Btn>
    </Sheet>
  );
}
