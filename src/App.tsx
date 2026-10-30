// The shell: one stack of screens, one tab bar, one place a message appears.

import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorText } from "./api";
import { setCurrency } from "./format";
import { AppCtx, type Nav, type Route, hashToRoute, routeToHash } from "./nav";
import { Icon } from "./ui/kit";
import type { Settings } from "./types";

import Today from "./screens/Today";
import Parties from "./screens/Parties";
import Party from "./screens/Party";
import Order from "./screens/Order";
import Payment from "./screens/Payment";
import Tasks from "./screens/Tasks";
import More from "./screens/More";
import Cheques from "./screens/Cheques";
import Notes from "./screens/Notes";
import Sums from "./screens/Sums";
import Reports from "./screens/Reports";
import SettingsScreen from "./screens/Settings";
import Search from "./screens/Search";
import Products from "./screens/Products";
import Invoice from "./screens/Invoice";
import Calc from "./screens/Calc";

interface Toast {
  id: number;
  text: string;
  tone: "plain" | "ok" | "bad";
}

const TABS: { name: Route["name"]; label: string; icon: string }[] = [
  { name: "today", label: "امروز", icon: "today" },
  { name: "parties", label: "حساب‌ها", icon: "parties" },
  { name: "tasks", label: "کارها", icon: "tasks" },
  { name: "more", label: "بیشتر", icon: "more" },
];

export default function App() {
  const [stack, setStack] = useState<Route[]>(() => [hashToRoute(window.location.hash)]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);

  const route = stack[stack.length - 1]!;

  // Each screen keeps an address, so the phone's back gesture, a reload and the
  // screenshot harness all land on the same place.
  useEffect(() => {
    const hash = routeToHash(route);
    if (window.location.hash !== hash) {
      window.history.replaceState(null, "", hash);
    }
  }, [route]);

  const reloadSettings = useCallback(async () => {
    try {
      const s = await api.settings();
      setSettings(s);
      document.documentElement.dataset.theme = s.theme === "dark" ? "dark" : "light";
      setCurrency(s.currency);
    } catch (e) {
      setSettings((old) => old);
      console.warn(errorText(e));
    }
  }, []);

  useEffect(() => {
    void reloadSettings();
  }, [reloadSettings]);

  const toast = useCallback((text: string, tone: "plain" | "ok" | "bad" = "plain") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, text, tone }]);
    window.setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 3200);
  }, []);

  const nav: Nav = useMemo(
    () => ({
      route,
      canGoBack: stack.length > 1,
      go: (next: Route) => setStack((s) => [...s, next]),
      back: () => setStack((s) => (s.length > 1 ? s.slice(0, -1) : s)),
      replace: (next: Route) => setStack((s) => [...s.slice(0, -1), next]),
    }),
    [route, stack.length],
  );

  // The phone's own back gesture and the browser's back both leave a screen
  // rather than closing the app.
  useEffect(() => {
    const onPop = () => nav.back();
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, [nav]);

  // A link straight to a screen resets the stack to it.
  useEffect(() => {
    const onHash = () => {
      const next = hashToRoute(window.location.hash);
      setStack((s) => (routeToHash(s[s.length - 1]!) === routeToHash(next) ? s : [next]));
    };
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  // A field under the keyboard is the worst thing that can happen in the app:
  // the page makes room for the keyboard, and whatever is being typed is brought
  // into the middle of what is left.
  useEffect(() => {
    const vv = window.visualViewport;
    if (!vv) return;
    const apply = () => {
      const covered = Math.max(0, Math.round(window.innerHeight - vv.height - vv.offsetTop));
      document.documentElement.style.setProperty("--kb", `${covered}px`);
      document.body.classList.toggle("kb-open", covered > 90);
    };
    vv.addEventListener("resize", apply);
    vv.addEventListener("scroll", apply);
    apply();
    return () => {
      vv.removeEventListener("resize", apply);
      vv.removeEventListener("scroll", apply);
    };
  }, []);

  useEffect(() => {
    const onFocus = (e: Event) => {
      const el = e.target as HTMLElement | null;
      if (!el || (el.tagName !== "INPUT" && el.tagName !== "TEXTAREA")) return;
      window.setTimeout(() => {
        el.scrollIntoView({ block: "center", behavior: "smooth" });
      }, 300);
    };
    document.addEventListener("focusin", onFocus);
    return () => document.removeEventListener("focusin", onFocus);
  }, []);

  const screen = useMemo(() => {
    switch (route.name) {
      case "today":
        return <Today />;
      case "parties":
        return <Parties />;
      case "party":
        return <Party id={route.id} />;
      case "order":
        return <Order kind={route.kind} id={route.id} partyId={route.partyId} />;
      case "payment":
        return <Payment partyId={route.partyId} />;
      case "tasks":
        return <Tasks />;
      case "more":
        return <More />;
      case "cheques":
        return <Cheques />;
      case "notes":
        return <Notes />;
      case "sums":
        return <Sums id={route.id} />;
      case "reports":
        return <Reports />;
      case "settings":
        return <SettingsScreen />;
      case "search":
        return <Search />;
      case "products":
        return <Products />;
      case "invoice":
        return <Invoice docId={route.docId} />;
      case "calc":
        return <Calc />;
      default:
        return null;
    }
  }, [route]);

  const chromeFree = route.name === "order" || route.name === "payment";

  return (
    <AppCtx.Provider value={{ nav, settings, reloadSettings, toast }}>
      <div className="app-viewport">
        <div className="app-frame">
          <div className="screen" key={`${route.name}-${"id" in route ? route.id ?? "" : ""}`}>
            {screen}
          </div>

          {!chromeFree && (
            <nav className="tabbar">
              {TABS.slice(0, 2).map((t) => (
                <TabButton key={t.name} tab={t} active={route.name === t.name} onClick={nav.replace} />
              ))}
              <div className="tabbar__fab">
                <button
                  className="tabbar__fabBtn"
                  aria-label="فاکتور نو"
                  onClick={() => nav.go({ name: "order", kind: "sale" })}
                >
                  <Icon name="plus" size={26} />
                </button>
              </div>
              {TABS.slice(2).map((t) => (
                <TabButton key={t.name} tab={t} active={route.name === t.name} onClick={nav.replace} />
              ))}
            </nav>
          )}

          <div className="toasts">
            {toasts.map((t) => (
              <div key={t.id} className={`toast toast--${t.tone}`}>
                {t.text}
              </div>
            ))}
          </div>
        </div>
      </div>
    </AppCtx.Provider>
  );
}

function TabButton({
  tab,
  active,
  onClick,
}: {
  tab: { name: Route["name"]; label: string; icon: string };
  active: boolean;
  onClick: (route: Route) => void;
}) {
  return (
    <button
      className={`tabbar__item ${active ? "is-on" : ""}`}
      onClick={() => onClick({ name: tab.name } as Route)}
    >
      <Icon name={tab.icon} size={21} />
      <span>{tab.label}</span>
    </button>
  );
}
