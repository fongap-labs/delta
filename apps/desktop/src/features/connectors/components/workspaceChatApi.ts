import { directConnectorAction } from "../../../runtimeTransport";

export interface WorkspaceChatMember {
  id: string;
  name: string;
  handle: string;
  guest: boolean;
}

export interface WorkspaceChatChannel {
  id: string;
  name: string;
  is_private: boolean;
  is_member: boolean;
}

export interface WorkspaceChatStatus {
  mode: string;
  relay: {
    state: "live" | "reconnecting" | "offline";
    reconnects: number;
    last_event_at: number | null;
    last_error: string;
  };
  teams: Record<string, { token_ok: boolean }>;
}

export function workspaceDirectory(connector: string, workspaceId: string, q = "") {
  return directConnectorAction(connector, "directory", {
    team_id: workspaceId,
    q,
  }) as Promise<{ ok: boolean; error?: string; members?: WorkspaceChatMember[] }>;
}

export function workspaceChannels(connector: string, workspaceId: string, q = "") {
  return directConnectorAction(connector, "channels", {
    team_id: workspaceId,
    q,
  }) as Promise<{ ok: boolean; error?: string; channels?: WorkspaceChatChannel[] }>;
}

export function allowWorkspaceUser(
  connector: string,
  userId: string,
  workspaceId?: string | null,
  displayName?: string,
) {
  return directConnectorAction(connector, "allow_user", {
    user_id: userId,
    ...(workspaceId ? { team_id: workspaceId } : {}),
    ...(displayName ? { name: displayName } : {}),
  }) as Promise<{ ok: boolean; error?: string }>;
}

export function disallowWorkspaceUser(
  connector: string,
  userId: string,
  workspaceId?: string | null,
) {
  return directConnectorAction(connector, "disallow_user", {
    user_id: userId,
    ...(workspaceId ? { team_id: workspaceId } : {}),
  }) as Promise<{ ok: boolean; error?: string }>;
}

export function addApprovalOwner(
  connector: string,
  userId: string,
  displayName?: string,
) {
  return directConnectorAction(connector, "add_approval_owner", {
    user_id: userId,
    ...(displayName ? { name: displayName } : {}),
  }) as Promise<{ ok: boolean; error?: string }>;
}

export function removeApprovalOwner(connector: string, userId: string) {
  return directConnectorAction(connector, "remove_approval_owner", {
    user_id: userId,
  }) as Promise<{ ok: boolean; error?: string }>;
}

export function disconnectWorkspace(connector: string, workspaceId: string) {
  return directConnectorAction(connector, "disconnect_workspace", {
    team_id: workspaceId,
  }) as Promise<{ ok: boolean; error?: string; remaining_workspaces?: number }>;
}

export function resolveWorkspaceMessage(
  connector: string,
  itemId: string,
  action: "dismiss" | "allow" | "allow_deliver",
) {
  return directConnectorAction(connector, "resolve_unauthorized", {
    item_id: itemId,
    action,
  }) as Promise<{ ok: boolean; error?: string }>;
}

export function workspaceChannelPrefix(connector: string, workspaceId?: string | null): string {
  return workspaceId ? `${connector}:${workspaceId}/` : `${connector}:`;
}
