import { useEffect, useRef, useState } from "react";
import {
  getConnectors,
  getRecentChannels,
  subscribeChannel,
  unsubscribeChannel,
  type RecentChannel,
  type Connector,
} from "../api";
import { workspaceChannels } from "../features/connectors/components/workspaceChatApi";
import { Icon } from "./Icon";
import { useI18n } from "@delta/i18n/I18nContext";

interface WorkspaceChatConnectorData {
  ui?: { detail?: string };
  workspaces?: { team_id: string; account: string }[];
}

// A workspace roster hit for the typeahead: type a channel NAME, we resolve the
// id through the active workspace-chat capability and compose its address.
interface RosterHit {
  address: string;
  name: string;
  workspace: string; // labeled only when >1 workspace is connected
  is_private: boolean;
  is_member: boolean;
}

// A channel input with a popover of recently-seen channels (the "recent list + type-the-id"
// picker). Free typing is allowed (a connector:channel address or a channel Copy-link URL). The
// popover is hand-rolled, NOT a <datalist>: WKWebView (the macOS desktop shell) doesn't render
// datalist suggestions at all, so the native path would silently show nothing on Mac.
export function ChannelPicker({
  value,
  onChange,
  recent,
  onSubmit,
  onPickName,
}: {
  value: string;
  onChange: (v: string) => void;
  recent: RecentChannel[];
  onSubmit?: () => void;
  // Fires when a pick RESOLVES a display name for the raw address — callers can echo the
  // human name (+ workspace) wherever they show the target (§25 consent line, summaries).
  onPickName?: (address: string, name: string, workspace?: string) => void;
}) {
  const [isOpen, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement | null>(null);
  const { t } = useI18n();
  useEffect(() => {
    if (!isOpen) return;
    const onDoc = (e: MouseEvent) => {
      if (wrap.current && !wrap.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [isOpen]);

  // Workspace-chat provider and workspaces are discovered from the capability manifest.
  // Older extension fixtures may not expose `ui.detail` yet, so the fallback is based only
  // on generic connector capabilities rather than a provider name.
  const [chatConnector, setChatConnector] = useState<string | null>(null);
  const [teams, setTeams] = useState<{ team_id: string; account: string }[] | null>(null);
  useEffect(() => {
    if (!isOpen || teams !== null) return;
    getConnectors()
      .then((cs) => {
        const chat = (
          cs.find(
            (c) => (c as Connector & WorkspaceChatConnectorData).ui?.detail === "workspace_chat",
          ) ?? cs.find((c) => c.connected && c.two_way && c.channels)
        ) as (Connector & WorkspaceChatConnectorData) | undefined;
        if (!chat?.connected) {
          setChatConnector(null);
          return setTeams([]);
        }
        setChatConnector(chat.name);
        setTeams(
          chat.workspaces?.length
            ? chat.workspaces.map((w) => ({ team_id: w.team_id, account: w.account }))
            : [{ team_id: "default", account: chat.account || "workspace" }],
        );
      })
      .catch(() => {
        setChatConnector(null);
        setTeams([]);
      });
  }, [isOpen, teams]);

  // Type a NAME → live roster suggestions (debounced; addresses/URLs skip the lookup).
  // `searching` keeps the wait VISIBLE because the first provider lookup may need to
  // populate a local roster cache before suggestions can be returned.
  const [roster, setRoster] = useState<RosterHit[]>([]);
  const [isSearching, setSearching] = useState(false);
  useEffect(() => {
    const name = value.trim().replace(/^#/, "");
    if (
      !isOpen ||
      !chatConnector ||
      !teams ||
      teams.length === 0 ||
      !name ||
      name.includes(":") ||
      name.includes("/")
    ) {
      setRoster([]);
      setSearching(false);
      return;
    }
    setSearching(true);
    const timer = setTimeout(async () => {
      const rows = await Promise.all(
        teams.map(async (tm) => {
          try {
            const r = await workspaceChannels(chatConnector, tm.team_id, name);
            return (r.ok ? r.channels || [] : []).map((c) => ({
              address:
                teams.length > 1
                  ? `${chatConnector}:${tm.team_id}/${c.id}`
                  : `${chatConnector}:${c.id}`,
              name: c.name,
              workspace: teams.length > 1 ? tm.account : "",
              is_private: c.is_private,
              is_member: c.is_member,
            }));
          } catch {
            return [] as RosterHit[];
          }
        }),
      );
      setRoster(rows.flat().slice(0, 12));
      setSearching(false);
    }, 250);
    return () => clearTimeout(timer);
  }, [value, isOpen, teams, chatConnector]);

  // Filter as the user types (name, address, or last-message text); full list on focus.
  const q = value.trim().toLowerCase();
  const options = recent.filter(
    (c) =>
      !q ||
      c.channel.toLowerCase().includes(q) ||
      (c.name || "").toLowerCase().includes(q) ||
      (c.last_text || "").toLowerCase().includes(q),
  );
  // Roster hits the recent list already covers would be duplicates — drop them.
  const seen = new Set(options.map((c) => c.channel));
  const lookups = roster.filter((r) => !seen.has(r.address));

  // Display ≠ value: the stored value stays the raw address, but at rest the input shows
  // the channel's NAME when we know it — from a pick, the recent list, or a roster hit.
  const [isFocused, setFocused] = useState(false);
  const [pickedName, setPickedName] = useState<Record<string, string>>({});
  const knownName =
    pickedName[value] ||
    recent.find((c) => c.channel === value)?.name ||
    roster.find((r) => r.address === value)?.name ||
    "";
  const display = !isFocused && knownName ? `#${knownName}` : value;

  const inputRef = useRef<HTMLInputElement | null>(null);

  return (
    <div className="relative flex-1 min-w-0" ref={wrap}>
      <input
        ref={inputRef}
        className="chan-input w-full"
        placeholder={t("connectors.channelPlaceholder")}
        value={display}
        title={value || undefined}
        onChange={(e) => {
          onChange(e.target.value);
          setOpen(true);
        }}
        onFocus={() => {
          setFocused(true);
          setOpen(true);
        }}
        onBlur={() => setFocused(false)}
        onKeyDown={(e) => {
          if (e.key === "Escape") setOpen(false);
          if (e.key === "Enter" && onSubmit) {
            setOpen(false);
            onSubmit();
          }
        }}
      />
      {isOpen && (options.length > 0 || lookups.length > 0 || isSearching) && (
        <div
          className="absolute left-0 right-0 top-full mt-1 z-40 rounded-xl border border-line bg-panel shadow-lg py-1 max-h-56 overflow-y-auto"
          role="listbox"
          data-testid="channel-suggestions"
        >
          {options.map((c) => (
            <button
              key={c.channel}
              role="option"
              className="block w-full text-left px-3 py-1.5 hover:bg-paper"
              onMouseDown={(e) => {
                e.preventDefault();
                onChange(c.channel);
                if (c.name) {
                  setPickedName((m) => ({ ...m, [c.channel]: c.name! }));
                  onPickName?.(c.channel, c.name);
                }
                setOpen(false);
                inputRef.current?.blur();
              }}
            >
              <span className="text-[12.5px] text-ink">
                {c.name ? `#${c.name}` : c.channel}
              </span>
              {c.name && <span className="ml-1.5 text-[11px] text-faint">{c.channel}</span>}
              {c.last_text && (
                <span className="block text-[11px] text-faint truncate">
                  {c.last_from ? `${c.last_from}: ` : ""}
                  {c.last_text}
                </span>
              )}
            </button>
          ))}
          {isSearching && lookups.length === 0 && (
            <div
              className="px-3 py-1.5 text-[12px] text-faint"
              data-testid="roster-searching"
            >
              {t("connectors.searchingChannels")}
            </div>
          )}
          {lookups.map((r) => (
            <button
              key={r.address}
              role="option"
              className="block w-full text-left px-3 py-1.5 hover:bg-paper"
              data-testid={`roster-channel-${r.address}`}
              onMouseDown={(e) => {
                e.preventDefault();
                onChange(r.address);
                if (r.name) {
                  setPickedName((m) => ({ ...m, [r.address]: r.name }));
                  onPickName?.(r.address, r.name, r.workspace || undefined);
                }
                setOpen(false);
                inputRef.current?.blur();
              }}
            >
              <span className="text-[12.5px] text-ink">
                {r.is_private ? "🔒 " : "#"}
                {r.name}
              </span>
              {r.workspace && (
                <span className="ml-1.5 text-[11px] text-faint">{r.workspace}</span>
              )}
              {!r.is_member && (
                <span className="block text-[11px] text-warnInk">
                  {t("connectors.inviteDelta")}
                </span>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function SubscriptionsChip({
  sessionId,
  channels,
  onChanged,
}: {
  sessionId: string;
  channels: string[];
  onChanged: () => void;
}) {
  const [isOpen, setOpen] = useState(false);
  const [recent, setRecent] = useState<RecentChannel[]>([]);
  const [draft, setDraft] = useState("");
  const ref = useRef<HTMLDivElement | null>(null);
  const { t } = useI18n();

  useEffect(() => {
    if (!isOpen) return;
    getRecentChannels().then(setRecent).catch(() => setRecent([]));
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [isOpen]);

  const add = async () => {
    const c = draft.trim();
    if (!c) return;
    await subscribeChannel(sessionId, c);
    setDraft("");
    onChanged();
  };
  const remove = async (c: string) => {
    await unsubscribeChannel(sessionId, c);
    onChanged();
  };

  return (
    <div className="sub-chip-wrap" ref={ref}>
      <button
        className={"wschip sub-chip" + (isOpen ? " active" : "")}
        title={t("connectors.channelsListening")}
        onClick={() => setOpen((v) => !v)}
      >
        <Icon name="plug" size={12} /> {channels.length || "+"}
      </button>
      {isOpen && (
        <div className="sub-pop" onMouseDown={(e) => e.stopPropagation()}>
          <div className="sub-pop-head">{t("connectors.channelsListening")}</div>
          {channels.length === 0 ? (
            <div className="dim sub-pop-empty">{t("connectors.notSubscribed")}</div>
          ) : (
            channels.map((c) => {
              const nm = recent.find((r) => r.channel === c)?.name;
              return (
              <div className="sub-pop-row" key={c}>
                <span className="sub-pop-chan" title={c}>{nm ? `#${nm}` : c}</span>
                <button className="sub-pop-x" title={t("connectors.unsubscribe")} onClick={() => remove(c)}>
                  ×
                </button>
              </div>
              );
            })
          )}
          <div className="sub-pop-add">
            <ChannelPicker value={draft} onChange={setDraft} recent={recent} onSubmit={add} />
            <button className="btn-primary sm" disabled={!draft.trim()} onClick={add}>
              {t("common.add")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
