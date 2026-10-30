// حساب‌ها: the whole book of people, biggest balance first.
//
// The only question this page answers is "who owes me, and how much" — so the
// two totals sit at the top, the names are in one column, and every amount
// carries its word.

import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorText } from "../api";
import { fa, fmtAmount } from "../format";
import { relativeDay } from "../jalali";
import { useNav } from "../nav";
import type { PartyCard } from "../types";
import {
  Body,
  Btn,
  Card,
  Empty,
  Field,
  Head,
  Loading,
  Row,
  Screen,
  Segmented,
} from "../ui/kit";
import "./parties.css";

type Filter = "all" | "owes" | "iowe";

export default function Parties() {
  const nav = useNav();
  const [cards, setCards] = useState<PartyCard[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const [filter, setFilter] = useState<Filter>("all");

  const load = useCallback(async () => {
    try {
      setCards(await api.parties());
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // The two numbers are always the whole book, never the filtered part of it.
  const totals = useMemo(() => {
    let owesMe = 0;
    let iOwe = 0;
    for (const c of cards ?? []) {
      if (c.balance > 0) owesMe += c.balance;
      else iOwe -= c.balance;
    }
    return { owesMe, iOwe };
  }, [cards]);

  const shown = useMemo(() => {
    const needle = q.trim();
    return (cards ?? []).filter((c) => {
      if (needle && !c.party.name.includes(needle)) return false;
      if (filter === "owes") return c.balance > 0;
      if (filter === "iowe") return c.balance < 0;
      return true;
    });
  }, [cards, q, filter]);

  if (error) {
    return (
      <Screen>
        <Head title="حساب‌ها" />
        <Body>
          <Empty title={error} action={<Btn onClick={load}>دوباره</Btn>} />
        </Body>
      </Screen>
    );
  }

  if (!cards) {
    return (
      <Screen>
        <Head title="حساب‌ها" />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  return (
    <Screen>
      <Head title="حساب‌ها" sub={`${fa(cards.length)} حساب`} />

      <Body>
        <div className="ptot">
          <div className="ptot__cell">
            <span className="ptot__word">طلب شما</span>
            <span className="ptot__num num ptot__num--pos">{fmtAmount(totals.owesMe)}</span>
          </div>
          <div className="ptot__split" />
          <div className="ptot__cell">
            <span className="ptot__word">بدهی شما</span>
            <span className="ptot__num num ptot__num--neg">{fmtAmount(totals.iOwe)}</span>
          </div>
        </div>

        <Field value={q} onChange={setQ} placeholder="نام مشتری…" />

        <Segmented<Filter>
          options={[
            { value: "all", label: "همه" },
            { value: "owes", label: "طلبکاران" },
            { value: "iowe", label: "بدهکاران" },
          ]}
          value={filter}
          onChange={setFilter}
        />

        {shown.length > 0 ? (
          <Card tight>
            {shown.map((c) => (
              <Row
                key={c.party.id}
                title={c.party.name}
                sub={under(c)}
                value={c.balance === 0 ? undefined : fmtAmount(Math.abs(c.balance))}
                tone={c.balance > 0 ? "pos" : c.balance < 0 ? "neg" : "plain"}
                note={
                  <span
                    className={`pword ${
                      c.balance > 0 ? "pword--pos" : c.balance < 0 ? "pword--neg" : ""
                    }`}
                  >
                    {word(c.balance)}
                  </span>
                }
                onClick={() => nav.go({ name: "party", id: c.party.id })}
              />
            ))}
          </Card>
        ) : cards.length === 0 ? (
          <Empty
            title="هنوز کسی در دفتر نیست"
            hint="هنوز کسی نیست"
            action={<Btn onClick={() => nav.go({ name: "order", kind: "sale" })}>فاکتور جدید</Btn>}
          />
        ) : q.trim() !== "" ? (
          <Empty title="پیدا نشد" hint="نام را کوتاه‌تر بنویس." />
        ) : (
          <Empty
            title={filter === "owes" ? "همه تسویه کرده‌اند" : "بدهی چیزی نیست"}
            hint={filter === "owes" ? "کسی طلبی ندارد." : "به کسی بدهکار نیستی."}
          />
        )}
      </Body>
    </Screen>
  );
}

/** The quiet second line: when he was last here, and how much is still open. */
function under(c: PartyCard): string {
  const parts: string[] = [];
  if (c.lastDay !== null) parts.push(relativeDay(c.lastDay));
  if (c.openDocs > 0) {
    parts.push(`${fa(c.openDocs)} ${c.party.kind === "supplier" ? "خرید باز" : "فاکتور باز"}`);
  }
  // A comma, not a middot: a small dot between a word and a Persian digit
  // reads as a zero.
  return parts.join("، ");
}

function word(balance: number): string {
  if (balance > 0) return "طلب شما";
  if (balance < 0) return "بدهی شما";
  return "حساب صفر";
}
