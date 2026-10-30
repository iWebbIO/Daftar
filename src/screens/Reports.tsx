// گزارش — three questions, answered in the order he asks them.
//
// No charts. A number he can read from the doorway, and a thin bar where a
// comparison actually helps.

import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorText } from "../api";
import { fa, fmtAmount, fmtQty } from "../format";
import { d2j, j2d, longDate, MONTHS, today, weekday } from "../jalali";
import { useNav } from "../nav";
import { Body, Card, Empty, Btn, Head, Loading, Money, Screen, Segmented } from "../ui/kit";
import type { AgeBucket, ProductReport, ReportDays } from "../types";
import "./reports.css";

type Period = "week" | "month" | "season";

const PERIODS: { value: Period; label: string }[] = [
  { value: "week", label: "این هفته" },
  { value: "month", label: "این ماه" },
  { value: "season", label: "این فصل" },
];

export default function Reports() {
  const nav = useNav();
  const [period, setPeriod] = useState<Period>("week");
  const [days, setDays] = useState<ReportDays[] | null>(null);
  const [products, setProducts] = useState<ProductReport[] | null>(null);
  const [ageing, setAgeing] = useState<AgeBucket[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const range = useMemo(() => span(period), [period]);

  const load = useCallback(async () => {
    try {
      const [d, p] = await Promise.all([
        api.reportDays(range.from, range.to),
        api.reportProducts(range.from, range.to),
      ]);
      setDays(d);
      setProducts(p);
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, [range]);

  const loadAgeing = useCallback(async () => {
    try {
      setAgeing(await api.reportAgeing());
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    void loadAgeing();
  }, [loadAgeing]);

  if (error) {
    return (
      <Screen className="reports">
        <Head title="گزارش" onBack={nav.back} />
        <Body>
          <Empty title={error} action={<Btn onClick={() => { void load(); void loadAgeing(); }}>دوباره</Btn>} />
        </Body>
      </Screen>
    );
  }

  if (!days || !products || !ageing) {
    return (
      <Screen className="reports">
        <Head title="گزارش" onBack={nav.back} />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  const sum = (pick: (d: ReportDays) => number) => days.reduce((n, d) => n + pick(d), 0);
  const sold = sum((d) => d.sold);
  const bought = sum((d) => d.bought);
  const received = sum((d) => d.received);
  const paid = sum((d) => d.paid);
  const count = sum((d) => d.count);

  const owed = ageing.reduce((n, b) => n + b.amount, 0);
  const debtors = ageing.reduce((n, b) => n + b.count, 0);
  const top = [...products].sort((a, b) => b.revenue - a.revenue).slice(0, 10);
  const best = top.length > 0 ? top[0]!.revenue : 0;

  return (
    <Screen className="reports">
      <Head title="گزارش" onBack={nav.back} sub={spoken(range)} />

      <Body>
        <Segmented options={PERIODS} value={period} onChange={setPeriod} />

        <div className="rp-q">چقدر فروختم؟</div>
        <Card className="rp-hero">
          <div className="rp-hero__label">فروش</div>
          <div className="rp-hero__money">
            <Money value={sold} size="xl" unit />
          </div>
          <div className="rp-lines">
            <Line label="خرید" value={bought} tone="neg" />
            <Line label="دریافت" value={received} tone="pos" />
            <Line label="پرداخت" value={paid} tone="neg" />
          </div>
        </Card>
        <div className="rp-note">
          {count > 0 ? `${fa(count)} برگه در این بازه` : "در این بازه برگی ثبت نشده"}
        </div>

        <div className="rp-q">کی پول من را نداده؟</div>
        <Card tight>
          {owed === 0 ? (
            <div className="rp-quiet">همه حساب‌ها صفر است. کسی به تو بدهی ندارد.</div>
          ) : (
            ageing.map((bucket) => (
              <div className={`rp-age ${bucket.amount === 0 ? "is-zero" : ""}`} key={bucket.label}>
                <div className="rp-age__main">
                  <div className="rp-age__label">{bucket.label}</div>
                  <div className="rp-age__count">{fa(bucket.count)} فاکتور</div>
                  <div className="rp-age__track">
                    <span
                      className="rp-age__fill"
                      style={{ width: `${share(bucket.amount, owed)}%` }}
                    />
                  </div>
                </div>
                <div className="rp-age__money num">{fmtAmount(bucket.amount)}</div>
              </div>
            ))
          )}
        </Card>
        {owed > 0 && (
          <div className="rp-total">
            <span>جمع طلب از {fa(debtors)} فاکتور</span>
            <span className="num">{fmtAmount(owed)}</span>
          </div>
        )}

        <div className="rp-q">چه چیزی بیشتر فروش داشته؟</div>
        {top.length === 0 ? (
          <Card>
            <div className="rp-quiet">در این بازه چیزی فروخته نشده.</div>
          </Card>
        ) : (
          <Card tight>
            {top.map((p, i) => (
              <div className="rp-prod" key={`${p.name}-${i}`}>
                <span className="rp-prod__rank num">{fa(i + 1)}</span>
                <div className="rp-prod__main">
                  <div className="rp-prod__name">{p.name}</div>
                  <div className="rp-prod__qty">
                    <span className="num">{fmtQty(p.qtyMilli)}</span> {p.unit}
                  </div>
                  <div className="rp-prod__track">
                    <span
                      className="rp-prod__fill"
                      style={{ width: `${share(p.revenue, best)}%` }}
                    />
                  </div>
                </div>
                <div className="rp-prod__money num">{fmtAmount(p.revenue)}</div>
              </div>
            ))}
          </Card>
        )}
        {products.length > top.length && (
          <div className="rp-note">فقط ۱۰ کالای اول را می‌بینی.</div>
        )}
      </Body>
    </Screen>
  );
}

function Line({ label, value, tone }: { label: string; value: number; tone: "pos" | "neg" }) {
  return (
    <div className={`rp-line ${value === 0 ? "is-zero" : ""}`}>
      <span className="rp-line__label">{label}</span>
      <Money value={value} size="md" tone={value === 0 ? "plain" : tone} />
    </div>
  );
}

/** The days the chosen period covers. */
function span(period: Period): { from: number; to: number } {
  const to = today();
  if (period === "week") return { from: to - weekday(to), to };
  const j = d2j(to);
  if (period === "month") return { from: j2d(j.year, j.month, 1), to };
  const first = Math.floor((j.month - 1) / 3) * 3 + 1;
  return { from: j2d(j.year, first, 1), to };
}

/** «از ۲ تا ۸ آبان ۱۴۰۵» — a period said the way he would say it. */
function spoken(range: { from: number; to: number }): string {
  if (range.from === range.to) return longDate(range.to);
  const from = d2j(range.from);
  const to = d2j(range.to);
  if (from.month === to.month && from.year === to.year) {
    return `از ${fa(from.day)} تا ${longDate(range.to)}`;
  }
  if (from.year === to.year) {
    return `از ${fa(from.day)} ${MONTHS[from.month - 1]} تا ${longDate(range.to)}`;
  }
  return `از ${longDate(range.from)} تا ${longDate(range.to)}`;
}

const share = (part: number, whole: number): number =>
  part <= 0 || whole <= 0 ? 0 : Math.max(2, Math.round((part / whole) * 100));
