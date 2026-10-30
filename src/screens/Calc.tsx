// The margin arithmetic from the notebook: 17 + 6 = 23, written down the side of
// the page. A tape, not a keypad full of functions — what he types stays visible,
// so a mistake is something he can see rather than something he has to remember.

import { useCallback, useState } from "react";
import { fmtAmount, latin, parseAmount } from "../format";
import { useNav } from "../nav";
import { Head, Icon, NumPad, Screen } from "../ui/kit";
import "./calc.css";

type Op = "+" | "−" | "×" | "÷";

interface Entry {
  a: number;
  op: Op;
  b: number | null;
}

function apply(a: number, op: Op, b: number): number {
  switch (op) {
    case "+":
      return a + b;
    case "−":
      return a - b;
    case "×":
      return Math.round(a * b);
    case "÷":
      return b === 0 ? a : Math.round(a / b);
  }
}

export default function Calc() {
  const nav = useNav();
  const [text, setText] = useState("");
  const [acc, setAcc] = useState<number | null>(null);
  const [pending, setPending] = useState<Op | null>(null);
  const [tape, setTape] = useState<Entry[]>([]);
  const [fresh, setFresh] = useState(true);

  const value = parseAmount(text) ?? 0;

  const onKey = useCallback(
    (key: string) => {
      if (key === "⌫") {
        setText((t) => t.slice(0, -1));
        return;
      }
      // A shop calculator works in whole toman; the pad's decimal key does nothing.
      if (key === "٫") return;
      const digit = latin(key);
      if (!/^\d$/.test(digit)) return;
      setText((t) => {
        if (fresh || t === "0") return digit;
        return t.length < 12 ? t + digit : t;
      });
      setFresh(false);
    },
    [fresh],
  );

  const chooseOp = (op: Op) => {
    if (pending !== null && acc !== null && text !== "") {
      const result = apply(acc, pending, value);
      setTape((t) => {
        const next = t.map((e, i) =>
          i === t.length - 1 && e.b === null ? { ...e, b: value } : e,
        );
        return [...next, { a: result, op, b: null }];
      });
      setAcc(result);
      setText("");
    } else {
      const current = text === "" ? (acc ?? 0) : value;
      setAcc(current);
      setTape((t) => [...t, { a: current, op, b: null }]);
      setText("");
    }
    setPending(op);
    setFresh(true);
  };

  const equals = () => {
    if (pending === null || acc === null) return;
    const result = apply(acc, pending, value);
    setTape((t) =>
      t.map((e, i) => (i === t.length - 1 && e.b === null ? { ...e, b: value } : e)),
    );
    setAcc(result);
    setPending(null);
    setText(String(result));
    setFresh(true);
  };

  const clear = () => {
    setText("");
    setAcc(null);
    setPending(null);
    setTape([]);
    setFresh(true);
  };

  const shown = text === "" ? (acc ?? 0) : value;

  return (
    <Screen>
      <Head
        title="ماشین‌حساب"
        onBack={nav.back}
        right={
          tape.length > 0 ? (
            <button className="calc__clear" onClick={clear}>
              پاک کن
            </button>
          ) : undefined
        }
      />

      <div className="calc__tape scroll">
        {tape.map((e, i) => (
          <div className="calc__line" key={i}>
            <span className="num">{fmtAmount(e.a)}</span>
            <span className="calc__op">{e.op}</span>
            <span className="num">{e.b === null ? "…" : fmtAmount(e.b)}</span>
          </div>
        ))}
      </div>

      <div className="calc__result">
        <div className="calc__label">
          {pending ? `${fmtAmount(acc ?? 0)} ${pending}` : text === "" ? "نتیجه" : "\u00a0"}
        </div>
        <div className="calc__value num">{fmtAmount(shown)}</div>
      </div>

      <div className="calc__pad">
        <div className="calc__ops">
          {(["÷", "×", "−", "+"] as Op[]).map((op) => (
            <button
              key={op}
              className={`calc__opbtn ${pending === op ? "is-on" : ""}`}
              onClick={() => chooseOp(op)}
            >
              {op}
            </button>
          ))}
          <button className="calc__eq" onClick={equals} disabled={pending === null}>
            <Icon name="check" size={20} />
          </button>
        </div>
        <NumPad onKey={onKey} />
      </div>
    </Screen>
  );
}
