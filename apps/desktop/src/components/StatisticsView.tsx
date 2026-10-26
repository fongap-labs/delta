import { useEffect, useState } from "react";
import { listStatisticsTools, runStatisticsTool, type StatisticsTool } from "../api";
import { useI18n } from "@delta/i18n/I18nContext";

// Statistics without a model (ADR-0078): pick a tool, fill its arguments, run it on a CSV file in
// the workspace. The tools are read-only and local, so this works with no AI gateway configured.
// The arguments form is generated from each tool's JSON Schema; nothing here computes a figure.

type Fields = Record<string, string>;

type Scalar = string | number | boolean | null;

function formatValue(value: unknown): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "number") return String(Number.isInteger(value) ? value : Number(value.toPrecision(8)));
  if (typeof value === "string" || typeof value === "boolean") return String(value);
  return JSON.stringify(value, null, 2);
}

export function StatisticsView({ workspace }: { workspace?: string }) {
  const { t } = useI18n();
  const [tools, setTools] = useState<StatisticsTool[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [selected, setSelected] = useState("");
  const [fields, setFields] = useState<Fields>({});
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<Record<string, unknown> | null>(null);

  useEffect(() => {
    let alive = true;
    listStatisticsTools()
      .then((list) => {
        if (!alive) return;
        setTools(list);
        setSelected(list[0]?.name ?? "");
      })
      .catch((error) => {
        if (alive) setLoadError(String(error));
      });
    return () => {
      alive = false;
    };
  }, []);

  const tool = tools.find((item) => item.name === selected);
  const specs = Object.entries(tool?.parameters.properties ?? {});
  const required = new Set(tool?.parameters.required ?? []);
  const missing = [...required].filter((key) => !(fields[key] ?? "").trim());
  const canRun = Boolean(tool && workspace && missing.length === 0 && !running);

  const choose = (name: string) => {
    setSelected(name);
    setFields({});
    setResult(null);
  };

  const run = async () => {
    if (!tool || !workspace) return;
    setRunning(true);
    const args: Record<string, unknown> = {};
    for (const [key, spec] of specs) {
      const raw = (fields[key] ?? "").trim();
      if (raw === "") continue;
      const numeric = spec.type === "number" || spec.type === "integer";
      args[key] = numeric ? Number(raw) : raw;
    }
    setResult(await runStatisticsTool(tool.name, args, workspace));
    setRunning(false);
  };

  const scalars = result
    ? Object.entries(result).filter(([key, value]) => key !== "ok" && key !== "error" && typeof value !== "object")
    : [];
  const structured = result ? Object.entries(result).filter(([, value]) => typeof value === "object" && value !== null) : [];

  return (
    <div className="flex h-full min-h-0" data-testid="statistics-view">
      <aside className="w-[220px] shrink-0 border-r border-line overflow-y-auto hairline-scroll p-3">
        <div className="px-2 pb-2 text-[11px] uppercase tracking-[0.05em] text-faint font-semibold">
          {t("statistics.tools", undefined, "Tools")}
        </div>
        {loadError && <div className="px-2 text-[12.5px] text-danger">{loadError}</div>}
        {tools.map((item) => (
          <button
            key={item.name}
            className={
              "w-full text-left px-2 py-1.5 rounded-lg text-[13px] " +
              (item.name === selected ? "bg-selected text-ink" : "text-muted hover:bg-bg")
            }
            onClick={() => choose(item.name)}
          >
            {item.name}
          </button>
        ))}
      </aside>

      <section className="min-w-0 flex-1 overflow-y-auto hairline-scroll p-6">
        <h2 className="text-[18px] font-semibold text-ink">{t("statistics.title", undefined, "Statistics")}</h2>
        {!workspace && (
          <p className="mt-2 text-[13px] text-muted">
            {t("statistics.noWorkspace", undefined, "Open a conversation in a workspace first; the data file is read from there.")}
          </p>
        )}
        {tool && (
          <>
            <p className="mt-2 text-[13px] text-muted">{tool.description}</p>

            <div className="mt-4 grid gap-3 max-w-[560px]">
              {specs.map(([key, spec]) => (
                <label key={key} className="grid gap-1">
                  <span className="text-[12.5px] text-ink">
                    {key}
                    {required.has(key) ? " *" : ""}
                  </span>
                  {spec.enum ? (
                    <select
                      className="rounded-lg border border-line bg-panel px-2 py-1.5 text-[13px]"
                      value={fields[key] ?? ""}
                      onChange={(e) => setFields({ ...fields, [key]: e.target.value })}
                    >
                      <option value="">{spec.default !== undefined ? `default: ${String(spec.default)}` : ""}</option>
                      {spec.enum.map((option) => (
                        <option key={option} value={option}>
                          {option}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <input
                      className="rounded-lg border border-line bg-panel px-2 py-1.5 text-[13px]"
                      type={spec.type === "number" || spec.type === "integer" ? "number" : "text"}
                      value={fields[key] ?? ""}
                      placeholder={spec.description}
                      onChange={(e) => setFields({ ...fields, [key]: e.target.value })}
                    />
                  )}
                  {spec.description && <span className="text-[11.5px] text-faint">{spec.description}</span>}
                </label>
              ))}
            </div>

            <button
              className="mt-4 rounded-lg bg-ink px-3 py-1.5 text-[13px] text-panel disabled:opacity-40"
              disabled={!canRun}
              onClick={run}
            >
              {running ? t("statistics.running", undefined, "Running…") : t("statistics.run", undefined, "Run")}
            </button>
            {missing.length > 0 && (
              <span className="ml-3 text-[12px] text-faint">
                {t("statistics.missing", { fields: missing.join(", ") }, `Fill in: ${missing.join(", ")}`)}
              </span>
            )}

            {result && (
              <div className="mt-5 max-w-[720px]" data-testid="statistics-result">
                {result.ok === false || result.error ? (
                  <div className="text-[13px] text-danger">
                    {t("statistics.error", undefined, "The calculation did not run:")} {String(result.error ?? "")}
                  </div>
                ) : (
                  <>
                    <div className="text-[12px] uppercase tracking-[0.05em] text-faint font-semibold">
                      {t("statistics.result", undefined, "Result")}
                    </div>
                    <table className="mt-2 w-full text-[13px]">
                      <tbody>
                        {(scalars as [string, Scalar][]).map(([key, value]) => (
                          <tr key={key} className="border-b border-line">
                            <td className="py-1 pr-4 text-muted">{key}</td>
                            <td className="py-1 font-mono text-ink">{formatValue(value)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                    {structured.map(([key, value]) => (
                      <div key={key} className="mt-3">
                        <div className="text-[12px] text-muted">{key}</div>
                        <pre className="mt-1 overflow-x-auto rounded-lg bg-bg p-2 text-[12px]">{formatValue(value)}</pre>
                      </div>
                    ))}
                  </>
                )}
              </div>
            )}
          </>
        )}
      </section>
    </div>
  );
}
