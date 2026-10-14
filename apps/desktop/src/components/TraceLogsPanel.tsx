import { useI18n } from "@delta/i18n/I18nContext";
import { finalizedArtifactTraces, type ArtifactTrace } from "../artifactTrace";

/** Shows where each delivered artifact came from. Renders nothing until one is finalized. */
export function TraceLogsPanel({ traces }: { traces: ArtifactTrace[] }) {
  const { t } = useI18n();
  const finalized = finalizedArtifactTraces(traces);
  if (finalized.length === 0) return null;

  return (
    <section className="trace-logs" aria-label={t("artifacts.trace.title")}>
      <h3 className="trace-logs-title">{t("artifacts.trace.title")}</h3>
      {finalized.map((trace) => (
        <div key={trace.path} className="trace-logs-item">
          <div className="trace-logs-artifact">
            <span>{trace.name}</span>
            <span className="trace-logs-status">{t("artifacts.trace.finalized")}</span>
          </div>
          {trace.lineage.length === 0 ? (
            <p className="trace-logs-empty">{t("artifacts.trace.empty")}</p>
          ) : (
            <ol className="trace-logs-sources">
              {trace.lineage.map((source, index) => (
                <li key={`${source.file_path}-${index}`}>
                  <code>{source.file_path}</code>
                  {source.matched_snippet && <blockquote>{source.matched_snippet}</blockquote>}
                </li>
              ))}
            </ol>
          )}
        </div>
      ))}
    </section>
  );
}
