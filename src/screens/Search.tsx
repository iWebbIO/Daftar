// جست‌وجو — one field for the whole book.
//
// He remembers a name, a spice, a number on a page. All three come back from the
// same box, grouped by what they are, and each line opens the screen it belongs
// to.

import { useEffect, useRef, useState } from "react";
import { api, errorText } from "../api";
import { fa, fmtAmount, latin } from "../format";
import { dayMonth, relativeDay } from "../jalali";
import { useNav } from "../nav";
import { Body, Card, Empty, Field, Head, Icon, Row, Screen } from "../ui/kit";
import type { DocRow, Note, SearchHit } from "../types";
import "./search.css";

type Kind = "party" | "invoice" | "product" | "note";

interface Hit {
  key: string;
  kind: Kind;
  id: number;
  title: string;
  sub: string;
  amount: number | null;
}

const GROUP: { kind: Kind; label: string }[] = [
  { kind: "party", label: "حساب‌ها" },
  { kind: "invoice", label: "فاکتورها" },
  { kind: "product", label: "کالاها" },
  { kind: "note", label: "یادداشت‌ها" },
];

export default function Search() {
  const nav = useNav();
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<Hit[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);

  const term = q.trim();

  useEffect(() => {
    if (term.length < 2) {
      setHits(null);
      setBusy(false);
      return;
    }
    const mine = ++seq.current;
    setBusy(true);
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const digits = amountDigits(term);
          const [found, notes, docs] = await Promise.all([
            api.search(term),
            api.notes({ q: term }),
            digits ? api.docs({ limit: 80 }) : Promise.resolve<DocRow[]>([]),
          ]);
          if (seq.current !== mine) return;
          setHits(collect(term, found, notes, docs, digits));
          setError(null);
        } catch (e) {
          if (seq.current === mine) setError(errorText(e));
        } finally {
          if (seq.current === mine) setBusy(false);
        }
      })();
    }, 260);
    return () => window.clearTimeout(timer);
  }, [term]);

  const open = (hit: Hit) => {
    switch (hit.kind) {
      case "party":
        nav.go({ name: "party", id: hit.id });
        return;
      case "invoice":
        nav.go({ name: "invoice", docId: hit.id });
        return;
      case "product":
        nav.go({ name: "products" });
        return;
      case "note":
        nav.go({ name: "notes" });
        return;
    }
  };

  return (
    <Screen className="search">
      <Head title="جست‌وجو" onBack={nav.back} />

      <Body>
        <div className="sr-box">
          <Field
            value={q}
            onChange={setQ}
            placeholder="نام، کالا، مبلغ، یادداشت…"
            suffix={<Icon name="search" size={19} />}
            autoFocus
          />
        </div>

        {error && <Empty title={error} />}

        {!error && term.length < 2 && (
          <>
            <Empty title="نام یا مبلغ را بنویس" />
            <div className="sr-kinds">نام · کالا · مبلغ · یادداشت</div>
          </>
        )}

        {!error && term.length >= 2 && busy && hits === null && (
          <div className="sr-wait">می‌گردم…</div>
        )}

        {!error && term.length >= 2 && hits !== null && hits.length === 0 && !busy && (
          <Empty title="چیزی پیدا نشد" />
        )}

        {!error &&
          hits !== null &&
          GROUP.map(({ kind, label }) => {
            const group = hits.filter((h) => h.kind === kind);
            if (group.length === 0) return null;
            return (
              <div key={kind}>
                <div className="section-head">
                  <span>{label}</span>
                  <span className="faint">{fa(group.length)}</span>
                </div>
                <Card tight>
                  {group.map((hit) => (
                    <Row
                      key={hit.key}
                      title={hit.title}
                      sub={hit.sub}
                      value={
                        hit.amount === null ? undefined : fmtAmount(Math.abs(hit.amount))
                      }
                      tone={
                        kind === "party" && hit.amount !== null
                          ? hit.amount > 0
                            ? "pos"
                            : hit.amount < 0
                              ? "neg"
                              : "plain"
                          : "plain"
                      }
                      onClick={() => open(hit)}
                    />
                  ))}
                </Card>
              </div>
            );
          })}
      </Body>
    </Screen>
  );
}

/** Every kind of answer, in one list. The screen groups them. */
function collect(
  term: string,
  found: SearchHit[],
  notes: Note[],
  docs: DocRow[],
  digits: string,
): Hit[] {
  const hits: Hit[] = [];
  const seen = new Set<string>();
  const keep = (hit: Hit) => {
    if (seen.has(hit.key)) return;
    seen.add(hit.key);
    hits.push(hit);
  };

  for (const h of found) {
    if (h.kind === "party") {
      keep({
        key: `party-${h.id}`,
        kind: "party",
        id: h.id,
        title: h.title,
        sub: balanceWord(h.amount),
        amount: h.amount,
      });
    } else if (h.kind === "product") {
      keep({
        key: `product-${h.id}`,
        kind: "product",
        id: h.id,
        title: h.title,
        sub: h.subtitle,
        amount: h.amount,
      });
    } else if (h.kind === "invoice") {
      keep({
        key: `invoice-${h.id}`,
        kind: "invoice",
        id: h.id,
        title: h.title,
        sub: h.subtitle,
        amount: h.amount,
      });
    } else if (h.kind === "note") {
      keep({
        key: `note-${h.id}`,
        kind: "note",
        id: h.id,
        title: h.title,
        sub: h.subtitle,
        amount: null,
      });
    }
  }

  // An amount arrives as digits with separators; the invoice is found by its total.
  if (digits !== "") {
    for (const row of docs) {
      if (!String(row.doc.total).includes(digits)) continue;
      keep({
        key: `invoice-${row.doc.id}`,
        kind: "invoice",
        id: row.doc.id,
        title: row.partyName,
        sub: `${row.doc.kind === "sale" ? "فاکتور" : "خرید"} ${fa(row.doc.number)} · ${dayMonth(
          row.doc.day,
        )}`,
        amount: row.doc.total,
      });
      if (hits.filter((h) => h.kind === "invoice").length >= 8) break;
    }
  }

  for (const note of notes) {
    if (!note.body.includes(term) && !(note.title ?? "").includes(term)) continue;
    keep({
      key: `note-${note.id}`,
      kind: "note",
      id: note.id,
      title: firstLine(note.body),
      sub: `${relativeDay(note.day)} · ${dayMonth(note.day)}`,
      amount: null,
    });
  }

  return hits;
}

function firstLine(body: string): string {
  const line = body.split("\n").find((l) => l.trim() !== "") ?? body;
  const text = line.trim();
  return text.length > 46 ? `${text.slice(0, 44)}…` : text;
}

function balanceWord(amount: number | null): string {
  if (amount === null) return "حساب";
  if (amount > 0) return "طلب شما";
  if (amount < 0) return "بدهی شما";
  return "حساب صفر";
}

/** A query made only of digits is a search for a total. */
function amountDigits(term: string): string {
  const text = latin(term).replace(/[\s٬,٫]/g, "");
  return /^\d{3,}$/.test(text) ? text : "";
}
