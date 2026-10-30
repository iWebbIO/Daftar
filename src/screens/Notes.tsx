// یادداشت‌ها — the pages of the notebook that are just words.
//
// Newest first, a few lines of each, and one big page to write on when you open
// it. Nothing here should ever feel like a form.

import { useCallback, useEffect, useState } from "react";
import { api, errorText } from "../api";
import { fa } from "../format";
import { longDate, relativeDay, today, weekdayName } from "../jalali";
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
  Screen,
  Sheet,
} from "../ui/kit";
import type { Note } from "../types";
import "./notes.css";

export default function Notes() {
  const nav = useNav();
  const [notes, setNotes] = useState<Note[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Note | "new" | null>(null);

  const load = useCallback(async () => {
    try {
      const list = await api.notes({});
      setNotes([...list].sort((a, b) => b.day - a.day || b.updatedAt - a.updatedAt));
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const close = useCallback(() => setEditing(null), []);
  const afterSave = useCallback(() => {
    setEditing(null);
    void load();
  }, [load]);

  if (error && !notes) {
    return (
      <Screen className="notes">
        <Head title="یادداشت‌ها" onBack={nav.back} />
        <Body>
          <Empty title={error} action={<Btn onClick={load}>دوباره</Btn>} />
        </Body>
      </Screen>
    );
  }

  if (!notes) {
    return (
      <Screen className="notes">
        <Head title="یادداشت‌ها" onBack={nav.back} />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  return (
    <Screen className="notes">
      <Head
        title="یادداشت‌ها"
        onBack={nav.back}
        sub={notes.length > 0 ? `${fa(notes.length)} یادداشت` : "صفحه‌ای نیست"}
      />

      <Body>
        {notes.length === 0 ? (
          <Empty title="هنوز چیزی ننوشته‌ای" />
        ) : (
          notes.map((note) => <NoteCard key={note.id} note={note} onOpen={() => setEditing(note)} />)
        )}
      </Body>

      <div className="nt-foot">
        <Btn wide onClick={() => setEditing("new")}>
          یادداشت جدید
        </Btn>
      </div>

      {editing !== null && (
        <NoteEditor
          note={editing === "new" ? null : editing}
          onClose={close}
          onSaved={afterSave}
        />
      )}
    </Screen>
  );
}

function NoteCard({ note, onOpen }: { note: Note; onOpen: () => void }) {
  return (
    <Card onClick={onOpen} className="nt-card">
      <div className="nt-card__date">
        <span>{relativeDay(note.day)}</span>
        <span className="nt-card__weekday">{weekdayName(note.day)}</span>
        <Icon name="edit" size={14} />
      </div>
      <div className="nt-card__body">{note.body}</div>
    </Card>
  );
}

function NoteEditor({
  note,
  onClose,
  onSaved,
}: {
  note: Note | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { toast } = useApp();
  const day = note ? note.day : today();
  const [body, setBody] = useState(note ? note.body : "");
  const [busy, setBusy] = useState(false);

  const save = async () => {
    const text = body.trim();
    if (text === "" || busy) return;
    setBusy(true);
    try {
      await api.saveNote({
        id: note ? note.id : undefined,
        day,
        title: note ? note.title : null,
        body: text,
      });
      toast("ذخیره شد", "ok");
      onSaved();
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  const changed = body.trim() !== (note ? note.body.trim() : "");

  return (
    <Sheet
      open
      full
      onClose={onClose}
      title={note ? longDate(note.day) : "یادداشت جدید"}
    >
      <div className="nt-page">
        <div className="nt-page__day">
          {relativeDay(day)} · {weekdayName(day)}
        </div>
        <Field value={body} onChange={setBody} multiline rows={14} autoFocus />
      </div>

      <div className="nt-submit">
        <Btn wide busy={busy} disabled={!changed || body.trim() === ""} onClick={() => void save()}>
          ذخیره
        </Btn>
      </div>
    </Sheet>
  );
}
