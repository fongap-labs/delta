// Trust facts for one artifact, read from what the runtime already records on the tool result
// that produced it: the finalized artifact payload (`path`, `sha256`, `run_id`) and the
// validation verdict (`ok`, `checks[]`). Nothing is inferred: if the producing tool result is
// not in the thread, or carries no validation, the artifact simply has no trust facts and the UI
// shows nothing (the runtime stays the authority, the UI only presents its records).

export interface TrustCheck {
  name: string;
  isOk: boolean;
  detail: string;
}

export interface ArtifactTrust {
  /** The run that registered the artifact; the key for the sources that run read. */
  runId?: string;
  /** Name of the tool call that produced the artifact (when the thread still has it). */
  producedBy?: string;
  sha256?: string;
  /** Validation verdict; absent when the producing tool result carried none. */
  checks?: TrustCheck[];
  isValidationOk?: boolean;
}

type RawMessage = {
  role?: string;
  tool_call_id?: string;
  name?: string;
  content?: unknown;
  tool_calls?: { id?: string; function?: { name?: string } }[];
};

const asRecord = (value: unknown): Record<string, unknown> | null =>
  value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : null;

const normalizePath = (path: string): string => path.replace(/\\/g, "/").replace(/^\.\//, "");

function parseContent(content: unknown): Record<string, unknown> | null {
  if (typeof content === "string") {
    try {
      return asRecord(JSON.parse(content));
    } catch {
      return null;
    }
  }
  return asRecord(content);
}

function parseChecks(validation: Record<string, unknown>): TrustCheck[] {
  if (!Array.isArray(validation.checks)) return [];
  return validation.checks.flatMap((entry) => {
    const check = asRecord(entry);
    if (!check || typeof check.name !== "string") return [];
    return [{ name: check.name, isOk: check.ok === true, detail: typeof check.detail === "string" ? check.detail : "" }];
  });
}

/** The latest tool result in the thread that registered `path`, or null when none did. */
export function artifactTrustFromMessages(messages: RawMessage[], path: string): ArtifactTrust | null {
  const wanted = normalizePath(path);
  const toolNames = new Map<string, string>();
  for (const message of messages) {
    for (const call of message.tool_calls ?? []) {
      if (call.id && call.function?.name) toolNames.set(call.id, call.function.name);
    }
  }

  let found: ArtifactTrust | null = null;
  for (const message of messages) {
    if (message.role !== "tool") continue;
    const output = parseContent(message.content);
    if (!output || !Array.isArray(output.artifacts)) continue;
    const entry = output.artifacts
      .map(asRecord)
      .find((artifact) => artifact && typeof artifact.path === "string" && normalizePath(artifact.path) === wanted);
    if (!entry) continue;

    const trust: ArtifactTrust = {};
    const producer = (message.tool_call_id && toolNames.get(message.tool_call_id)) || message.name;
    if (producer) trust.producedBy = producer;
    if (typeof entry.sha256 === "string" && entry.sha256) trust.sha256 = entry.sha256;
    if (typeof entry.run_id === "string" && entry.run_id) trust.runId = entry.run_id;
    const validation = asRecord(output.validation);
    if (validation) {
      const checks = parseChecks(validation);
      if (checks.length > 0) {
        trust.checks = checks;
        trust.isValidationOk = validation.ok !== false && checks.every((check) => check.isOk);
      }
    }
    found = trust; // later results replace earlier ones: the newest write is the one on disk
  }
  return found;
}

export function summarizeChecks(trust: ArtifactTrust): { passed: number; total: number } | null {
  if (!trust.checks) return null;
  return { passed: trust.checks.filter((check) => check.isOk).length, total: trust.checks.length };
}

/**
 * How a source location is shown: a URL loses its query and fragment (they can carry tokens or
 * personal data), a file keeps the workspace-relative path the runtime recorded.
 */
export function displaySourceLocation(location: string): string {
  if (!/^[a-z][a-z0-9+.-]*:\/\//i.test(location)) return location;
  try {
    const url = new URL(location);
    return url.origin + url.pathname;
  } catch {
    return location.split(/[?#]/)[0];
  }
}
