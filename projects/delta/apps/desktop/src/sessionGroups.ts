// Sidebar organisation helpers: date buckets for the Recent list and the client-side "projects"
// (named groups of sessions). Projects are presentation metadata kept on this device, like the
// collapsed-sidebar flag — the runtime has no project concept, and a session's folder, approvals
// and history never depend on which project it is filed under.
import { useCallback, useEffect, useState } from "react";

export type DateBucket = "today" | "yesterday" | "last7" | "earlier";
export const DATE_BUCKETS: DateBucket[] = ["today", "yesterday", "last7", "earlier"];

/** Session timestamps arrive as "YYYY-MM-DD HH:MM:SS" (or ISO); both parse as local time. */
function parseStamp(value: string | null | undefined): number | null {
  if (!value) return null;
  const ms = Date.parse(value.includes("T") ? value : value.replace(" ", "T"));
  return Number.isNaN(ms) ? null : ms;
}

function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function dateBucket(updatedAt: string | null | undefined, now: number = Date.now()): DateBucket {
  const ms = parseStamp(updatedAt);
  if (ms === null) return "earlier";
  const today = startOfDay(now);
  if (ms >= today) return "today";
  if (ms >= startOfDay(today - 1)) return "yesterday";
  const weekAgo = new Date(today);
  weekAgo.setDate(weekAgo.getDate() - 7);
  return ms >= weekAgo.getTime() ? "last7" : "earlier";
}

/** Keeps the incoming (already newest-first) order inside each bucket; empty buckets are dropped. */
export function groupByDate<T extends { updated_at?: string | null }>(
  items: T[],
  now: number = Date.now(),
): { bucket: DateBucket; items: T[] }[] {
  const map = new Map<DateBucket, T[]>();
  for (const item of items) {
    const bucket = dateBucket(item.updated_at, now);
    const list = map.get(bucket);
    if (list) list.push(item);
    else map.set(bucket, [item]);
  }
  return DATE_BUCKETS.filter((bucket) => map.has(bucket)).map((bucket) => ({ bucket, items: map.get(bucket)! }));
}

// ---- projects -------------------------------------------------------------------------------

export interface Project {
  id: string;
  name: string;
  collapsed?: boolean;
}

export interface ProjectStore {
  projects: Project[];
  /** session_id → project id. A session belongs to at most one project. */
  assign: Record<string, string>;
}

const KEY = "delta:projects:v1";
const EVENT = "delta:projects-changed";
const EMPTY: ProjectStore = { projects: [], assign: {} };

export function parseProjectStore(raw: string | null): ProjectStore {
  if (!raw) return EMPTY;
  try {
    const v = JSON.parse(raw);
    const projects: Project[] = Array.isArray(v?.projects)
      ? v.projects
          .filter((p: any) => p && typeof p.id === "string" && typeof p.name === "string" && p.name.trim())
          .map((p: any) => ({ id: p.id, name: String(p.name), ...(p.collapsed ? { collapsed: true } : {}) }))
      : [];
    const ids = new Set(projects.map((p) => p.id));
    const assign: Record<string, string> = {};
    if (v?.assign && typeof v.assign === "object") {
      for (const [sessionId, projectId] of Object.entries(v.assign)) {
        if (typeof projectId === "string" && ids.has(projectId)) assign[sessionId] = projectId;
      }
    }
    return { projects, assign };
  } catch {
    return EMPTY;
  }
}

function read(): ProjectStore {
  try {
    return parseProjectStore(localStorage.getItem(KEY));
  } catch {
    return EMPTY;
  }
}

function write(next: ProjectStore) {
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    /* private mode etc. — still applies for this session via the event below */
  }
  window.dispatchEvent(new CustomEvent(EVENT, { detail: next }));
}

/** Pure reducers, so the behaviour is testable without a DOM. */
export const projectOps = {
  create(store: ProjectStore, id: string, name: string): ProjectStore {
    const clean = name.trim();
    return clean ? { ...store, projects: [...store.projects, { id, name: clean }] } : store;
  },
  rename(store: ProjectStore, id: string, name: string): ProjectStore {
    const clean = name.trim();
    if (!clean) return store;
    return { ...store, projects: store.projects.map((project) => (project.id === id ? { ...project, name: clean } : project)) };
  },
  /** Removing a project only ungroups its sessions; nothing else is touched. */
  remove(store: ProjectStore, id: string): ProjectStore {
    const assign = Object.fromEntries(Object.entries(store.assign).filter(([, assignedId]) => assignedId !== id));
    return { projects: store.projects.filter((project) => project.id !== id), assign };
  },
  toggle(store: ProjectStore, id: string): ProjectStore {
    return { ...store, projects: store.projects.map((project) => (project.id === id ? { ...project, collapsed: !project.collapsed } : project)) };
  },
  move(store: ProjectStore, sessionId: string, projectId: string | null): ProjectStore {
    const assign = { ...store.assign };
    if (projectId && store.projects.some((project) => project.id === projectId)) assign[sessionId] = projectId;
    else delete assign[sessionId];
    return { ...store, assign };
  },
};

let counter = 0;
const newProjectId = () => `p_${Date.now().toString(36)}${(counter++).toString(36)}`;

export function useProjects() {
  const [store, setStore] = useState<ProjectStore>(read);

  useEffect(() => {
    const onChange = (e: Event) => setStore((e as CustomEvent<ProjectStore>).detail ?? read());
    const onStorage = (e: StorageEvent) => {
      if (e.key === KEY) setStore(read());
    };
    window.addEventListener(EVENT, onChange);
    window.addEventListener("storage", onStorage);
    return () => {
      window.removeEventListener(EVENT, onChange);
      window.removeEventListener("storage", onStorage);
    };
  }, []);

  const apply = useCallback((fn: (s: ProjectStore) => ProjectStore) => write(fn(read())), []);

  return {
    projects: store.projects,
    assign: store.assign,
    create: (name: string): string => {
      const id = newProjectId();
      apply((s) => projectOps.create(s, id, name));
      return id;
    },
    rename: (id: string, name: string) => apply((s) => projectOps.rename(s, id, name)),
    remove: (id: string) => apply((s) => projectOps.remove(s, id)),
    toggle: (id: string) => apply((s) => projectOps.toggle(s, id)),
    move: (sessionId: string, projectId: string | null) => apply((s) => projectOps.move(s, sessionId, projectId)),
  };
}
