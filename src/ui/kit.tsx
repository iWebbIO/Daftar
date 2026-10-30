// The handful of pieces every screen is built from.
//
// Nothing here is clever. A screen is a head, a list of cards, and big numbers
// on the left of each row, because that is what a shopkeeper reads first.

import { useEffect, useRef, type ReactNode } from "react";
import { fmtAmount, fmtQty } from "../format";
import "./kit.css";

// ---------------------------------------------------------------- structure

export function Screen({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return <div className={`screen ${className}`}>{children}</div>;
}

/** The fixed top of a screen: a title, an optional back arrow, optional action. */
export function Head({
  title,
  sub,
  onBack,
  right,
  big = false,
}: {
  title: ReactNode;
  sub?: ReactNode;
  onBack?: () => void;
  right?: ReactNode;
  big?: boolean;
}) {
  return (
    <header className={`head ${big ? "head--big" : ""}`}>
      <div className="head__lead">
        {onBack && (
          <button className="head__back" onClick={onBack} aria-label="برگشت">
            <Icon name="back" />
          </button>
        )}
        <div className="head__text">
          <h1 className="head__title">{title}</h1>
          {sub && <div className="head__sub">{sub}</div>}
        </div>
      </div>
      {right && <div className="head__right">{right}</div>}
    </header>
  );
}

export function Body({ children, pad = true }: { children: ReactNode; pad?: boolean }) {
  return <div className={`body scroll ${pad ? "" : "body--flush"}`}>{children}</div>;
}

export function Card({
  children,
  onClick,
  className = "",
  tight = false,
}: {
  children: ReactNode;
  onClick?: () => void;
  className?: string;
  tight?: boolean;
}) {
  const cls = `card ${tight ? "card--tight" : ""} ${onClick ? "card--tap" : ""} ${className}`;
  if (onClick) {
    return (
      <button className={cls} onClick={onClick}>
        {children}
      </button>
    );
  }
  return <div className={cls}>{children}</div>;
}

export function Label({ children }: { children: ReactNode }) {
  return <div className="label">{children}</div>;
}

export function Divider() {
  return <div className="divider" />;
}

/**
 * One line of a list: what it was on the right, what it cost on the left.
 */
export function Row({
  title,
  sub,
  value,
  note,
  onClick,
  tone = "plain",
  struck = false,
  leading,
}: {
  title: ReactNode;
  sub?: ReactNode;
  value?: ReactNode;
  note?: ReactNode;
  onClick?: () => void;
  tone?: "plain" | "pos" | "neg" | "warn";
  struck?: boolean;
  leading?: ReactNode;
}) {
  const content = (
    <>
      {leading && <div className="row__lead">{leading}</div>}
      <div className="row__main">
        <div className={`row__title ${struck ? "is-struck" : ""}`}>{title}</div>
        {sub && <div className="row__sub">{sub}</div>}
      </div>
      {(value !== undefined || note) && (
        <div className="row__end">
          {value !== undefined && <div className={`row__value num tone-${tone}`}>{value}</div>}
          {note && <div className="row__note">{note}</div>}
        </div>
      )}
    </>
  );
  if (onClick) {
    return (
      <button className="row row--tap" onClick={onClick}>
        {content}
      </button>
    );
  }
  return <div className="row">{content}</div>;
}

/** A number the eye should land on. */
export function Money({
  value,
  tone = "plain",
  size = "md",
  unit = false,
}: {
  value: number;
  tone?: "plain" | "pos" | "neg" | "warn" | "auto";
  size?: "sm" | "md" | "lg" | "xl";
  unit?: boolean;
}) {
  const resolved = tone === "auto" ? (value > 0 ? "pos" : value < 0 ? "neg" : "plain") : tone;
  return (
    <span className={`money num money--${size} tone-${resolved}`}>
      {fmtAmount(value)}
      {unit && <span className="money__unit"> تومان</span>}
    </span>
  );
}

export function Qty({ milli, unit }: { milli: number; unit?: string }) {
  return (
    <span className="qty num">
      {fmtQty(milli)}
      {unit ? ` ${unit}` : ""}
    </span>
  );
}

export function Pill({
  children,
  tone = "plain",
}: {
  children: ReactNode;
  tone?: "plain" | "pos" | "neg" | "warn" | "brand";
}) {
  return <span className={`pill pill--${tone}`}>{children}</span>;
}

/** What a party's balance means, in words rather than a sign. */
export function Balance({ value }: { value: number }) {
  if (value === 0) {
    return (
      <div className="balance balance--zero">
        <span className="balance__word">حساب صفر</span>
      </div>
    );
  }
  const owes = value > 0;
  return (
    <div className={`balance ${owes ? "balance--pos" : "balance--neg"}`}>
      <div className="balance__word">{owes ? "طلب شما" : "بدهی شما"}</div>
      <div className="balance__num num">{fmtAmount(Math.abs(value))}</div>
      <div className="balance__unit">تومان</div>
    </div>
  );
}

// ---------------------------------------------------------------- controls

export function Btn({
  children,
  onClick,
  variant = "primary",
  wide = false,
  disabled = false,
  busy = false,
  size = "md",
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "primary" | "ghost" | "quiet" | "danger" | "positive";
  wide?: boolean;
  disabled?: boolean;
  busy?: boolean;
  size?: "sm" | "md" | "lg";
}) {
  return (
    <button
      className={`btn btn--${variant} btn--${size} ${wide ? "btn--wide" : ""} ${
        busy ? "is-busy" : ""
      }`}
      onClick={onClick}
      disabled={disabled || busy}
    >
      {busy ? <span className="btn__spin" /> : children}
    </button>
  );
}

export function Field({
  label,
  value,
  onChange,
  placeholder,
  inputMode = "text",
  suffix,
  autoFocus = false,
  onEnter,
  multiline = false,
  rows = 4,
}: {
  label?: ReactNode;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  inputMode?: "text" | "numeric" | "tel" | "decimal";
  suffix?: ReactNode;
  autoFocus?: boolean;
  onEnter?: () => void;
  multiline?: boolean;
  rows?: number;
}) {
  const ref = useRef<HTMLInputElement | HTMLTextAreaElement>(null);
  useEffect(() => {
    if (autoFocus) {
      const t = window.setTimeout(() => ref.current?.focus(), 120);
      return () => window.clearTimeout(t);
    }
  }, [autoFocus]);

  const shared = {
    value,
    placeholder,
    onChange: (e: { target: { value: string } }) => onChange(e.target.value),
    onKeyDown: (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && onEnter && !multiline) {
        e.preventDefault();
        onEnter();
      }
    },
  };

  return (
    <div className="field">
      {label && <div className="field__label">{label}</div>}
      <div className={`field__box ${multiline ? "field__box--area" : ""}`}>
        {multiline ? (
          <textarea
            ref={ref as React.Ref<HTMLTextAreaElement>}
            rows={rows}
            inputMode={inputMode === "numeric" ? "numeric" : undefined}
            {...shared}
          />
        ) : (
          <input
            ref={ref as React.Ref<HTMLInputElement>}
            type="text"
            inputMode={inputMode === "numeric" ? "numeric" : inputMode === "tel" ? "tel" : "text"}
            dir={inputMode === "text" ? "auto" : "ltr"}
            {...shared}
          />
        )}
        {suffix && <div className="field__suffix">{suffix}</div>}
      </div>
    </div>
  );
}

/** A quiet band of choices. */
export function Segmented<T extends string | number>({
  options,
  value,
  onChange,
}: {
  options: { value: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="segmented">
      {options.map((o) => (
        <button
          key={String(o.value)}
          className={`segmented__item ${o.value === value ? "is-on" : ""}`}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** A bottom sheet: everything that needs a bit of the screen slides up. */
export function Sheet({
  open,
  onClose,
  title,
  children,
  full = false,
}: {
  open: boolean;
  onClose: () => void;
  title?: ReactNode;
  children: ReactNode;
  full?: boolean;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;
  return (
    <div className="sheet-layer" role="dialog" aria-modal="true">
      <div className="sheet-backdrop" onClick={onClose} />
      <div className={`sheet ${full ? "sheet--full" : ""}`}>
        <div className="sheet__grip" />
        {title && (
          <div className="sheet__head">
            <div className="sheet__title">{title}</div>
            <button className="sheet__close" onClick={onClose} aria-label="بستن">
              <Icon name="close" />
            </button>
          </div>
        )}
        <div className="sheet__body scroll">{children}</div>
      </div>
    </div>
  );
}

export function Empty({
  title,
  hint,
  action,
}: {
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      <div className="empty__title">{title}</div>
      {hint && <div className="empty__hint">{hint}</div>}
      {action && <div className="empty__action">{action}</div>}
    </div>
  );
}

export function Loading({ text = "یک لحظه…" }: { text?: string }) {
  return (
    <div className="loading">
      <span className="loading__dot" />
      <span className="loading__dot" />
      <span className="loading__dot" />
      <span className="loading__text">{text}</span>
    </div>
  );
}

/**
 * The numeric pad. Big keys, order the way a phone keypad is: ۷۸۹ on the top
 * row, because that is what the hand already knows.
 */
export function NumPad({
  onKey,
  onSubmit,
  submitLabel = "تایید",
  extra,
}: {
  onKey: (key: string) => void;
  onSubmit?: () => void;
  submitLabel?: string;
  extra?: ReactNode;
}) {
  const keys = ["۷", "۸", "۹", "۴", "۵", "۶", "۱", "۲", "۳", "۰", "٫", "⌫"];
  return (
    <div className="numpad">
      <div className="numpad__grid">
        {keys.map((k) => (
          <button
            key={k}
            className={`numpad__key ${k === "⌫" ? "numpad__key--del" : ""}`}
            onClick={() => onKey(k)}
          >
            {k}
          </button>
        ))}
        {extra}
        {onSubmit && (
          <button className="numpad__key numpad__key--ok" onClick={onSubmit}>
            {submitLabel}
          </button>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- icons

const PATHS: Record<string, ReactNode> = {
  today: (
    <>
      <rect x="3" y="4.5" width="18" height="16" rx="4" />
      <path d="M3 9.5h18M8 3v3M16 3v3" />
    </>
  ),
  parties: (
    <>
      <circle cx="9" cy="8" r="3.2" />
      <path d="M3.5 19.5c0-3 2.5-5 5.5-5s5.5 2 5.5 5" />
      <path d="M16 6.2a3 3 0 0 1 0 5.6M17.5 19.5c0-2.2-.8-3.9-2-4.9" />
    </>
  ),
  tasks: (
    <>
      <rect x="3" y="4" width="18" height="16" rx="4" />
      <path d="M7.5 12l2.6 2.6L16 9" />
    </>
  ),
  more: (
    <>
      <circle cx="5.5" cy="12" r="1.6" />
      <circle cx="12" cy="12" r="1.6" />
      <circle cx="18.5" cy="12" r="1.6" />
    </>
  ),
  plus: <path d="M12 5v14M5 12h14" />,
  back: <path d="M9 6l6 6-6 6" />,
  close: <path d="M6 6l12 12M18 6L6 18" />,
  search: (
    <>
      <circle cx="11" cy="11" r="6.5" />
      <path d="M16 16l4 4" />
    </>
  ),
  calc: (
    <>
      <rect x="4" y="3" width="16" height="18" rx="3.5" />
      <path d="M8 7.5h8M8 12h2M8 16h2M14 12h2M14 16h2" />
    </>
  ),
  check: <path d="M5 12.5l4 4L19 7" />,
  chevron: <path d="M15 6l-6 6 6 6" />,
  send: <path d="M4 12l16-7-7 16-2.5-6.5L4 12z" />,
  edit: (
    <>
      <path d="M4 20l4-.8 10-10a2.4 2.4 0 0 0-3.4-3.4L4.6 15.8 4 20z" />
    </>
  ),
  phone: (
    <path d="M6 3.5h3l1.5 4-2 1.4a12 12 0 0 0 5.6 5.6l1.4-2 4 1.5v3a2 2 0 0 1-2.2 2A16.5 16.5 0 0 1 4 5.7 2 2 0 0 1 6 3.5z" />
  ),
  trash: (
    <>
      <path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13" />
    </>
  ),
  book: (
    <>
      <path d="M4 5.5A2.5 2.5 0 0 1 6.5 3H19v18H6.5A2.5 2.5 0 0 1 4 18.5z" />
      <path d="M8 7h7M8 11h7" />
    </>
  ),
};

export function Icon({
  name,
  size = 22,
  filled = false,
}: {
  name: keyof typeof PATHS | string;
  size?: number;
  filled?: boolean;
}) {
  return (
    <svg
      className={`icon ${filled ? "icon--filled" : ""}`}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.9"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {PATHS[name] ?? null}
    </svg>
  );
}
