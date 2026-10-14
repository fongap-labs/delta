import ReactMarkdown, { defaultUrlTransform } from "react-markdown";
import remarkGfm from "remark-gfm";
import { useState, type ReactNode } from "react";
import { Icon } from "./Icon";

// §34 (UX-016): the agent ends a deliverable turn with plain markdown —
// [Title](artifact:relative/path) — and the renderer turns it into a chip that opens the
// artifact viewer in place. Plumbing is a window event (the viewer lives in RightRail;
// this component renders deep inside the transcript): RightRail resolves the path against
// the session's artifact list, App un-hides the rail.
export const OPEN_ARTIFACT_EVENT = "delta-open-artifact";

// Markdown link hrefs are URI-encoded (non-ASCII filenames become %E4%BA%8C…), while the
// artifact list and the server resolve the decoded on-disk path. Decode once here so the
// chip displays the real filename and the viewer can match it; a literal "%" that isn't an
// escape sequence (e.g. a file named "100%.md") falls back to the raw path unchanged.
function decodeArtifactPath(path: string): string {
  try {
    return decodeURIComponent(path);
  } catch {
    return path;
  }
}

function ArtifactChip({ path, title }: { path: string; title: string }) {
  const file = path.split("/").pop() || path;
  return (
    <button
      className="art-chip"
      data-testid="artifact-chip"
      title={path}      onClick={() =>
        window.dispatchEvent(new CustomEvent(OPEN_ARTIFACT_EVENT, { detail: { path } }))
      }
    >
      <span className="art-chip-ico">
        <Icon name="file" size={14} />
      </span>
      <span className="art-chip-meta">
        <b>{title || file}</b>
        {title && title !== file && <span>{file}</span>}
      </span>
      <span className="art-chip-open">Open ›</span>
    </button>
  );
}

/** Fenced code with a copy affordance, the way ChatGPT/Claude/WorkBuddy present it:
 *  the button lives in the block's top-right corner and only appears on hover/focus, so
 *  reading a long snippet is never disturbed by chrome that is irrelevant most of the time. */
function CodeBlock({ children }: { children?: ReactNode }) {
  const [isCopied, setCopied] = useState(false);
  // react-markdown hands us <pre><code class="language-x">…</code></pre>; the code
  // element owns the raw text, so read it off the child rather than re-deriving it.
  // Nested arrays (a block built from several text nodes) are flattened.
  const code = (children as { props?: { children?: unknown } } | undefined)?.props?.children;
  const flatten = (value: unknown): string => {
    if (typeof value === "string") return value;
    if (Array.isArray(value)) return value.map(flatten).join("");
    return "";
  };
  const text = flatten(code);
  const copy = () => {
    navigator.clipboard
      ?.writeText(text)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1400);
      })
      .catch(() => {});
  };
  return (
    <div className="md-code" data-testid="md-code">
      {children}
      {text && (
        <button
          type="button"
          className="md-code-copy"
          data-testid="md-code-copy"
          aria-label={isCopied ? "Copied" : "Copy code"}
          title={isCopied ? "Copied" : "Copy code"}
          onClick={copy}
        >
          <Icon name={isCopied ? "check" : "copy"} size={12} />
          <span>{isCopied ? "Copied" : "Copy"}</span>
        </button>
      )}
    </div>
  );
}

// Assistant messages rendered as GitHub-flavored markdown (headings, lists, tables, code,
// links). Links open externally — never navigate the app shell — except artifact: links,
// which open the session's artifact viewer.
export function Markdown({ text }: { text: string }) {
  return (
    <div className="md">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        // artifact: is ours — keep it through the sanitizer (everything else gets the default
        // http/https/mailto policy).
        urlTransform={(url) => (url.startsWith("artifact:") ? url : defaultUrlTransform(url))}
        components={{
          a: ({ node: _n, href, children, ...props }) => {
            if (href?.startsWith("artifact:")) {
              const title = Array.isArray(children) ? children.join("") : String(children ?? "");
              return (
                <ArtifactChip
                  path={decodeArtifactPath(href.slice("artifact:".length))}
                  title={title}
                />
              );
            }
            return (
              <a href={href} {...props} target="_blank" rel="noreferrer">
                {children}
              </a>
            );
          },
          pre: ({ children }) => <CodeBlock>{children}</CodeBlock>,
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
