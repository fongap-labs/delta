import { useEffect, useState } from "react";
import {
  EGRESS_LEVELS,
  getEgressSettings,
  getMcpServers,
  pickWorkspaceFolder,
  setEgressDefault,
  setEndpointMark,
  setLocalOnly,
  setWorkspaceEgress,
  type EgressChange,
  type EgressLevel,
  type EgressSettings,
  type EndpointMark,
} from "../api";
import { useI18n } from "@delta/i18n/I18nContext";
import { LoadError } from "./ViewStates";

const CARD = "rounded-xl2 border border-line bg-panel p-4 mb-4";
const BTN = "text-[12.5px] px-3 py-1.5 rounded-lg border border-line bg-bg hover:border-line-strong shrink-0";

/** The four egress levels as a radio list, each with its one-line explanation. Nothing is preselected
 *  when `value` is null (the first-run choice, ADR-0057). */
export function EgressLevelList({
  value,
  onChange,
  name,
}: {
  value: EgressLevel | null;
  onChange: (level: EgressLevel) => void;
  name: string;
}) {
  const { t } = useI18n();
  return (
    <div role="radiogroup" className="flex flex-col gap-2">
      {EGRESS_LEVELS.map((level) => (
        <label
          key={level}
          data-testid={`egress-level-${level}`}
          className={
            "flex items-start gap-2.5 rounded-lg border px-3 py-2.5 cursor-pointer " +
            (value === level ? "border-primary bg-bg" : "border-line hover:border-line-strong")
          }
        >
          <input
            type="radio"
            name={name}
            className="mt-1"
            checked={value === level}
            onChange={() => onChange(level)}
          />
          <span>
            <span className="block text-[13px] font-medium text-ink">{t(`egress.level.${level}`)}</span>
            <span className="block text-[12px] text-muted mt-0.5">{t(`egress.level.${level}Help`)}</span>
          </span>
        </label>
      ))}
    </div>
  );
}

/** Runs a settings change; when the runtime says it loosens a level, asks before repeating it confirmed. */
export function useConfirmedChange(onSettings: (settings: EgressSettings) => void) {
  const [pending, setPending] = useState<((isConfirmed: boolean) => Promise<EgressChange>) | null>(null);
  const [error, setError] = useState("");
  const { t } = useI18n();

  const settle = (out: EgressChange | null) => {
    if (out && out.ok) {
      onSettings(out);
      return true;
    }
    if (out && out.needs_confirmation) return false;
    setError(t("egress.error.save"));
    return true;
  };

  const run = async (change: (isConfirmed: boolean) => Promise<EgressChange>) => {
    setError("");
    const out = await change(false).catch(() => null);
    if (!settle(out)) setPending(() => change);
  };

  const confirm = async () => {
    const change = pending;
    setPending(null);
    if (change) settle(await change(true).catch(() => null));
  };

  const dialog = pending ? (
    <div className="gate-overlay" role="dialog" aria-modal="true" aria-labelledby="egress-confirm-title">
      <div className="gate max-w-[480px]" data-testid="egress-confirm">
        <h2 id="egress-confirm-title">{t("egress.confirm.title")}</h2>
        <p className="gate-sub">{t("egress.confirm.body")}</p>
        <div className="gate-foot justify-end gap-2">
          <button className="btn" onClick={() => setPending(null)}>
            {t("egress.confirm.cancel")}
          </button>
          <button className="btn primary" onClick={() => void confirm()}>
            {t("egress.confirm.apply")}
          </button>
        </div>
      </div>
    </div>
  ) : null;

  return { run, dialog, error };
}

export function PrivacySection() {
  const { t } = useI18n();
  const [settings, setSettings] = useState<EgressSettings | null>(null);
  const [hasFailed, setFailed] = useState(false);
  const [host, setHost] = useState("");
  const [mark, setMark] = useState<EndpointMark>("approved");
  const [localServers, setLocalServers] = useState<string[]>([]);
  const { run, dialog, error } = useConfirmedChange(setSettings);

  const load = () => {
    setFailed(false);
    getEgressSettings()
      .then(setSettings)
      .catch(() => setFailed(true));
  };
  useEffect(load, []);
  useEffect(() => {
    // Only local (stdio) servers can carry the mark; a remote server is judged by its address.
    getMcpServers()
      .then((servers) =>
        setLocalServers(servers.filter((server) => server.transport === "stdio").map((server) => server.name)),
      )
      .catch(() => setLocalServers([]));
  }, []);

  if (hasFailed && !settings) return <LoadError what={t("egress.what")} onRetry={load} />;
  if (!settings) return null;

  const addWorkspace = async () => {
    const path = await pickWorkspaceFolder();
    if (!path) return;
    // A new entry starts at the strictest level; loosening it later asks for confirmation.
    await run((isConfirmed) => setWorkspaceEgress(path, "private_only", isConfirmed));
  };

  return (
    <div data-testid="privacy-section">
      {dialog}
      {error && <div className="gate-error mb-3">{error}</div>}

      <section className={CARD}>
        <div className="text-[13.5px] font-semibold">{t("egress.default.title")}</div>
        <div className="text-[12px] text-muted mt-1 mb-3">{t("egress.default.sub")}</div>
        <EgressLevelList
          name="egress-default"
          value={settings.default_level}
          onChange={(level) => void run((isConfirmed) => setEgressDefault(level, isConfirmed))}
        />
      </section>

      <section className={CARD}>
        <div className="flex items-center justify-between gap-3 mb-3">
          <div className="text-[13.5px] font-semibold">{t("egress.workspaces.title")}</div>
          <button className={BTN} onClick={() => void addWorkspace()}>
            {t("egress.workspaces.add")}
          </button>
        </div>
        {settings.workspaces.length === 0 ? (
          <div className="text-[12.5px] text-muted">{t("egress.workspaces.empty")}</div>
        ) : (
          settings.workspaces.map((entry) => (
            <div key={entry.workspace} className="flex items-center gap-2 py-1.5" data-testid="egress-workspace">
              <span className="flex-1 min-w-0 text-[12.5px] break-all">{entry.workspace}</span>
              <select
                className="text-[12.5px] rounded-lg border border-line bg-bg px-2 py-1"
                value={entry.level}
                onChange={(e) =>
                  void run((isConfirmed) =>
                    setWorkspaceEgress(entry.workspace, e.target.value as EgressLevel, isConfirmed),
                  )
                }
              >
                {EGRESS_LEVELS.map((level) => (
                  <option key={level} value={level}>
                    {t(`egress.level.${level}`)}
                  </option>
                ))}
              </select>
              <button
                className={BTN}
                onClick={() => void run((isConfirmed) => setWorkspaceEgress(entry.workspace, null, isConfirmed))}
              >
                {t("egress.workspaces.followDefault")}
              </button>
            </div>
          ))
        )}
      </section>

      <section className={CARD}>
        <div className="text-[13.5px] font-semibold">{t("egress.endpoints.title")}</div>
        <div className="text-[12px] text-muted mt-1 mb-3">{t("egress.endpoints.sub")}</div>
        {settings.endpoint_marks.length === 0 ? (
          <div className="text-[12.5px] text-muted mb-3">{t("egress.endpoints.empty")}</div>
        ) : (
          settings.endpoint_marks.map((entry) => (
            <div key={entry.host} className="flex items-center gap-2 py-1.5" data-testid="egress-endpoint">
              <span className="flex-1 min-w-0 text-[12.5px] break-all">{entry.host}</span>
              <span className="text-[12px] text-muted">{t(`egress.mark.${entry.mark}`)}</span>
              <button
                className={BTN}
                onClick={() => void run((isConfirmed) => setEndpointMark(entry.host, null, isConfirmed))}
              >
                {t("egress.endpoints.remove")}
              </button>
            </div>
          ))
        )}
        <div className="flex items-center gap-2 mt-2">
          <input
            className="flex-1 min-w-0 px-3 py-1.5 rounded-lg border border-line bg-bg text-[12.5px] outline-none focus:border-primary"
            placeholder={t("egress.endpoints.hostPlaceholder")}
            value={host}
            onChange={(e) => setHost(e.target.value)}
          />
          <select
            className="text-[12.5px] rounded-lg border border-line bg-bg px-2 py-1.5"
            value={mark}
            onChange={(e) => setMark(e.target.value as EndpointMark)}
          >
            <option value="approved">{t("egress.mark.approved")}</option>
            <option value="private">{t("egress.mark.private")}</option>
          </select>
          <button
            className={BTN}
            disabled={!host.trim()}
            onClick={() => {
              const value = host.trim();
              void run((isConfirmed) => setEndpointMark(value, mark, isConfirmed)).then(() => setHost(""));
            }}
          >
            {t("egress.endpoints.add")}
          </button>
        </div>
      </section>

      <section className={CARD}>
        <div className="text-[13.5px] font-semibold">{t("egress.localOnly.title")}</div>
        <div className="text-[12px] text-muted mt-1 mb-3">{t("egress.localOnly.sub")}</div>
        {localServers.length === 0 ? (
          <div className="text-[12.5px] text-muted">{t("egress.localOnly.empty")}</div>
        ) : (
          localServers.map((server) => (
            <label key={server} className="flex items-center gap-2 py-1.5" data-testid="egress-local-server">
              <span className="flex-1 min-w-0 text-[12.5px] break-all">{server}</span>
              <input
                type="checkbox"
                checked={settings.local_only_servers.includes(server)}
                onChange={(e) =>
                  void run((isConfirmed) => setLocalOnly(server, e.target.checked, isConfirmed))
                }
              />
              <span className="text-[12px] text-muted">{t("egress.localOnly.label")}</span>
            </label>
          ))
        )}
      </section>
    </div>
  );
}
