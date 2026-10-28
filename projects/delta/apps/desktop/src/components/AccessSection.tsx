// AccessSection — the connectors and folders this session can touch. It is the body of the rail's
// Permissions section (the former standalone "Access" section is folded into it):
//   · Connectors — Connected toggles (per-session mute), Recommended (connect-in-context), and the
//     two-way connectors' channels drill-down — the drawer's content, recut.
//   · Folders — the session's working directories (add/remove, RO/RW gate, branch).
// Owns its data (GET /v1/sessions/{id}/connections + the connector index). It mounts with the
// Permissions section, so opening that section refetches everything it shows.

import { useCallback, useEffect, useState, type Ref } from "react";
import {
  getConnectors,
  getRecentChannels,
  getSessionConnections,
  getSubscriptions,
  setSessionConnection,
  subscribeChannel,
  unsubscribeChannel,
  type Connector,
  type RecentChannel,
  type SessionConnections,
  type Subscription,
} from "../api";
import { ConnectorBadge } from "../features/connectors/ConnectorIcon";
import { indexConnectors, labelFor, visualFor, type ConnectorMap } from "../features/connectors/visuals";
import { useRoots } from "../useRoots";
import { AddFolderForm } from "./AddFolderForm";
import { Icon } from "./Icon";
import { ConnectSetup } from "./ManageTabs";
import { RootRow } from "./RootRow";
import { ChannelPicker } from "./SubscriptionsChip";
import { Toggle } from "./Toggle";
import { useI18n } from "@delta/i18n/I18nContext";

// A channel address's platform: "slack:C0123" → "slack"; a bare id or "#mention" defaults to
// slack (the backend's own default when no platform prefix is given).
const platformOf = (channel: string) => (channel.includes(":") ? channel.split(":")[0] : "slack");

const SEC_H = "text-[11px] uppercase tracking-[0.05em] text-faint font-semibold";
const TAG_CORE =
  "text-[10px] px-1.5 py-0.5 rounded-full bg-warn-soft/70 text-warn border border-warn/15";
const BTN_ACCENT = "text-[12px] px-2.5 py-1.5 rounded-lg bg-primary text-on-primary shrink-0";
const BTN_BORDERED =
  "text-[12px] px-2.5 py-1.5 rounded-lg border border-line bg-bg hover:border-line-strong shrink-0";

export function AccessSection({
  sessionId,
  branch,
  scratchPrimary,
  sectionRef,
  onOpenIntegrations,
}: {
  sessionId: string;
  branch?: string | null;
  scratchPrimary?: boolean;
  // The rail scrolls this into view on a deep link ("Configure ›", onboarding's Start-working).
  sectionRef?: Ref<HTMLDivElement>;
  onOpenIntegrations?: () => void;
}) {
  const { t } = useI18n();
  const [conns, setConns] = useState<SessionConnections | null>(null);
  const [byName, setByName] = useState<ConnectorMap>({});
  const { roots, isBusy: rootsBusy, error: rootsError, addRoot, toggleAccess, removeRoot } =
    useRoots(sessionId);

  const reload = useCallback(() => {
    getSessionConnections(sessionId)
      .then(setConns)
      .catch(() => setConns(null));
  }, [sessionId]);
  useEffect(() => {
    reload();
  }, [reload]);

  // The connector index feeds brand colors and gates the "Channels ·" links. It loads on every
  // mount, so a single failed fetch can't hide them for as long as the section stays open.
  useEffect(() => {
    let isLive = true;
    getConnectors()
      .then((list) => isLive && setByName(indexConnectors(list)))
      .catch(() => {});
    return () => {
      isLive = false;
    };
  }, []);

  // Child views (connect-in-context / channels drill-down) replace the section body inline.
  const [channelsFor, setChannelsFor] = useState<string | null>(null);
  const [connectFor, setConnectFor] = useState<Connector | null>(null);
  // "+ Add a source…" (§32 addendum): the FULL catalog in-session. The list shows on focus,
  // before any typing (FB-012: typing-to-see was a hidden step), and the query filters it
  // live; rich browsing (detail pages, connect states) stays on the global Connectors page.
  const [isAdding, setAdding] = useState(false);
  const [query, setQuery] = useState("");
  // The add flow guarantees the new source is live HERE: the user asked for it in this
  // session, so after the connect lands it is also enabled per-session explicitly.
  const [addedFrom, setAddedFrom] = useState<string | null>(null);
  // Folders mirrors Sources: flat rows + a quiet "+" link that expands the inline form.
  const [isAddingFolder, setAddingFolder] = useState(false);
  const [subs, setSubs] = useState<Subscription[]>([]);
  const [recent, setRecent] = useState<RecentChannel[]>([]);
  const [draft, setDraft] = useState("");
  const [addErr, setAddErr] = useState<string | null>(null);
  const loadSubs = () => getSubscriptions().then(setSubs).catch(() => setSubs([]));
  useEffect(() => {
    loadSubs();
    getRecentChannels().then(setRecent).catch(() => setRecent([]));
  }, []);

  const toggleSession = async (connector: string, isNext: boolean) => {
    await setSessionConnection(sessionId, connector, isNext);
    reload();
  };
  const channelsOf = (connector: string) =>
    subs.filter((s) => s.session_id === sessionId && platformOf(s.channel) === connector);
  const addChannel = async () => {
    const raw = draft.trim();
    if (!raw || !channelsFor) return;
    const channel = raw.includes(":") || raw.startsWith("#") ? raw : `${channelsFor}:${raw}`;
    const r = await subscribeChannel(sessionId, channel);
    if (!r.ok) {
      setAddErr(r.error || t("access.addChannelError", undefined, "Couldn't add that channel."));
      return;
    }
    setAddErr(null);
    setDraft("");
    loadSubs();
  };
  const removeChannel = async (channel: string) => {
    await unsubscribeChannel(sessionId, channel);
    loadSubs();
  };

  const connected = conns?.connected ?? [];
  const recommended = conns?.recommended ?? [];

  // Catalog list: available, not already in the Connected list (those have toggles above).
  // Empty query = the whole catalog (FB-012 — the list renders before any typing); a query
  // narrows it on title/name/aliases ("calendar" must surface Outlook, not just Google
  // Calendar). Alphabetical so filtering never reorders; the container height-caps it, so
  // no count cap here.
  const connectedSet = new Set(connected.map((c) => c.connector));
  const q = query.trim().toLowerCase();
  const results = Object.values(byName)
    .filter(
      (c) =>
        c.available &&
        !connectedSet.has(c.name) &&
        (!q ||
          c.title.toLowerCase().includes(q) ||
          c.name.toLowerCase().includes(q) ||
          (c.aliases ?? []).some((a) => a.toLowerCase().includes(q))),
    )
    .sort((a, b) => a.title.localeCompare(b.title));

  return (
    <div
      ref={sectionRef}
      className="mt-4 pt-3 border-t border-line"
      data-testid="access-section"
      role="region"
      aria-label={t("access.sessionAccessAria", undefined, "Session access")}
    >
      {connectFor ? (
        <ConnectInline
          c={connectFor}
          onDone={() => {
            const name = connectFor.name;
            setConnectFor(null);
            if (addedFrom === name) {
              // Added from THIS session's panel → also enable it here explicitly (a
              // catalog connector need not already have a session override).
              setAddedFrom(null);
              setSessionConnection(sessionId, name, true)
                .catch(() => {})
                .finally(reload);
              return;
            }
            reload();
          }}
          onBack={() => {
            setConnectFor(null);
            setAddedFrom(null);
          }}
        />
      ) : channelsFor ? (
        <ChannelsInline
          label={labelFor(channelsFor, byName)}
          channels={channelsOf(channelsFor)}
          recent={recent}
          draft={draft}
          onDraft={(v) => {
            setDraft(v);
            setAddErr(null);
          }}
          onAdd={addChannel}
          error={addErr}
          onRemove={removeChannel}
          onBack={() => setChannelsFor(null)}
        />
      ) : (
        <div className="space-y-4">
          {/* Sources — each toggle is a per-session override (mute for THIS session only). */}
          <div>
            <div className={`${SEC_H} mb-1.5`}>{t("access.sources", undefined, "Connectors")}</div>
            {connected.length === 0 && (
              <div className="text-[12px] text-faint py-0.5">
                {t("access.noConnectors", undefined, "No connectors enabled for this session.")}
              </div>
            )}
            <div className="space-y-1">
              {connected.map((c) => (
                <div className="flex items-center gap-2 py-1" key={c.connector}>
                  <ConnectorBadge connector={visualFor(c.connector, "connector", byName)} size={24} />
                  <div className="min-w-0 flex-1">
                    <div className="text-[12.5px] font-medium leading-tight truncate">
                      <span>{labelFor(c.connector, byName)}</span>
                      {c.detail && <span className="text-faint font-normal"> · {c.detail}</span>}
                    </div>
                    {byName[c.connector]?.channels && (
                      <button
                        className="inline-flex items-center gap-0.5 text-[11px] text-primary hover:underline"
                        onClick={() => {
                          setDraft("");
                          setChannelsFor(c.connector);
                        }}
                      >
                        {t("access.channelsCount", { n: channelsOf(c.connector).length }, `Channels · ${channelsOf(c.connector).length}`)}
                        <Icon name="chevronRight" size={10} />
                      </button>
                    )}
                  </div>
                  <Toggle
                    checked={c.enabled}
                    onChange={(isNext) => toggleSession(c.connector, isNext)}
                    title={t("access.toggleTitle", undefined, "Enabled for this session — tap to mute here")}
                  />
                </div>
              ))}
            </div>
            {connected.length > 0 && (
              <p className="text-[10.5px] text-faint mt-1 leading-snug">
                {t("access.offHint", undefined, "Off mutes it for ")}
                <b>{t("access.offHintEm", undefined, "this session only")}</b>
                {t("access.offHintTail", undefined, " — the connector stays connected.")}
              </p>
            )}
            {/* §32 addendum (owner ask 2026-07-13; FB-012): the catalog's long tail,
                in-session. A quiet row that becomes a typeahead: full list on focus,
                filter as you type. */}
            {isAdding ? (
              <div className="mt-1.5">
                <input
                  className="w-full px-2.5 py-1.5 rounded-lg border border-line bg-panel text-[12.5px] outline-none focus:border-primary"
                  placeholder={t("access.searchPlaceholder", undefined, "Search connectors…")}
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Escape") {
                      setAdding(false);
                      setQuery("");
                    }
                  }}
                  autoFocus
                  data-testid="access-add-search"
                />
                {results.length === 0 && (
                  // Also covers a failed/empty catalog fetch: an open picker must never
                  // be silently blank — point at the Connectors page either way.
                  <div className="text-[11.5px] text-faint mt-1.5 px-0.5">
                    {t("access.noMatch", undefined, "No match — see all on the Connectors page below.")}
                  </div>
                )}
                <div className="mt-1 max-h-64 overflow-y-auto">
                  {results.map((c) => (
                    <button
                      key={c.name}
                      className="w-full flex items-center gap-2 py-1.5 px-0.5 rounded-lg text-left hover:bg-bg"
                      data-testid={`access-add-${c.name}`}
                      onClick={() => {
                        setAdding(false);
                        setQuery("");
                        setAddedFrom(c.name);
                        setConnectFor(c);
                      }}
                    >
                      <ConnectorBadge connector={visualFor(c.name, "connector", byName)} size={22} />
                      <span className="min-w-0 flex-1">
                        <span className="block text-[12.5px] font-medium leading-tight">
                          {c.title}
                        </span>
                        <span className="block text-[11px] text-faint truncate">
                          {t("connectors." + c.name + ".blurb", undefined, c.blurb)}
                        </span>
                      </span>
                      <Icon name="chevronRight" size={11} className="text-faint shrink-0" />
                    </button>
                  ))}
                </div>
              </div>
            ) : (
              <button
                className="mt-1 text-[12px] text-primary hover:underline text-left"
                onClick={() => setAdding(true)}
                data-testid="access-add-source"
              >
                {t("access.addSource", undefined, "+ Add a connector…")}
              </button>
            )}
            {/* Lives with its list (tester ask 2026-07-26): each group's manage link sits
                directly under that group, not pooled at the section's bottom. */}
            <button
              className="mt-1.5 block text-[12px] text-primary font-medium hover:underline text-left"
              onClick={() => onOpenIntegrations?.()}
            >
              {t("access.manageAll", undefined, "Manage all connectors (global) →")}
            </button>
          </div>

          {recommended.length > 0 && (
            <div>
              <div className={`${SEC_H} mb-1.5`}>{t("access.recommended", undefined, "Recommended")}</div>
              <div className="space-y-1">
                {recommended.map((r) => (
                  <div className="flex items-center gap-2 py-1" key={r.connector}>
                    <ConnectorBadge connector={visualFor(r.connector, "connector", byName)} size={24} />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5 text-[12.5px] font-medium leading-tight">
                        <span className="truncate">{labelFor(r.connector, byName)}</span>
                        {r.tier === "core" && <span className={TAG_CORE}>{t("access.core", undefined, "core")}</span>}
                      </div>
                      <div className="text-[11px] text-faint truncate" title={r.reason}>
                        {r.reason}
                      </div>
                    </div>
                    <button
                      className={r.tier === "core" ? BTN_ACCENT : BTN_BORDERED}
                      onClick={() => {
                        // Connect IN CONTEXT when we ship this connector; unknown refs
                        // (no descriptor) still fall back to the global page.
                        const desc = byName[r.connector];
                        if (desc) setConnectFor(desc);
                        else onOpenIntegrations?.();
                      }}
                    >
                      {t("access.connect", undefined, "Connect")}
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Working directories — standing session config (§22/§23 lineage). Flat rows +
              a quiet "+" link, structurally identical to Sources (owner ask 2026-07-13:
              the old drawer's card wrapper read too heavy in the rail). */}
          <div data-testid="drawer-directories">
            <div className={`${SEC_H} mb-1.5`}>{t("access.folders", undefined, "Folders")}</div>
            <div className="-mx-1.5">
              {roots.map((r) => (
                <RootRow
                  key={r.path}
                  root={r}
                  busy={rootsBusy}
                  scratchPrimary={scratchPrimary}
                  branch={r.primary ? branch : undefined}
                  onToggle={toggleAccess}
                  onRemove={removeRoot}
                />
              ))}
            </div>
            {isAddingFolder ? (
              <div className="mt-1.5">
                <AddFolderForm
                  onAdd={addRoot}
                  busy={rootsBusy}
                  startOpen
                  onDismiss={() => setAddingFolder(false)}
                />
              </div>
            ) : (
              <button
                className="mt-1 text-[12px] text-primary hover:underline text-left"
                onClick={() => setAddingFolder(true)}
              >
                {t("access.addFolder", undefined, "+ Give access to a folder…")}
              </button>
            )}
            {rootsError && <div className="roots-err">{rootsError}</div>}
          </div>
        </div>
      )}
    </div>
  );
}

// Connect-in-context (§32 child view): the same ConnectSetup the global Connectors page uses,
// hosted inline in the section so connecting never navigates away. Managed connects complete
// out-of-band (browser → broker → sidecar), so poll until the connector flips.
function ConnectInline({
  c,
  onDone,
  onBack,
}: {
  c: Connector;
  onDone: () => void;
  onBack: () => void;
}) {
  const { t } = useI18n();
  useEffect(() => {
    const t = setInterval(async () => {
      try {
        const list = await getConnectors();
        if (list.find((x) => x.name === c.name)?.connected) onDone();
      } catch {
        /* poll again */
      }
    }, 2500);
    return () => clearInterval(t);
  }, [c.name, onDone]);

  return (
    <div>
      <button
        className="inline-flex items-center gap-1 text-[12px] text-faint hover:text-ink mb-2"
        onClick={onBack}
        aria-label={t("access.backToSources", undefined, "Back to connectors")}
      >
        <Icon name="arrowLeft" size={13} /> {t("access.connectTitle", { name: c.title }, `Connect ${c.title}`)}
      </button>
      {c.blurb && (
        <p className="text-[12px] text-muted mb-1 leading-relaxed">
          {t("connectors." + c.name + ".blurb", undefined, c.blurb)}
        </p>
      )}
      <div className="-mx-2">
        <ConnectSetup c={c} onConnected={onDone} />
      </div>
      {/* Scope semantics, stated once (owner ask 2026-07-13): connecting is account-level,
          the toggle above is what scopes it to a session. */}
      <p className="text-[10.5px] text-faint mt-2 leading-snug">
        {t("access.connectScope", { name: c.title }, `Connecting makes ${c.title} available to all your Delta agents — the toggle in this list controls just this session.`)}
      </p>
    </div>
  );
}

// The per-connector channels drill-down (§32 child view): which channels THIS session listens
// to on a two-way messaging connector (Slack/Telegram).
function ChannelsInline({
  label,
  channels,
  recent,
  draft,
  onDraft,
  onAdd,
  error,
  onRemove,
  onBack,
}: {
  label: string;
  channels: Subscription[];
  recent: RecentChannel[];
  draft: string;
  onDraft: (v: string) => void;
  onAdd: () => void;
  error?: string | null;
  onRemove: (channel: string) => void;
  onBack: () => void;
}) {
  const { t } = useI18n();
  return (
    <div>
      <button
        className="inline-flex items-center gap-1 text-[12px] text-faint hover:text-ink mb-2"
        onClick={onBack}
        aria-label={t("access.backToSources", undefined, "Back to connectors")}
      >
        <Icon name="arrowLeft" size={13} /> {t("access.channelsLabel", { label }, `${label} channels`)}
      </button>
      <div className={`${SEC_H} mb-1.5`}>
        {t("access.subscribedChannels", { n: channels.length }, `Subscribed channels · ${channels.length}`)}
      </div>
      {channels.length === 0 ? (
        <div className="text-[12px] text-faint py-0.5">
          {t("access.noChannelsYet", { label }, `Not listening to any ${label} channel yet.`)}
        </div>
      ) : (
        <div className="space-y-1">
          {channels.map((s) => (
            <div className="flex items-center gap-1.5 py-1" key={s.channel}>
              <Icon name="plug" size={13} className="text-muted shrink-0" />
              <span className="min-w-0 flex-1 text-[12.5px] truncate" title={s.channel}>
                {s.channel_name ? `#${s.channel_name}` : s.channel}
              </span>
              {s.collision && (
                <span
                  className="text-[10.5px] text-warn bg-warn-soft/70 border border-warn/15 rounded px-1 shrink-0"
                  title={t("access.collisionTitle", undefined, "This channel is also this session's Inbox-routing target — inbound and outbound collide.")}
                >
                  ⚠
                </span>
              )}
              <button
                className="w-5 h-5 grid place-items-center text-faint hover:text-danger shrink-0"
                title={t("access.stopListening", undefined, "Stop listening")}
                onClick={() => onRemove(s.channel)}
              >
                ×
              </button>
            </div>
          ))}
        </div>
      )}
      <div className={`${SEC_H} mt-3 mb-1.5`}>{t("access.addChannel", undefined, "Add a channel")}</div>
      <div className="flex items-center gap-1.5">
        <ChannelPicker value={draft} onChange={onDraft} recent={recent} onSubmit={onAdd} />
        <button className={BTN_ACCENT} disabled={!draft.trim()} onClick={onAdd}>
          {t("access.add", undefined, "Add")}
        </button>
      </div>
      {error && (
        <p className="text-[11px] text-warn mt-1.5 leading-snug" data-testid="channel-add-error">
          {error}
        </p>
      )}
      <p className="text-[10.5px] text-faint mt-1.5 leading-snug">
        {t("access.channelsHint", undefined, "The agent receives messages posted to these channels. Removing one stops this session from listening — the connector stays connected.")}
      </p>
    </div>
  );
}
