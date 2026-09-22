import { useEffect, useRef, useState } from "react";
import {
  getSubscriptions,
  unsubscribeChannel,
  type Connector,
  type ParkedMessage,
  type Subscription,
} from "../../../api";
import { ConnectorBadge } from "../ConnectorIcon";
import { AddConnectionModal } from "./AddConnectionModal";
import type { DetailProps } from "./ConnectorsSection";
import { ToolsDisclosure } from "./ToolsDisclosure";
import {
  addApprovalOwner,
  allowWorkspaceUser,
  disconnectWorkspace,
  disallowWorkspaceUser,
  removeApprovalOwner,
  resolveWorkspaceMessage,
  workspaceChannelPrefix,
  workspaceDirectory,
  type WorkspaceChatMember,
} from "./workspaceChatApi";
import { FOOT, GRP, GRP_H, PILL_ACCENT, PILL_LINE, ROW, XBTN } from "./ui";
import { useI18n } from "@delta/i18n/I18nContext";

const LABEL = "text-[12.5px] text-muted w-24 shrink-0";

interface WorkspaceRow {
  team_id: string;
  account: string;
  domain?: string;
  allowed_users: string[];
  allow_all: boolean;
  allowed_user_names?: Record<string, string | null>;
  approval_owner_ids?: string[];
  approval_owner_names?: Record<string, string | null>;
  installer_user_id?: string;
  installer_name?: string;
}

interface WorkspaceChatConnectorData {
  workspaces?: WorkspaceRow[];
  approval_owner_ids?: string[];
  approval_owner_names?: Record<string, string | null>;
}

function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

export function WorkspaceChatDetail({ c, onChanged }: DetailProps) {
  const { t } = useI18n();
  const data = c as unknown as Connector & WorkspaceChatConnectorData;
  const workspaces = data.workspaces ?? [];
  const [isAdding, setAdding] = useState(false);
  const [subs, setSubs] = useState<Subscription[]>([]);
  const loadSubs = () => getSubscriptions().then(setSubs).catch(() => setSubs([]));

  useEffect(() => {
    loadSubs();
  }, [c.name]);

  const changed = () => {
    onChanged();
    loadSubs();
  };

  return (
    <div data-testid="workspace-chat-detail">
      <div className="flex items-center gap-3.5 mb-5">
        <ConnectorBadge connector={c} size={44} title={c.title} />
        <div className="min-w-0 flex-1">
          <h2 className="text-[20px] font-semibold tracking-tight leading-tight">{c.title}</h2>
          <div className="text-[12.5px] text-muted flex items-center gap-1.5">
            {c.connected ? (
              <>
                <span className="w-2 h-2 rounded-full bg-ok" />
                <span>{t("connectors.connected")}</span>
              </>
            ) : (
              <span>{t("connectors.notConnected")}</span>
            )}
          </div>
        </div>
        {!c.connected && (
          <button className={PILL_ACCENT} data-testid="add-workspace-btn" onClick={() => setAdding(true)}>
            {t("connectors.addWorkspace")}
          </button>
        )}
      </div>

      {workspaces.map((workspace) => (
        <WorkspaceGroup
          key={workspace.team_id}
          c={c}
          workspace={workspace}
          subs={subs}
          onChanged={changed}
        />
      ))}

      {c.connected && workspaces.length === 0 && (
        <div data-testid="workspace-chat-manual-card">
          <div className={GRP_H}>{c.account || t("connectors.workspace")}</div>
          <div className={GRP}>
            <PeopleRow
              connector={c.name}
              allowed={c.allowed_users}
              names={c.allowed_user_names}
              protectedIds={c.approval_owner_ids}
              workspaceId={null}
              onChanged={changed}
            />
            <ApprovalOwnersRow
              connector={c.name}
              owners={data.approval_owner_ids ?? []}
              names={data.approval_owner_names}
              editable
              onChanged={changed}
            />
            {(c.unauthorized ?? [])
              .filter((message) => !message.team_id)
              .map((message) => (
                <WaitingRow key={message.id} connector={c.name} message={message} onChanged={changed} />
              ))}
            <ListeningRows
              subs={subs.filter((sub) =>
                sub.channel.startsWith(workspaceChannelPrefix(c.name)) && !sub.channel.includes("/"),
              )}
              onChanged={changed}
            />
          </div>
        </div>
      )}

      <ToolsDisclosure c={c} onChanged={onChanged} />
      {c.connected && <div className={FOOT + " mt-2"}>{t("connectors.directoryNote")}</div>}

      {isAdding && (
        <AddConnectionModal
          c={c}
          title={t("connectors.addWorkspace")}
          onClose={() => setAdding(false)}
          onChanged={changed}
        />
      )}
    </div>
  );
}

function WorkspaceGroup({
  c,
  workspace,
  subs,
  onChanged,
}: {
  c: Connector;
  workspace: WorkspaceRow;
  subs: Subscription[];
  onChanged: () => void;
}) {
  const [isBusy, setBusy] = useState(false);
  const parked = (c.unauthorized ?? []).filter((message) => message.team_id === workspace.team_id);
  const listening = subs.filter((sub) =>
    sub.channel.startsWith(workspaceChannelPrefix(c.name, workspace.team_id)),
  );

  const disconnect = async () => {
    setBusy(true);
    try {
      await disconnectWorkspace(c.name, workspace.team_id);
      onChanged();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div data-testid={`workspace-chat-workspace-${workspace.team_id}`}>
      <div className={GRP_H + " flex items-center gap-2"}>
        <span>
          {workspace.account || workspace.team_id}{" "}
          <span className="font-normal text-faint" title={workspace.team_id}>
            · {workspace.domain || workspace.team_id}
          </span>
        </span>
      </div>
      <div className={GRP}>
        <PeopleRow
          connector={c.name}
          allowed={workspace.allowed_users}
          names={workspace.allowed_user_names}
          protectedIds={workspace.approval_owner_ids}
          workspaceId={workspace.team_id}
          installerId={workspace.installer_user_id}
          installerName={workspace.installer_name}
          onChanged={onChanged}
        />
        <ApprovalOwnersRow
          connector={c.name}
          owners={workspace.approval_owner_ids ?? []}
          names={workspace.approval_owner_names}
          installerId={workspace.installer_user_id}
          installerName={workspace.installer_name}
          editable={false}
          onChanged={onChanged}
        />
        {parked.map((message) => (
          <WaitingRow key={message.id} connector={c.name} message={message} onChanged={onChanged} />
        ))}
        <ListeningRows subs={listening} onChanged={onChanged} />
        <div className={ROW}>
          <span className="flex-1" />
          <DisconnectButton workspaceId={workspace.team_id} isBusy={isBusy} onClick={disconnect} />
        </div>
      </div>
    </div>
  );
}

function DisconnectButton({ workspaceId, isBusy, onClick }: { workspaceId: string; isBusy: boolean; onClick: () => void }) {
  const { t } = useI18n();
  return (
    <button
      className="text-[12.5px] text-danger/80 hover:text-danger shrink-0"
      data-testid={`disconnect-workspace-${workspaceId}`}
      title={t("connectors.disconnectWorkspaceTitle")}
      onClick={onClick}
      disabled={isBusy}
    >
      {isBusy ? t("connectors.disconnecting") : t("connectors.disconnectWorkspace")}
    </button>
  );
}

function PeopleRow({
  connector,
  allowed,
  names,
  protectedIds,
  workspaceId,
  installerId,
  installerName,
  onChanged,
}: {
  connector: string;
  allowed: string[];
  names?: Record<string, string | null>;
  protectedIds?: string[];
  workspaceId: string | null;
  installerId?: string;
  installerName?: string;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const label = (userId: string) =>
    names?.[userId] || (userId === installerId ? installerName || t("connectors.you") : userId);

  return (
    <div className={ROW}>
      <span className={LABEL}>{t("connectors.people")}</span>
      <span className="min-w-0 flex-1 flex flex-wrap items-center gap-1.5">
        {allowed.length === 0 && <span className="text-[12px] text-faint">{t("connectors.nobodyYetPick")}</span>}
        {allowed.map((userId) => (
          <span
            key={userId}
            className="inline-flex items-center gap-1.5 pl-1 pr-2 py-0.5 rounded-full bg-paper border border-line text-[12.5px]"
            title={t("connectors.id", { value: userId })}
          >
            <span className="w-5 h-5 rounded-full bg-accentSoft text-accent grid place-items-center text-[9px] font-bold">
              {initials(label(userId))}
            </span>
            {label(userId)}
            {protectedIds?.includes(userId) ? (
              <span className="text-[10.5px] text-faint">{t("connectors.ownerTag")}</span>
            ) : (
              <button
                className={XBTN}
                title={t("common.remove")}
                onClick={async () => {
                  await disallowWorkspaceUser(connector, userId, workspaceId);
                  onChanged();
                }}
              >
                ×
              </button>
            )}
          </span>
        ))}
        <PersonPicker connector={connector} workspaceId={workspaceId} allowed={allowed} onChanged={onChanged} />
      </span>
    </div>
  );
}

function PersonPicker({
  connector,
  workspaceId,
  allowed,
  onChanged,
  onPick,
  buttonLabel,
  testId,
}: {
  connector: string;
  workspaceId: string | null;
  allowed: string[];
  onChanged: () => void;
  onPick?: (member: WorkspaceChatMember) => Promise<{ ok: boolean; error?: string }>;
  buttonLabel?: string;
  testId?: string;
}) {
  const { t } = useI18n();
  const [isOpen, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [rows, setRows] = useState<WorkspaceChatMember[]>([]);
  const [error, setError] = useState<string | null>(null);
  const wrap = useRef<HTMLSpanElement | null>(null);
  const button = useRef<HTMLButtonElement | null>(null);
  const [position, setPosition] = useState<{ top: number; left: number } | null>(null);

  const toggle = () => {
    if (isOpen) return setOpen(false);
    const rect = button.current?.getBoundingClientRect();
    setPosition(rect ? { top: rect.bottom + 4, left: Math.min(rect.left, window.innerWidth - 300) } : null);
    setOpen(true);
  };

  useEffect(() => {
    if (!isOpen) return;
    const timer = setTimeout(() => {
      workspaceDirectory(connector, workspaceId || "default", query)
        .then((result) => {
          if (result.ok) {
            setRows(result.members ?? []);
            setError(null);
          } else {
            setError(result.error || t("connectors.directoryUnavailable"));
          }
        })
        .catch(() => setError(t("connectors.directoryUnavailable")));
    }, 200);
    return () => clearTimeout(timer);
  }, [connector, isOpen, query, t, workspaceId]);

  useEffect(() => {
    if (!isOpen) return;
    const onDocumentClick = (event: MouseEvent) => {
      if (wrap.current && !wrap.current.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDocumentClick);
    return () => document.removeEventListener("mousedown", onDocumentClick);
  }, [isOpen]);

  const pick = async (member: WorkspaceChatMember) => {
    const result = onPick
      ? await onPick(member)
      : await allowWorkspaceUser(connector, member.id, workspaceId, member.name);
    if (result.ok === false) {
      setError(result.error || t("connectors.couldNotAddPerson"));
      return;
    }
    setOpen(false);
    setQuery("");
    onChanged();
  };

  const candidates = rows.filter((member) => !allowed.includes(member.id));

  return (
    <span className="relative" ref={wrap}>
      <button
        ref={button}
        className="inline-flex items-center px-2 py-0.5 rounded-full border border-dashed border-line text-[12.5px] text-muted hover:text-ink hover:border-faint"
        data-testid={testId || `add-person-${workspaceId || "default"}`}
        title={t("connectors.pickFromDirectory")}
        onClick={toggle}
      >
        {buttonLabel ?? t("connectors.addPerson")}
      </button>
      {isOpen && (
        <div
          className="fixed z-50 w-72 rounded-xl border border-line bg-panel shadow-lg p-1"
          style={{ top: position?.top, left: position?.left }}
          data-testid="person-picker"
        >
          <input
            autoFocus
            className="w-full bg-paper border border-line rounded-lg px-2 py-1 text-[12.5px] outline-none placeholder:text-faint"
            placeholder={t("connectors.typeNamePlaceholder")}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") setOpen(false);
            }}
          />
          <div className="max-h-56 overflow-y-auto py-1">
            {error ? (
              <div className="px-2 py-1.5 text-[12px] text-warnInk">{error}</div>
            ) : candidates.length === 0 ? (
              <div className="px-2 py-1.5 text-[12px] text-faint">{t("connectors.noMatches")}</div>
            ) : (
              candidates.map((member) => (
                <button
                  key={member.id}
                  className="block w-full text-left px-2 py-1.5 rounded-lg hover:bg-paper"
                  data-testid={`pick-person-${member.id}`}
                  title={t("connectors.id", { value: member.id })}
                  onMouseDown={(event) => {
                    event.preventDefault();
                    void pick(member);
                  }}
                >
                  <span className="text-[12.5px] font-medium">{member.name}</span>{" "}
                  <span className="text-[11.5px] text-faint">@{member.handle}</span>
                  {member.guest && (
                    <span className="ml-1 text-[10.5px] text-faint">{t("connectors.guest")}</span>
                  )}
                </button>
              ))
            )}
          </div>
        </div>
      )}
    </span>
  );
}

function ApprovalOwnersRow({
  connector,
  owners,
  names,
  installerId,
  installerName,
  editable,
  onChanged,
}: {
  connector: string;
  owners: string[];
  names?: Record<string, string | null>;
  installerId?: string;
  installerName?: string;
  editable: boolean;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const [error, setError] = useState<string | null>(null);
  const label = (userId: string) =>
    names?.[userId] || (userId === installerId ? installerName || t("connectors.you") : userId);

  const remove = async (userId: string) => {
    const result = await removeApprovalOwner(connector, userId);
    if (!result.ok) {
      setError(result.error || t("connectors.couldNotRemoveOwner"));
      return;
    }
    setError(null);
    onChanged();
  };

  return (
    <div className={ROW} data-testid="workspace-chat-approval-owners">
      <span className={LABEL}>{t("connectors.approvals")}</span>
      <span className="min-w-0 flex-1 flex flex-wrap items-center gap-1.5">
        {owners.length === 0 && <span className="text-[12px] text-warnInk">{t("connectors.chooseApprovalOwner")}</span>}
        {owners.map((userId) => (
          <span
            key={userId}
            className="inline-flex items-center gap-1.5 pl-1 pr-2 py-0.5 rounded-full bg-paper border border-line text-[12.5px]"
            data-testid={`approval-owner-${userId}`}
          >
            <span className="w-5 h-5 rounded-full bg-accentSoft text-accent grid place-items-center text-[9px] font-bold">
              {initials(label(userId))}
            </span>
            {label(userId)}
            {editable && (
              <button className={XBTN} title={t("connectors.removeApprovalOwner")} onClick={() => void remove(userId)}>
                ×
              </button>
            )}
          </span>
        ))}
        {editable && (
          <PersonPicker
            connector={connector}
            workspaceId={null}
            allowed={owners}
            onChanged={onChanged}
            onPick={(member) => addApprovalOwner(connector, member.id, member.name)}
            buttonLabel={t("connectors.addOwner")}
            testId="add-approval-owner"
          />
        )}
        {error && <span className="basis-full text-[11.5px] text-warnInk">{error}</span>}
      </span>
    </div>
  );
}

function WaitingRow({
  connector,
  message,
  onChanged,
}: {
  connector: string;
  message: ParkedMessage;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const act = async (action: "dismiss" | "allow" | "allow_deliver") => {
    await resolveWorkspaceMessage(connector, message.id, action);
    onChanged();
  };

  return (
    <div className={ROW + " bg-warnSoft/25"} data-testid={`waiting-${message.id}`}>
      <span className={LABEL}>{t("connectors.waiting")}</span>
      <span className="min-w-0 flex-1">
        <span className="font-medium text-[13px]">{message.user_name || message.user_id}</span>{" "}
        <span className="text-[12.5px] text-muted">
          {t("connectors.inChannel", { name: message.chat_name || message.chat_id })}
        </span>
        <span className="block text-[12.5px] text-muted truncate">“{message.text}”</span>
      </span>
      <button
        className={PILL_ACCENT + " !py-1"}
        data-testid={`workspace-chat-allow-deliver-${message.id}`}
        onClick={() => void act("allow_deliver")}
      >
        {t("connectors.allowDeliver")}
      </button>
      <button
        className={PILL_LINE + " !py-1"}
        data-testid={`workspace-chat-allow-${message.id}`}
        onClick={() => void act("allow")}
      >
        {t("common.allow")}
      </button>
      <button
        className={XBTN + " px-1"}
        data-testid={`workspace-chat-dismiss-${message.id}`}
        title={t("common.dismiss")}
        onClick={() => void act("dismiss")}
      >
        ×
      </button>
    </div>
  );
}

function ListeningRows({ subs, onChanged }: { subs: Subscription[]; onChanged: () => void }) {
  const { t } = useI18n();
  if (subs.length === 0) return null;
  return (
    <div className={ROW} data-testid="workspace-chat-listening">
      <span className={LABEL}>{t("connectors.listening")}</span>
      <span className="min-w-0 flex-1 space-y-1">
        {subs.map((sub) => (
          <span key={sub.session_id + sub.channel} className="flex items-center gap-2 text-[12.5px]">
            <span className="font-medium truncate" title={sub.session_id}>
              {sub.session_title || sub.session_id}
            </span>
            <span className="text-faint">←</span>
            <span className="text-muted truncate" title={sub.channel}>
              {sub.channel_name ? `#${sub.channel_name}` : sub.channel}
            </span>
            <button
              className={XBTN + " ml-auto"}
              title={t("connectors.unsubscribeSession")}
              onClick={async () => {
                await unsubscribeChannel(sub.session_id, sub.channel);
                onChanged();
              }}
            >
              ×
            </button>
          </span>
        ))}
      </span>
    </div>
  );
}