// کالاها — the catalogue, with the price he last sold each thing at.
//
// The point is the price memory: زردچوبه was ۶۴۰٬۰۰۰ and is now ۷۲۰٬۰۰۰. Tap a
// product and the last few prices are there, each with the day it changed.

import { useCallback, useEffect, useState } from "react";
import { api, errorText } from "../api";
import { currencyLabel, fa, fmtAmount, parseAmount, toInput, UNITS } from "../format";
import { dayMonth } from "../jalali";
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
  Money,
  Row,
  Screen,
  Sheet,
} from "../ui/kit";
import type { PricePoint, Product } from "../types";
import "./products.css";

export default function Products() {
  const nav = useNav();
  const { toast } = useApp();
  const [list, setList] = useState<Product[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const [history, setHistory] = useState<Product | null>(null);
  const [editing, setEditing] = useState<Product | "new" | null>(null);

  const load = useCallback(async (term: string) => {
    try {
      setList(await api.products(term === "" ? undefined : term));
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void load(q.trim());
    }, 200);
    return () => window.clearTimeout(timer);
  }, [q, load]);

  const afterSave = useCallback(() => {
    setEditing(null);
    void load(q.trim());
    toast("ذخیره شد", "ok");
  }, [load, q, toast]);

  if (error && !list) {
    return (
      <Screen className="products">
        <Head title="کالاها" onBack={nav.back} />
        <Body>
          <Empty title={error} action={<Btn onClick={() => void load(q.trim())}>دوباره</Btn>} />
        </Body>
      </Screen>
    );
  }

  if (!list) {
    return (
      <Screen className="products">
        <Head title="کالاها" onBack={nav.back} />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  return (
    <Screen className="products">
      <Head
        title="کالاها"
        onBack={nav.back}
        sub={list.length > 0 ? `${fa(list.length)} کالا` : "کالایی نیست"}
      />

      <Body>
        <Field
          value={q}
          onChange={setQ}
          placeholder="جست‌وجوی کالا"
          suffix={<Icon name="search" size={18} />}
        />

        {list.length === 0 ? (
          <Empty title={q.trim() !== "" ? "کالایی با این نام نیست" : "کالایی نیست"} />
        ) : (
          <Card tight>
            {list.map((product) => (
              <Row
                key={product.id}
                title={
                  <>
                    {product.name}
                    <span className="pr-u">{product.unit}</span>
                  </>
                }
                sub={`خرید ${fmtAmount(product.buyPrice)}`}
                value={<span className="pr-sale">{fmtAmount(product.salePrice)}</span>}
                note="فروش"
                onClick={() => setHistory(product)}
              />
            ))}
          </Card>
        )}
      </Body>

      <div className="pr-foot">
        <Btn wide onClick={() => setEditing("new")}>
          کالای جدید
        </Btn>
      </div>

      {history && (
        <HistorySheet
          product={history}
          onClose={() => setHistory(null)}
          onEdit={() => {
            setEditing(history);
            setHistory(null);
          }}
        />
      )}

      {editing !== null && (
        <ProductSheet
          product={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={afterSave}
        />
      )}
    </Screen>
  );
}

function HistorySheet({
  product,
  onClose,
  onEdit,
}: {
  product: Product;
  onClose: () => void;
  onEdit: () => void;
}) {
  const [points, setPoints] = useState<PricePoint[] | null>(null);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const rows = await api.priceHistory(product.id);
        if (live) setPoints(collapse(rows));
      } catch {
        if (live) setPoints([]);
      }
    })();
    return () => {
      live = false;
    };
  }, [product.id]);

  const newest = points && points.length > 0 ? points[0] : null;
  const previous = points && points.length > 1 ? points[1] : null;
  const change =
    newest && previous && previous.price > 0
      ? Math.round(((newest.price - previous.price) / previous.price) * 100)
      : 0;

  return (
    <Sheet open onClose={onClose} title={product.name}>
      <div className="pr-now">
        <div className="pr-now__label">فروش</div>
        <div className="pr-now__money">
          <Money value={product.salePrice} size="xl" unit />
        </div>
        <div className="pr-now__unit">{product.unit}</div>
      </div>

      {points === null ? (
        <Loading text="قیمت‌ها…" />
      ) : points.length === 0 ? (
        <div className="pr-quiet">قیمتی نیست</div>
      ) : (
        <>
          {previous && (
            <div className="pr-change">
              <div className="pr-change__nums">
                از {fmtAmount(previous.price)}
                <span className="pr-change__day">{dayMonth(previous.day)}</span>
              </div>
              <div className="pr-change__word">{changeWord(change)}</div>
            </div>
          )}
          <Card tight>
            {points.slice(0, 6).map((point) => (
              <div className="pr-point" key={`${point.day}-${point.price}`}>
                <span className="pr-point__day">{dayMonth(point.day)}</span>
                <span className="pr-point__price num">{fmtAmount(point.price)}</span>
              </div>
            ))}
          </Card>
        </>
      )}

      <div className="pr-sheet-foot">
        <Btn wide variant="ghost" onClick={onEdit}>
          ویرایش کالا
        </Btn>
      </div>
    </Sheet>
  );
}

function ProductSheet({
  product,
  onClose,
  onSaved,
}: {
  product: Product | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { toast } = useApp();
  const [name, setName] = useState(product ? product.name : "");
  const [unit, setUnit] = useState(product ? product.unit : UNITS[0]!);
  // The fields show the price as he reads it — rial is ten to the toman — so
  // saving one he never touched cannot move it by a factor of ten.
  const [sale, setSale] = useState(product ? toInput(product.salePrice) : "");
  const [buy, setBuy] = useState(product ? toInput(product.buyPrice) : "");
  const [busy, setBusy] = useState(false);

  const custom = UNITS.includes(unit) ? "" : unit;
  const saleValue = parseAmount(sale);

  const save = async () => {
    if (busy) return;
    const words = name.trim();
    if (words === "") {
      toast("نام کالا را بنویس", "bad");
      return;
    }
    if (saleValue === null || saleValue < 0) {
      toast("قیمت فروش را درست بنویس", "bad");
      return;
    }
    setBusy(true);
    try {
      await api.saveProduct({
        id: product ? product.id : undefined,
        name: words,
        unit: unit.trim() === "" ? "کیلو" : unit.trim(),
        salePrice: saleValue,
        buyPrice: parseAmount(buy) ?? 0,
        active: product ? product.active : true,
      });
      onSaved();
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Sheet open onClose={onClose} title={product ? "ویرایش کالا" : "کالای جدید"}>
      <Field
        label="نام کالا"
        value={name}
        onChange={setName}
        placeholder="مثلاً زردچوبه"
        autoFocus
      />

      <div className="pr-label">واحد</div>
      <div className="pr-units">
        {UNITS.map((u) => (
          <button
            key={u}
            className={`pr-unit ${unit === u ? "is-on" : ""}`}
            onClick={() => setUnit(u)}
          >
            {u}
          </button>
        ))}
      </div>
      <Field
        value={custom}
        onChange={(v) => setUnit(v === "" ? UNITS[0]! : v)}
        placeholder="واحد دیگر"
      />

      <Field
        label="قیمت فروش"
        value={sale}
        onChange={setSale}
        inputMode="numeric"
        placeholder="۰"
        suffix={currencyLabel()}
      />
      {saleValue !== null && saleValue > 0 && (
        <div className="pr-echo num">{fmtAmount(saleValue)} {currencyLabel()}</div>
      )}

      <Field
        label="قیمت خرید"
        value={buy}
        onChange={setBuy}
        inputMode="numeric"
        placeholder="۰"
        suffix={currencyLabel()}
        onEnter={() => void save()}
      />

      <Btn wide busy={busy} onClick={() => void save()}>
        ذخیره
      </Btn>
    </Sheet>
  );
}

function changeWord(pct: number): string {
  if (pct === 0) return "تغییر نکرده";
  return pct > 0 ? `${fa(pct)}٪ بالا رفته` : `${fa(Math.abs(pct))}٪ پایین آمده`;
}

/**
 * Newest first, with runs of the same price folded into one row: what he wants
 * from this page is the days the price *moved*, not eight rows saying ۷۲۰٬۰۰۰.
 */
function collapse(rows: PricePoint[]): PricePoint[] {
  const sorted = [...rows].sort((a, b) => b.day - a.day);
  const out: PricePoint[] = [];
  for (const point of sorted) {
    const last = out[out.length - 1];
    if (last && last.price === point.price) continue;
    out.push(point);
  }
  return out;
}
