// The endpoint Delta will actually request, computed from what the user typed.
//
// This mirrors `crates/delta-core/src/provider.rs::versioned_endpoint` and must stay in step with
// it. The help text used to say "include /v1 when the endpoint requires it", which told the user
// nothing about what the runtime would do, so a correct base URL looked wrong and a complete
// endpoint path was spliced twice. Showing the resolved URL replaces that guess with a fact.

/** The endpoint path each protocol is served at, relative to the version segment. */
export const PROTOCOL_ENDPOINT = {
  openai_chat: "chat/completions",
  anthropic: "messages",
  openai_responses: "responses",
} as const;

export type ProtocolName = keyof typeof PROTOCOL_ENDPOINT;

/** True when the last path segment is an API version: `v1`, `v2`, `v1beta`, `api/v3`. */
function hasVersionSegment(path: string): boolean {
  const segment = (path.split("/").pop() ?? "").toLowerCase();
  return (
    segment.length >= 2 &&
    segment.startsWith("v") &&
    segment.slice(1).split("").every((c) => c >= "0" && c <= "9")
  );
}

/**
 * Join a configured base URL with a protocol's endpoint, without duplicating a version segment
 * or the endpoint itself.
 *
 * | base URL | result |
 * |---|---|
 * | `https://host` | `https://host/v1/<endpoint>` |
 * | `https://host/v1` | `https://host/v1/<endpoint>` |
 * | `https://host/v1/messages` | unchanged — already a full endpoint |
 * | `https://host/api/v2` | `https://host/api/v2/<endpoint>` |
 */
export function resolveEndpoint(baseUrl: string, endpoint: string): string {
  const base = baseUrl.trim().replace(/\/+$/, "");
  if (!base) return "";
  // Stripping the endpoint can leave a trailing slash (`…/v1/messages` → `…/v1/`), which must go
  // before the version check or the last segment reads as empty.
  const stripped = base.endsWith(endpoint)
    ? base.slice(0, base.length - endpoint.length).replace(/\/+$/, "")
    : base;
  if (stripped !== base && hasVersionSegment(stripped)) return base;
  return hasVersionSegment(stripped) ? `${stripped}/${endpoint}` : `${stripped}/v1/${endpoint}`;
}

/** The URL the add-provider form shows under the base-URL field. */
export function endpointPreview(baseUrl: string, protocol: ProtocolName): string {
  const endpoint = PROTOCOL_ENDPOINT[protocol] ?? PROTOCOL_ENDPOINT.openai_chat;
  return resolveEndpoint(baseUrl, endpoint);
}
