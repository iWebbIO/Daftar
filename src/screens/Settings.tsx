// Settings, in the order a shopkeeper meets them: what the shop is called, how
// the bot is set up, and whether the book is backed up.

import { useEffect, useState } from "react";
import { api, errorText, inApp } from "../api";
import { fmtAmount } from "../format";
import { dayOf, longDate } from "../jalali";
import { useApp, useNav } from "../nav";
import {
  Body,
  Btn,
  Card,
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
import type { BackupInfo, Settings } from "../types";

export default function SettingsScreen() {
  const nav = useNav();
  const { toast, reloadSettings } = useApp();
  const [s, setS] = useState<Settings | null>(null);
  const [busy, setBusy] = useState(false);
  const [backup, setBackup] = useState<BackupInfo | null>(null);
  const [tokenSheet, setTokenSheet] = useState(false);

  useEffect(() => {
    void (async () => {
      try {
        setS(await api.settings());
      } catch (e) {
        toast(errorText(e), "bad");
      }
    })();
  }, [toast]);

  if (!s) {
    return (
      <Screen>
        <Head title="تنظیمات" onBack={nav.back} />
        <Body>
          <Loading />
        </Body>
      </Screen>
    );
  }

  const patch = (part: Partial<Settings>) => setS({ ...s, ...part });

  const save = async (part: Partial<Settings>) => {
    const next = { ...s, ...part };
    setS(next);
    try {
      await api.saveSettings(next);
      await reloadSettings();
    } catch (e) {
      toast(errorText(e), "bad");
    }
  };

  const doBackup = async () => {
    setBusy(true);
    try {
      const info = await api.backupNow();
      setBackup(info);
      toast(info.message, info.sent ? "ok" : "bad");
      await reloadSettings();
    } catch (e) {
      toast(errorText(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen>
      <Head title="تنظیمات" onBack={nav.back} />
      <Body>
        <div className="label">فروشگاه</div>
        <Card>
          <Field
            label="نام فروشگاه"
            value={s.shopName}
            onChange={(v) => patch({ shopName: v })}
            placeholder="مثلاً خشکبار امینی"
            onEnter={() => save({ shopName: s.shopName })}
          />
          <Field
            label="تلفن"
            value={s.shopPhone}
            onChange={(v) => patch({ shopPhone: v })}
            inputMode="tel"
            placeholder="۰۹۱۲…"
            onEnter={() => save({ shopPhone: s.shopPhone })}
          />
          <Field
            label="نشانی"
            value={s.shopAddress}
            onChange={(v) => patch({ shopAddress: v })}
            onEnter={() => save({ shopAddress: s.shopAddress })}
          />
          <Field
            label="نوشته پایین فاکتور"
            value={s.invoiceNote}
            onChange={(v) => patch({ invoiceNote: v })}
            placeholder="مثلاً ممنون از خرید شما"
            onEnter={() => save({ invoiceNote: s.invoiceNote })}
          />
          <Btn wide onClick={() => save({})}>
            ذخیره
          </Btn>
        </Card>

        <div className="label">کارت برای پرداخت</div>
        <Card>
          <div className="tiny faint" style={{ marginBottom: 10 }}>
            روی فاکتور چاپ می‌شود تا مشتری کارت به کارت کند.
          </div>
          <Field
            label="بانک"
            value={s.bankName}
            onChange={(v) => patch({ bankName: v })}
            placeholder="مثلاً بانک ملت"
            onEnter={() => save({ bankName: s.bankName })}
          />
          <Field
            label="صاحب حساب"
            value={s.cardHolder}
            onChange={(v) => patch({ cardHolder: v })}
            onEnter={() => save({ cardHolder: s.cardHolder })}
          />
          <Field
            label="شماره کارت"
            value={s.cardNumber}
            onChange={(v) => patch({ cardNumber: v })}
            inputMode="numeric"
            placeholder="6037-9974-1234-5678"
            onEnter={() => save({ cardNumber: s.cardNumber })}
          />
          <Btn wide onClick={() => save({})}>
            ذخیره
          </Btn>
        </Card>

        <div className="label">روبـات تلگرام</div>
        <Card>
          <Row
            leading={<Icon name="send" />}
            title={s.chatId ? "وصل است" : "وصل نیست"}
            sub={s.chatId ? "فاکتور و پشتیبان به تلگرام می‌رود" : "توکن ربات را بگذار تا وصل شود"}
            value={s.chatId ? <Pill tone="pos">آماده</Pill> : <Pill tone="warn">نصب نشده</Pill>}
            onClick={() => setTokenSheet(true)}
          />
        </Card>

        <div className="label">پشتیبان</div>
        <Card>
          <Row
            leading={<Icon name="today" />}
            title={s.lastBackupAt ? `آخرین پشتیبان: ${longDate(dayOf(new Date(s.lastBackupAt)))}` : "هنوز پشتیبان نگرفته‌ای"}
            sub={s.lastBackupAt ? saat(s.lastBackupAt) : "هر روز خودش می‌فرستد"}
            value={
              s.chatId ? undefined : <span className="faint tiny">بدون تلگرام فقط روی گوشی</span>
            }
          />
          <div style={{ padding: "4px 14px 12px" }}>
            <Btn wide variant={s.chatId ? "primary" : "ghost"} busy={busy} onClick={doBackup}>
              همین حالا پشتیبان بگیر
            </Btn>
            {backup && (
              <div className="tiny faint" style={{ marginTop: 8 }}>
                {backup.message} · {fmtAmount(Math.round(backup.size / 1024))} کیلوبایت
              </div>
            )}
          </div>
        </Card>

        <div className="label">شکل برنامه</div>
        <Card>
          <div style={{ padding: "12px 14px" }}>
            <Segmented
              options={[
                { value: "light", label: "روشن" },
                { value: "dark", label: "تیره" },
              ]}
              value={s.theme}
              onChange={(v) => {
                patch({ theme: v });
                document.documentElement.dataset.theme = v;
                void save({ theme: v });
              }}
            />
            <div className="tiny faint">برای مغازه روشن بهتر است.</div>
          </div>
        </Card>

        {!inApp() && (
          <div className="tiny faint" style={{ marginTop: 16, textAlign: "center" }}>
            این نسخه فقط برای دیدن ظاهر است و به دفتر اصلی وصل نیست.
          </div>
        )}
      </Body>

      <TokenSheet
        open={tokenSheet}
        onClose={() => setTokenSheet(false)}
        token={s.botToken}
        onSaved={async (token, chatId) => {
          await save({ botToken: token, chatId });
          setTokenSheet(false);
        }}
      />
    </Screen>
  );
}

function TokenSheet({
  open,
  onClose,
  token,
  onSaved,
}: {
  open: boolean;
  onClose: () => void;
  token: string;
  onSaved: (token: string, chatId: string) => Promise<void>;
}) {
  const [value, setValue] = useState(token);
  const [step, setStep] = useState<"token" | "start">("token");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setValue(token);
      setStep("token");
      setErr(null);
    }
  }, [open, token]);

  const connect = async () => {
    setBusy(true);
    setErr(null);
    try {
      const info = await api.botLink(value.trim());
      await onSaved(value.trim(), info.chatId ?? "");
    } catch (e) {
      setErr(errorText(e));
      setStep("start");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Sheet open={open} onClose={onClose} title="وصل کردن ربات">
      {step === "token" ? (
        <>
          <ol className="howto">
            <li>در تلگرام به BotFather پیام بده.</li>
            <li>با /newbot یک ربات بساز.</li>
            <li>توکنی که می‌دهد را اینجا بگذار.</li>
          </ol>
          <Field
            label="توکن ربات"
            value={value}
            onChange={setValue}
            placeholder="123456789:AA…"
            autoFocus
          />
          {err && <div className="tiny" style={{ color: "var(--negative)" }}>{err}</div>}
          <Btn wide busy={busy} disabled={value.trim().length < 20} onClick={connect}>
            وصل کن
          </Btn>
        </>
      ) : (
        <>
          <p className="howto-p">
            حالا در تلگرام ربات خودت را باز کن و دکمه <b>Start</b> را بزن. بعد اینجا دوباره
            بزن.
          </p>
          <Btn wide busy={busy} onClick={connect}>
            Start را زدم
          </Btn>
        </>
      )}
    </Sheet>
  );
}

const fa = (n: number | string) =>
  String(n).replace(/[0-9]/g, (d) => "۰۱۲۳۴۵۶۷۸۹"[Number(d)]!);

function saat(ms: number): string {
  const d = new Date(ms);
  const h = fa(d.getHours());
  const m = fa(String(d.getMinutes()).padStart(2, "0"));
  return `ساعت ${h}:${m}`;
}
