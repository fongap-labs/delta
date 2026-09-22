// R6 native bridge: React -> Tauri invoke/listen -> Rust Runtime.
// Commands use `invoke`; runtime events use `listen("delta-runtime-event")`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { RuntimeEventEnvelopeV1 } from "./runtime-contract";

/** The Tauri event name carrying runtime event frames. */
const RUNTIME_EVENT_CHANNEL = "delta-runtime-event";

export interface DirectRuntimeAcceptance {
  ok: boolean;
  accepted?: boolean;
  runId?: string;
  state?: string;
  error?: string;
}

// -----------------------------------------------------------------------------
// Runtime controls (agent loop verbs)
// -----------------------------------------------------------------------------

export async function directRun(args: {
  sessionId: string;
  modelId: string;
  userInput: string;
  workspace?: string;
  attachments?: unknown[];
  skill?: string;
  mode?: string;
  maxIterations?: number;
  maxRetries?: number;
  source?: unknown;
  onEvent: (event: WsEventEnvelope) => void;
  onError?: (error: unknown) => void;
}): Promise<DirectRuntimeAcceptance> {
  const {
    sessionId, modelId, userInput,
    workspace, attachments, skill, mode, maxIterations, maxRetries, source,
  } = args;
  try {
    const out = await invoke("runtime_run", {
      sessionId, modelId, userInput,
      workspace, attachments, skill, mode, maxIterations, maxRetries, source,
    });
    if (out && (out as any).error) {
      args.onError?.((out as any).error);
      return { ok: false, error: (out as any).error };
    }
    return {
      ok: true,
      accepted: (out as any).accepted === true,
      runId: typeof (out as any).runId === "string" ? (out as any).runId : undefined,
      state: typeof (out as any).state === "string" ? (out as any).state : undefined,
    };
  } catch (e) {
    args.onError?.(e);
    return { ok: false, error: String(e) };
  }
}

export async function directResume(
  sessionId: string,
): Promise<DirectRuntimeAcceptance> {
  try {
    const out = await invoke("runtime_resume", { sessionId });
    if (out && (out as any).error) return { ok: false, error: (out as any).error };
    return {
      ok: true,
      accepted: (out as any).accepted === true,
      runId: typeof (out as any).runId === "string" ? (out as any).runId : undefined,
    };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function directRetry(
  sessionId: string,
): Promise<DirectRuntimeAcceptance> {
  try {
    const out = await invoke("runtime_retry", { sessionId });
    if (out && (out as any).error) return { ok: false, error: (out as any).error };
    return {
      ok: true,
      accepted: (out as any).accepted === true,
      runId: typeof (out as any).runId === "string" ? (out as any).runId : undefined,
    };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function directSteer(
  sessionId: string,
  text: string,
  source?: unknown,
): Promise<DirectRuntimeAcceptance> {
  try {
    const out = await invoke("runtime_steer", { sessionId, text, source });
    if (out && (out as any).error) return { ok: false, error: (out as any).error };
    return {
      ok: true,
      accepted: (out as any).accepted === true,
      runId: typeof (out as any).runId === "string" ? (out as any).runId : undefined,
    };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function directFollowUp(
  sessionId: string,
  text: string,
  source?: unknown,
): Promise<DirectRuntimeAcceptance> {
  try {
    const out = await invoke("runtime_follow_up", { sessionId, text, source });
    if (out && (out as any).error) return { ok: false, error: (out as any).error };
    return {
      ok: true,
      accepted: (out as any).accepted === true,
      runId: typeof (out as any).runId === "string" ? (out as any).runId : undefined,
    };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function directCancel(
  sessionId: string,
): Promise<{ ok: boolean; error?: string }> {
  try {
    const out = await invoke("runtime_cancel", { sessionId });
    if (out && (out as any).error) return { ok: false, error: (out as any).error };
    return { ok: true };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function directMessages(
  sessionId: string,
): Promise<unknown[]> {
  try {
    const out = await invoke("runtime_messages", { sessionId });
    return (out && (out as any).messages) || [];
  } catch {
    return [];
  }
}

export async function directSwitchModel(
  sessionId: string,
  modelId: string,
): Promise<{ ok: boolean; error?: string; notice?: unknown }> {
  try {
    const out = await invoke("runtime_switch_model", { sessionId, modelId });
    if (out && (out as any).error) return { ok: false, error: (out as any).error };
    return { ok: true, notice: (out as any).notice };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function directTruncate(
  sessionId: string,
  index: number,
): Promise<{ ok: boolean; error?: string }> {
  try {
    const out = await invoke("runtime_truncate", { sessionId, index });
    if (out && (out as any).error) return { ok: false, error: (out as any).error };
    return { ok: true };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function directHealth(): Promise<unknown> {
  try {
    return await invoke("health");
  } catch (e) {
    return { status: "error", error: String(e) };
  }
}

export async function directApproval(
  sessionId: string,
  decision: string,
  toolCallId?: string,
): Promise<{ ok: boolean; error?: string; toolCallId?: string }> {
  try {
    const out = await invoke("runtime_approval", { sessionId, decision, toolCallId });
    if (out && (out as any).error) return { ok: false, error: (out as any).error };
    return {
      ok: true,
      toolCallId: typeof (out as any).toolCallId === "string" ? (out as any).toolCallId : undefined,
    };
  } catch (error) {
    return { ok: false, error: String(error) };
  }
}

export const directSetMode = (sessionId: string, mode: string) =>
  invokeAuthority("runtime_set_mode", { sessionId, mode });
export const resolveDirectory = (
  sessionId: string,
  isGranted: boolean,
  path?: string,
  canWrite = false,
) => invokeAuthority("resolve_directory_request", { sessionId, isGranted, path, canWrite });
export const resolvePlan = (
  sessionId: string,
  isApproved: boolean,
  mode?: string,
  feedback?: string,
) => invokeAuthority("resolve_plan_request", { sessionId, isApproved, mode, feedback });
export const resolveQuestion = (sessionId: string, answer: string) =>
  invokeAuthority("resolve_question_request", { sessionId, answer });

async function invokeAuthority(command: string, args: Record<string, unknown> = {}): Promise<any> {
  try {
    return await invoke(command, args);
  } catch (error) {
    return { ok: false, error: String(error) };
  }
}

export const directGetSettings = () => invokeAuthority("settings_get");
export const updateModelKey = (apiKey: string) =>
  invokeAuthority("update_model_key", { apiKey });
export const updateModelDefault = (modelId: string) =>
  invokeAuthority("update_model_default", { modelId });
export const directAddModel = (modelId: string) =>
  invokeAuthority("settings_add_model", { modelId });
export const directRemoveModel = (modelId: string) =>
  invokeAuthority("settings_remove_model", { modelId });
export const directSetOnboarded = (isOnboarded: boolean) =>
  invokeAuthority("settings_set_onboarded", { isOnboarded });
export const directSetLanguage = (language: string) =>
  invokeAuthority("settings_set_language", { language });
export const updateContextBar = (isShown: boolean) =>
  invokeAuthority("update_context_bar", { isShown });
export const updateSessionPeek = (count: number) =>
  invokeAuthority("update_session_peek", { count });
export const updateScratchBase = (path: string) =>
  invokeAuthority("update_scratch_base", { path });
export const updatePdfSettings = (patch: Record<string, unknown>) =>
  invokeAuthority("update_pdf_settings", { patch });
export const updateCompaction = (patch: Record<string, unknown>) =>
  invokeAuthority("update_compaction", { patch });
export const directGetProviders = () => invokeAuthority("providers_list");
export const directGetProtocols = () => invokeAuthority("provider_protocols");
export const directSetProvider = (
  name: string,
  fields: Record<string, string>,
  protocol?: string,
) => invokeAuthority("provider_set", { name, fields, protocol });
export const directRemoveProvider = (name: string) =>
  invokeAuthority("provider_remove", { name });
export const fetchProviderModels = (name: string, fields: Record<string, string>) =>
  invokeAuthority("fetch_provider_models", { name, fields });
export const directVerifyProvider = (name: string, fields: Record<string, string>) =>
  invokeAuthority("provider_verify", { name, fields });

// -----------------------------------------------------------------------------
// Runtime event subscription
// -----------------------------------------------------------------------------

/** A raw runtime event envelope as delivered by Tauri events. */
export interface WsEventEnvelope extends RuntimeEventEnvelopeV1<Record<string, unknown>> {}

/**
 * Subscribe to a session's runtime events via Tauri events.
 * Returns an unsubscribe function.
 */
export function directListenSession(
  sessionId: string,
  onEvent: (event: WsEventEnvelope) => void,
): () => void {
  let isDisposed = false;
  let unlisten: (() => void) | undefined;
  listen<WsEventEnvelope>(RUNTIME_EVENT_CHANNEL, (event) => {
    if (isDisposed) return;
    const payload = event.payload as WsEventEnvelope;
    // Filter to this session only.
    if (payload && payload.sessionId === sessionId) {
      onEvent(payload);
    }
  }).then((fn) => {
    if (isDisposed && fn) fn();
    else if (fn) unlisten = fn;
  });
  return () => {
    isDisposed = true;
    unlisten?.();
  };
}

const RUNTIME_SESSION_EVENT_TYPES = new Set<string>([
  "ready",
  "inbound",
  "turn_start",
  "assistant_delta",
  "reasoning_delta",
  "assistant_message",
  "tool_proposed",
  "permission_required",
  "directory_requested",
  "question_requested",
  "plan_proposed",
  "tool_started",
  "tool_finished",
  "iteration_end",
  "turn_end",
  "error",
  "input_rejected",
  "interrupted",
  "model_changed",
  "memory_saved",
  "compacting",
  "compacted",
  "turn_done",
]);

export { RUNTIME_SESSION_EVENT_TYPES };

// -----------------------------------------------------------------------------
// R6 Application Control Plane — Session/Workspace via direct IPC.
// Mirrors the removed FastAPI /v1/sessions + /v1/workspaces read/write shapes so
// api.ts callers can switch to IPC unchanged.
// -----------------------------------------------------------------------------

export async function directListSessions(workspace?: string): Promise<unknown> {
  try {
    return await invoke("sessions_list", workspace ? { workspace } : {});
  } catch (e) {
    return { sessions: [], error: String(e) };
  }
}

export async function directSessionMessages(sessionId: string): Promise<unknown> {
  try {
    return await invoke("session_messages", { sessionId });
  } catch (e) {
    return { messages: [], error: String(e) };
  }
}

export async function directSessionRename(
  sessionId: string,
  title: string,
): Promise<unknown> {
  try {
    return await invoke("session_rename", { sessionId, title });
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function updateSessionFlags(
  sessionId: string,
  flags: { pinned?: boolean; archived?: boolean },
): Promise<unknown> {
  try {
    return await invoke("update_session_flags", {
      sessionId,
      isPinned: flags.pinned,
      isArchived: flags.archived,
    });
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function directSessionDelete(sessionId: string): Promise<unknown> {
  try {
    return await invoke("session_delete", { sessionId });
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function directRecentWorkspaces(): Promise<unknown> {
  try {
    return await invoke("workspaces_recent");
  } catch (e) {
    return { workspaces: [], error: String(e) };
  }
}

export function directListenApp(
  onEvent: (event: WsEventEnvelope) => void,
): () => void {
  let isDisposed = false;
  let unlisten: (() => void) | undefined;
  listen<WsEventEnvelope>(RUNTIME_EVENT_CHANNEL, (event) => {
    if (isDisposed) return;
    const payload = event.payload as WsEventEnvelope;
    if (payload && payload.sessionId === null) onEvent(payload);
  }).then((fn) => {
    if (isDisposed && fn) fn();
    else if (fn) unlisten = fn;
  });
  return () => {
    isDisposed = true;
    unlisten?.();
  };
}

export const directPickFolder = () => invokeAuthority("pick_folder");
export const directOpenWorkspace = (path: string, shouldCreate: boolean) =>
  invokeAuthority("workspace_open", { path, shouldCreate });
export const directTrustedWorkspaces = () => invokeAuthority("workspaces_trusted");
export const updateWorkspaceTrust = (path: string, isTrusted: boolean) =>
  invokeAuthority("update_workspace_trust", { path, isTrusted });
export const directSessionRevert = (sessionId: string, index: number) =>
  invokeAuthority("session_revert", { sessionId, index });
export const updateSessionReasoning = (sessionId: string, effort: string) =>
  invokeAuthority("update_session_reasoning", { sessionId, effort });
export const directSessionRoots = (sessionId: string) =>
  invokeAuthority("session_roots", { sessionId });
export const addSessionRoot = (sessionId: string, path: string, canWrite: boolean) =>
  invokeAuthority("add_session_root", { sessionId, path, canWrite });
export const deleteSessionRoot = (sessionId: string, path: string) =>
  invokeAuthority("delete_session_root", { sessionId, path });
export const getSessionUnattended = (sessionId: string) =>
  invokeAuthority("get_session_unattended", { sessionId });
export const updateSessionUnattended = (sessionId: string, isUnattended: boolean) =>
  invokeAuthority("update_session_unattended", { sessionId, isUnattended });
export const directListInbox = (sessionId?: string, itemState?: string) =>
  invokeAuthority("inbox_list", { sessionId, itemState });
export const directResolveInbox = (id: string, resolution: string) =>
  invokeAuthority("inbox_resolve", { id, resolution });
export const directListArtifacts = (sessionId: string) =>
  invokeAuthority("artifacts_list", { sessionId });
export const directReadArtifact = (sessionId: string, path: string) =>
  invokeAuthority("artifact_read", { sessionId, path });
export const resolveArtifactPath = (sessionId: string, path: string) =>
  invokeAuthority("artifact_resolve_path", { sessionId, path });
export const directListMemory = () => invokeAuthority("memory_list");
export const directUpdateMemory = (id: number, content: string) =>
  invokeAuthority("memory_update", { id, content });
export const directDeleteMemory = (id: number) =>
  invokeAuthority("memory_delete", { id });
export const clearMemory = () => invokeAuthority("clear_memory");
export const fetchMemorySettings = () => invokeAuthority("memory_settings");
export const updateMemorySettings = (patch: Record<string, unknown>) =>
  invokeAuthority("update_memory_settings", { patch });
export const directListAutomations = () => invokeAuthority("automations_list");
export const directCreateAutomation = (payload: Record<string, unknown>) =>
  invokeAuthority("automation_create", { payload });
export const directGetAutomation = (id: string) =>
  invokeAuthority("automation_get", { id });
export const directUpdateAutomation = (id: string, changes: Record<string, unknown>) =>
  invokeAuthority("automation_update", { id, changes });
export const directDeleteAutomation = (id: string) =>
  invokeAuthority("automation_delete", { id });
export const updateAutomationSeen = (id: string) =>
  invokeAuthority("update_automation_seen", { id });
export const prepareAutomationRun = (id: string) =>
  invokeAuthority("prepare_automation_run", { id });
export const finalizeAutomationRun = (id: string, runId: string) =>
  invokeAuthority("finalize_automation_run", { id, runId });
export const directListMcp = () => invokeAuthority("mcp_list");
export const directPutMcp = (name: string, config: Record<string, unknown>) =>
  invokeAuthority("mcp_put", { name, config });
export const directPatchMcp = (name: string, changes: Record<string, unknown>) =>
  invokeAuthority("mcp_patch", { name, changes });
export const directDeleteMcp = (name: string) => invokeAuthority("mcp_delete", { name });
export const directMcpTools = (name: string) => invokeAuthority("mcp_tools", { name });
export const directReloadMcp = () => invokeAuthority("mcp_reload");
export const directConnectMcp = (name: string) => invokeAuthority("mcp_connect", { name });
export const directSignoutMcp = (name: string) => invokeAuthority("mcp_signout", { name });
export const directListAudit = (params: Record<string, unknown>) =>
  invokeAuthority("audit_list", params);
export const directListSkills = (workspace?: string) =>
  invokeAuthority("skills_list", { workspace });
export const directCreateSkill = (body: Record<string, unknown>) =>
  invokeAuthority("skill_create", { body });
export const directUpdateSkill = (name: string, patch: Record<string, unknown>) =>
  invokeAuthority("skill_update", { name, patch });
export const directDeleteSkill = (name: string, workspace?: string) =>
  invokeAuthority("skill_delete", { name, workspace });
export const directMoveSkill = (name: string, scope: string, workspace?: string) =>
  invokeAuthority("skill_move", { name, scope, workspace });
export const resolveSkillFolder = (name: string, workspace?: string) =>
  invokeAuthority("resolve_skill_folder", { name, workspace });
export const stageSkillUpload = (dataB64: string, filename: string) =>
  invokeAuthority("stage_skill_upload", { dataB64, filename });
export const confirmSkillUpload = (token: string, scope: string, workspace?: string) =>
  invokeAuthority("confirm_skill_upload", { token, scope, workspace });
export const directSessionSkills = (sessionId: string, workspace?: string) =>
  invokeAuthority("session_skills", { sessionId, workspace });
export const updateSessionSkill = (
  sessionId: string,
  skill: string,
  isEnabled: boolean,
  shouldClear: boolean,
  workspace?: string,
) => invokeAuthority("update_session_skill", { sessionId, skill, isEnabled, shouldClear, workspace });
export const directListConnectors = () => invokeAuthority("connectors_list");
export const directConnectConnector = (name: string, fields: Record<string, string>) =>
  invokeAuthority("connector_connect", { name, fields });
export const directDisconnectConnector = (name: string) =>
  invokeAuthority("connector_disconnect", { name });
export const applyConnectorTools = (name: string, toolStates: Record<string, boolean>) =>
  invokeAuthority("update_connector_tools", { name, toolStates });
export const directConnectorAction = (name: string, action: string, payload: unknown = {}) =>
  invokeAuthority("connector_action", { name, action, payload });
export const directSessionConnections = (sessionId: string) =>
  invokeAuthority("session_connections", { sessionId });
export const updateSessionConnection = (
  sessionId: string,
  connector: string,
  isEnabled: boolean,
  shouldClear: boolean,
) => invokeAuthority("update_session_connection", { sessionId, connector, isEnabled, shouldClear });
export const directListSubscriptions = () => invokeAuthority("subscriptions_list");
export const directAddSubscription = (sessionId: string, channel: string) =>
  invokeAuthority("subscription_add", { sessionId, channel });
export const directRemoveSubscription = (sessionId: string, channel: string) =>
  invokeAuthority("subscription_remove", { sessionId, channel });
export const listInboxRoutes = () => invokeAuthority("list_inbox_routes");
export const updateInboxRoutes = (name: string, channel: string | null, target: string) =>
  invokeAuthority("update_inbox_routes", { name, channel, target });
export const directListUnrouted = () => invokeAuthority("unrouted_list");
export const directRecentChannels = () => invokeAuthority("recent_channels");
export const getDmRoute = () => invokeAuthority("get_dm_route");
export const updateDmRoute = (sessionId: string) =>
  invokeAuthority("update_dm_route", { sessionId });
export const directBrowserState = () => invokeAuthority("browser_state");
export const directBrowserScreenshot = () => invokeAuthority("browser_screenshot");
export const directBrowserClose = () => invokeAuthority("browser_close");
