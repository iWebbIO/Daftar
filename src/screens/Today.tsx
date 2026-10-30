// Today: the page he opens. Who owes him, what happened today, what is left to do.

import { useCallback, useEffect, useState } from "react";
import { api, errorText } from "../api";
import { fmtAmount } from "../format";
import { longDate, weekdayName } from "../jalali";
import { useApp, useNav } from "../nav";
import { Body, Btn, Card, Empty, Head, Icon, Loading, Money, Pill, Row, Screen } from "../ui/kit";
import type { TodayView } from "../types";
import "./today.css";

export default function Today() {
  const nav = useNav();
  const { toast, settings, update } = useApp();
  const [view, setView] = useState<TodayView | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setView(await api.today());
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (error) {
    return (
      <Screen>
        <Head title="امروز" />
        <Body>
          <Empty title={error} action={<Btn onClick={load}>دوباره</Btn>} />
        </Body>
      </Screen>
    );
  }

  if (!view) {
    return (
      <Screen>
        <Head title="امروز" />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  return (
    <Screen>
      <Head
        big
        title={longDate(view.day)}
        sub={weekdayName(view.day)}
        right={
          <button className="head__icon" onClick={() => nav.go({ name: "search" })} aria-label="جست‌وجو">
            <Icon name="search" size={21} />
          </button>
        }
      />

      <Body>
        {/* The bot lives three taps away in settings; say it once here, where he
            actually starts his day. */}
        {!settings?.chatId && (
          <button className="connectbot" onClick={() => nav.go({ name: "settings" })}>
            <Icon name="send" size={17} />
            <span>به تلگرام وصل نیست</span>
          </button>
        )}

        {update?.behind && (
          <button className="connectbot" onClick={() => nav.go({ name: "settings" })}>
            <Icon name="today" size={17} />
            <span>نسخه جدید هست</span>
          </button>
        )}

        {/* The two numbers the whole book exists for. */}
        <button className="debts" onClick={() => nav.replace({ name: "parties" })}>
          <div className="debts__side debts__side--pos">
            <div className="debts__label">طلب شما</div>
            <div className="debts__num num">{fmtAmount(view.totals.owesMe)}</div>
          </div>
          <div className="debts__split" />
          <div className="debts__side debts__side--neg">
            <div className="debts__label">بدهی شما</div>
            <div className="debts__num num">{fmtAmount(view.totals.iOwe)}</div>
          </div>
        </button>

        <div className="quick">
          <QuickTile icon="book" label="فاکتور جدید" onClick={() => nav.go({ name: "order", kind: "sale" })} />
          <QuickTile icon="check" label="پول گرفتم" onClick={() => nav.go({ name: "payment" })} />
          <QuickTile icon="send" label="خرید" onClick={() => nav.go({ name: "order", kind: "purchase" })} />
        </div>

        <div className="today-sum">
          <SumCell label="فروش امروز" value={view.sold} tone="plain" />
          <SumCell label="دریافت امروز" value={view.received} tone="pos" />
        </div>

        {view.docs.length > 0 && (
          <>
            <div className="section-head">
              <span>امروز</span>
              <span className="faint">{view.docs.length} برگه</span>
            </div>
            <Card tight>
              {view.docs.map(({ doc, partyName }) => (
                <Row
                  key={`${doc.kind}-${doc.id}`}
                  title={partyName}
                  sub={
                    <span>
                      {doc.kind === "sale" ? `فاکتور ${doc.number}` : `خرید ${doc.number}`}
                      {doc.remaining > 0 && doc.paid > 0 ? " · مانده دارد" : ""}
                    </span>
                  }
                  value={fmtAmount(doc.total)}
                  tone="plain"
                  struck={doc.status === "settled"}
                  note={doc.remaining > 0 ? `مانده ${fmtAmount(doc.remaining)}` : undefined}
                  onClick={() => nav.go({ name: "invoice", docId: doc.id })}
                />
              ))}
            </Card>
          </>
        )}

        {view.tasks.length > 0 && (
          <>
            <div className="section-head">
              <span>کارها</span>
              <button className="linkbtn" onClick={() => nav.replace({ name: "tasks" })}>
                همه
              </button>
            </div>
            <Card tight>
              {view.tasks.slice(0, 4).map(({ task, partyName }) => (
                <Row
                  key={task.id}
                  leading={
                    <span
                      className="tick"
                      role="button"
                      onClick={async (e) => {
                        e.stopPropagation();
                        try {
                          await api.toggleTask(task.id);
                          void load();
                        } catch (err) {
                          toast(errorText(err), "bad");
                        }
                      }}
                    >
                      <Icon name="check" size={15} />
                    </span>
                  }
                  title={task.text}
                  sub={partyName ?? undefined}
                />
              ))}
            </Card>
          </>
        )}

        {view.cheques.length > 0 && (
          <>
            <div className="section-head">
              <span>چک‌ها</span>
            </div>
            <Card tight>
              {view.cheques.map(({ cheque, partyName }) => (
                <Row
                  key={cheque.id}
                  title={partyName}
                  sub={`${cheque.bank ?? "چک"} · ${longDate(cheque.dueDay)}`}
                  value={fmtAmount(cheque.amount)}
                  tone={cheque.direction === "in" ? "pos" : "neg"}
                  note={cheque.direction === "in" ? "دریافتنی" : "پرداختنی"}
                />
              ))}
            </Card>
          </>
        )}

        {view.docs.length === 0 && view.tasks.length === 0 && (
          <Empty
            title="امروز هنوز چیزی ثبت نشده"
            hint="با دکمه پایین یک فاکتور بساز."
            action={
              <Btn onClick={() => nav.go({ name: "order", kind: "sale" })}>فاکتور جدید</Btn>
            }
          />
        )}

        {view.lastBackupAt && <BackupNote at={view.lastBackupAt} />}
      </Body>
    </Screen>
  );
}

function QuickTile({
  icon,
  label,
  onClick,
}: {
  icon: string;
  label: string;
  onClick: () => void;
}) {
  return (
    <button className="quick__tile" onClick={onClick}>
      <Icon name={icon} size={22} />
      <span>{label}</span>
    </button>
  );
}

function SumCell({
  label,
  value,
  tone,
}: {
  label: string;
  value: number;
  tone: "plain" | "pos" | "neg";
}) {
  return (
    <div className="sumcell">
      <div className="sumcell__label">{label}</div>
      <Money value={value} tone={tone} size="lg" />
    </div>
  );
}

function BackupNote({ at }: { at: number }) {
  const hours = Math.max(0, Math.round((Date.now() - at) / 36e5));
  const fresh = hours < 30;
  return (
    <div className={`backupnote ${fresh ? "" : "backupnote--stale"}`}>
      <Pill tone={fresh ? "pos" : "warn"}>{fresh ? "پشتیبان گرفته شد" : "پشتیبان قدیمی است"}</Pill>
      <span className="faint tiny">
        {hours < 1 ? "همین حالا" : `${fa(hours)} ساعت پیش`}
      </span>
    </div>
  );
}

const fa = (n: number) => String(n).replace(/[0-9]/g, (d) => "۰۱۲۳۴۵۶۷۸۹"[Number(d)]!);
