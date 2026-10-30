// One invoice: the exact page that goes to the customer, and the same facts in
// plain rows underneath, because a picture can always fail and a shopkeeper
// still has to be able to read his own paper.

import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api, errorText } from "../api";
import { fa, fmtAmount, fmtQty, lineTotal } from "../format";
import { longDate } from "../jalali";
import { useApp, useNav } from "../nav";
import type { DocFull } from "../types";
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
  Sheet,
} from "../ui/kit";
import "./invoice.css";

export default function Invoice(props: { docId: number }) {
  const nav = useNav();
  const { toast, settings } = useApp();
  const [full, setFull] = useState<DocFull | null>(null);
  const [svg, setSvg] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [choosing, setChoosing] = useState(false);
  const [which, setWhich] = useState<"tg" | "share" | null>(null);
  const [voiding, setVoiding] = useState(false);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);

  const id = props.docId;

  useEffect(() => {
    void (async () => {
      try {
        setFull(await api.doc(id));
        setError(null);
      } catch (e) {
        setError(errorText(e));
      }
      try {
        const page = await api.invoiceSvg(id);
        setSvg(page.svg ?? "");
      } catch {
        // the readable version underneath is the fallback, not a failure
      }
    })();
  }, [id]);

  const lines = useMemo(() => (full?.lines ?? []).filter((l) => !l.voided), [full]);
  const sum = useMemo(
    () => lines.reduce((n, l) => n + (l.overrideTotal ?? lineTotal(l.qtyMilli, l.price)), 0),
    [lines],
  );

  // The paper goes out two ways, and which one is his call: through the bot he
  // already has, or through whatever the phone itself offers.
  const sendTelegram = async () => {
    if (which !== null) return;
    setWhich("tg");
    try {
      const answer = await api.invoiceSend(id);
      toast(answer.message, answer.ok ? "ok" : "bad");
      setChoosing(false);
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setWhich(null);
    }
  };

  const shareIt = async () => {
    if (which !== null) return;
    setWhich("share");
    setChoosing(false);
    let text: string;
    try {
      text = (await api.invoiceText(id)).text;
    } catch {
      toast("متن فاکتور آماده نشد", "bad");
      setWhich(null);
      return;
    }
    try {
      await invoke("plugin:share|share", { text });
    } catch {
      // No share sheet here: the words are still worth having on the clipboard.
      try {
        await navigator.clipboard.writeText(text);
        toast("متن فاکتور کپی شد؛ در پیام‌رسان بچسبان", "ok");
      } catch {
        toast("اشتراک‌گذاری روی این گوشی نیست", "bad");
      }
    }
    setWhich(null);
  };

  const doVoid = async () => {
    setBusy(true);
    try {
      await api.voidDoc(id, reason.trim());
      setVoiding(false);
      setReason("");
      toast("فاکتور خط خورد", "ok");
      setFull(await api.doc(id));
      setSvg("");
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  if (error) {
    return (
      <Screen>
        <Head title="فاکتور" onBack={nav.back} />
        <Body>
          <Empty title={error} />
        </Body>
      </Screen>
    );
  }

  if (!full) {
    return (
      <Screen>
        <Head title="فاکتور" onBack={nav.back} />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  const { doc, party } = full;
  const purchase = doc.kind === "purchase";
  const title = purchase ? `خرید ${fa(doc.number)}` : `فاکتور ${fa(doc.number)}`;
  const voided = doc.status === "void";

  return (
    <Screen>
      <Head
        title={title}
        sub={party.name}
        onBack={nav.back}
        right={
          voided ? (
            <Pill tone="neg">خط خورده</Pill>
          ) : doc.status === "settled" ? (
            <Pill tone="pos">تسویه شد</Pill>
          ) : (
            <Pill tone="warn">مانده دارد</Pill>
          )
        }
      />

      <Body>
        {voided && (
          <div className="inv-void">
            <Icon name="close" size={17} />
            <span>خط خورده{doc.voidReason ? ` — ${doc.voidReason}` : ""}</span>
          </div>
        )}

        {/* the page itself, when the app can draw it */}
        {svg !== "" && (
          <img
            className="inv-svg"
            alt={title}
            src={`data:image/svg+xml;utf8,${encodeURIComponent(svg)}`}
            onError={() => setSvg("")}
          />
        )}

        {/* the same paper, in rows */}
        <Card className={`inv-paper ${voided ? "is-void" : ""}`}>
          {svg === "" && (
            <>
              <div className="inv-paper__top">
                <span className="inv-paper__no">{title}</span>
                <span className="inv-paper__shop">{settings?.shopName || "دفتر"}</span>
              </div>
              <div className="inv-paper__meta">
                <span className="inv-paper__to">{party.name}</span>
                <span>{longDate(doc.day)}</span>
              </div>
            </>
          )}

          <div className="inv-paper__lines">
            {lines.length > 0 ? (
              lines.map((l) => (
                <div className="inv-line" key={l.id}>
                  <span className="inv-line__name">{l.name}</span>
                  <span className="inv-line__calc num">
                    {fmtQty(l.qtyMilli)} {l.unit} × {fmtAmount(l.price)}
                  </span>
                  <span className="inv-line__total num">
                    {fmtAmount(l.overrideTotal ?? lineTotal(l.qtyMilli, l.price))}
                  </span>
                </div>
              ))
            ) : (
              <div className="inv-line inv-line--empty">سطری ندارد</div>
            )}
          </div>

          <div className="inv-paper__sums">
            {(doc.discount !== 0 || doc.rounding !== 0) && (
              <SumRow label="جمع سطرها" value={fmtAmount(sum)} />
            )}
            {doc.discount !== 0 && <SumRow label="تخفیف" value={fmtAmount(-doc.discount)} />}
            {doc.rounding !== 0 && <SumRow label="رند" value={fmtAmount(doc.rounding)} />}
            <SumRow label={purchase ? "جمع خرید" : "جمع کل"} value={fmtAmount(doc.total)} strong />
            {doc.paid > 0 && <SumRow label="پرداخت" value={fmtAmount(doc.paid)} />}
            <SumRow
              label="مانده"
              value={doc.remaining > 0 ? fmtAmount(doc.remaining) : "تسویه شد"}
              tone={doc.remaining > 0 ? "rest" : "clear"}
            />
          </div>

          {svg === "" && settings?.invoiceNote && (
            <div className="inv-paper__note">{settings.invoiceNote}</div>
          )}
        </Card>

        {!voided && (
          <Card tight>
            <Row
              leading={<Icon name="send" />}
              title="فرستادن فاکتور"
              sub="به تلگرام یا برنامه دیگر"
              value={which ? <span className="faint tiny">می‌رود…</span> : undefined}
              onClick={() => setChoosing(true)}
            />
            <Row
              leading={<Icon name="book" />}
              title={purchase ? "خرید جدید برای همین فروشنده" : "فاکتور جدید برای همین مشتری"}
              sub={party.name}
              onClick={() =>
                nav.go({ name: "order", kind: doc.kind, partyId: party.id })
              }
            />
            <Row
              leading={<Icon name="parties" />}
              title="حساب این نفر"
              sub={party.name}
              onClick={() => nav.go({ name: "party", id: party.id })}
            />
          </Card>
        )}

        {!voided && (
          <div className="inv-voidrow">
            <Btn variant="danger" wide onClick={() => setVoiding(true)}>
              خط زدن فاکتور
            </Btn>
          </div>
        )}
      </Body>

      <Sheet open={choosing} onClose={() => setChoosing(false)} title="فرستادن فاکتور">
        <Card tight>
          <Row
            leading={<Icon name="send" />}
            title="فرستادن به تلگرام"
            sub="به ربات خودت"
            value={which === "tg" ? <span className="faint tiny">می‌رود…</span> : undefined}
            onClick={() => void sendTelegram()}
          />
          <Row
            leading={<Icon name="more" />}
            title="اشتراک‌گذاری"
            sub="به هر برنامه دیگری"
            value={which === "share" ? <span className="faint tiny">می‌رود…</span> : undefined}
            onClick={() => void shareIt()}
          />
        </Card>
      </Sheet>

      <Sheet open={voiding} onClose={() => setVoiding(false)} title="خط زدن فاکتور">
        <p className="howto-p">
          فاکتور در دفتر می‌ماند، ولی از حساب این نفر و از جمع‌ها برداشته می‌شود.
        </p>
        <Field
          label="چرا؟"
          value={reason}
          onChange={setReason}
          autoFocus
          placeholder="مثلاً اشتباه زده شد"
        />
        <Btn wide variant="danger" busy={busy} disabled={!reason.trim()} onClick={() => void doVoid()}>
          خط بزن
        </Btn>
      </Sheet>
    </Screen>
  );
}

function SumRow({
  label,
  value,
  strong = false,
  tone = "plain",
}: {
  label: string;
  value: string;
  strong?: boolean;
  tone?: "plain" | "rest" | "clear";
}) {
  return (
    <div className={`inv-sum inv-sum--${tone} ${strong ? "is-strong" : ""}`}>
      <span className="inv-sum__label">{label}</span>
      <span className="inv-sum__value num">{value}</span>
    </div>
  );
}
