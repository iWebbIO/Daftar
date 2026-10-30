// Where you are, how you got there, and the two things every screen needs
// besides its own data: a way to move, and a way to say something went wrong.

import { createContext, useContext } from "react";
import type { Settings, UpdateInfo } from "./types";

export type Route =
  | { name: "today" }
  | { name: "parties" }
  | { name: "party"; id: number }
  | { name: "order"; kind: "sale" | "purchase"; id?: number; partyId?: number }
  | { name: "payment"; partyId?: number }
  | { name: "tasks" }
  | { name: "more" }
  | { name: "cheques" }
  | { name: "notes" }
  | { name: "sums"; id?: number }
  | { name: "reports" }
  | { name: "settings" }
  | { name: "search" }
  | { name: "products" }
  | { name: "invoice"; docId: number }
  | { name: "calc" };

export interface Nav {
  route: Route;
  go: (route: Route) => void;
  back: () => void;
  replace: (route: Route) => void;
  canGoBack: boolean;
}

/** `#party/12`, `#order/sale`, `#invoice/7` — so a screen has an address. */
export function routeToHash(route: Route): string {
  switch (route.name) {
    case "party":
      return `#party/${route.id}`;
    case "order":
      return `#order/${route.kind}${route.id ? `/${route.id}` : ""}`;
    case "payment":
      return `#payment${route.partyId ? `/${route.partyId}` : ""}`;
    case "sums":
      return `#sums${route.id ? `/${route.id}` : ""}`;
    case "invoice":
      return `#invoice/${route.docId}`;
    default:
      return `#${route.name}`;
  }
}

export function hashToRoute(hash: string): Route {
  const [name, ...rest] = hash.replace(/^#/, "").split("/");
  const id = rest[0] ? Number(rest[0]) : undefined;
  switch (name) {
    case "parties":
      return { name: "parties" };
    case "party":
      return { name: "party", id: Number.isFinite(id) ? Number(id) : 1 };
    case "order":
      return {
        name: "order",
        kind: rest[0] === "purchase" ? "purchase" : "sale",
        id: rest[1] ? Number(rest[1]) : undefined,
      };
    case "payment":
      return { name: "payment", partyId: Number.isFinite(id) ? Number(id) : undefined };
    case "tasks":
      return { name: "tasks" };
    case "more":
      return { name: "more" };
    case "cheques":
      return { name: "cheques" };
    case "notes":
      return { name: "notes" };
    case "sums":
      return { name: "sums", id: Number.isFinite(id) ? Number(id) : undefined };
    case "reports":
      return { name: "reports" };
    case "settings":
      return { name: "settings" };
    case "search":
      return { name: "search" };
    case "products":
      return { name: "products" };
    case "invoice":
      return { name: "invoice", docId: Number.isFinite(id) ? Number(id) : 1 };
    case "calc":
      return { name: "calc" };
    default:
      return { name: "today" };
  }
}

export interface AppContextValue {
  nav: Nav;
  settings: Settings | null;
  /** Set once the startup check finds a newer release; null while it is current. */
  update: UpdateInfo | null;
  reloadSettings: () => Promise<void>;
  toast: (text: string, tone?: "plain" | "ok" | "bad") => void;
}

export const AppCtx = createContext<AppContextValue | null>(null);

export function useApp(): AppContextValue {
  const ctx = useContext(AppCtx);
  if (!ctx) throw new Error("AppCtx missing");
  return ctx;
}

export function useNav(): Nav {
  return useApp().nav;
}

export function useSettings(): Settings {
  return useApp().settings as Settings;
}
