import { useState } from "react";
import type { Connector } from "../../../api";
import { directConnectorAction } from "../../../runtimeTransport";
import { ConnectSetup } from "../../../components/ManageTabs";
import { ConnectorBadge } from "../ConnectorIcon";
import type { DetailProps } from "./ConnectorsSection";
import { ToolsDisclosure } from "./ToolsDisclosure";
import { FOOT, GRP, GRP_H, PILL_ACCENT, ROW, TAG_ACCENT, TAG_QUIET, TAG_WARN, XBTN } from "./ui";
import { useI18n } from "@delta/i18n/I18nContext";

type StructuredDetailKind =
  | "mail_accounts_privacy"
  | "calendar_accounts"
  | "crm_portals_privacy";

type RecordValue = Record<string, unknown>;

type ConnectorWithStructuredUi = Connector & {
  ui?: { detail?: StructuredDetailKind };
  accounts?: RecordValue[];
  portals?: RecordValue[];
  filters?: { senders?: string[]; labels?: string[] };
  hidden_fields?: string[];
};

function asText(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function asBool(value: unknown): boolean {
  return value === true;
}

function rowId(row: RecordValue): string {
  return asText(row.account_id) || asText(row.email) || asText(row.hub_id);
}

function rowName(row: RecordValue): string {
  return asText(row.name) || asText(row.email) || asText(row.account_id) || asText(row.hub_id);
}

export function StructuredDetail({ c, onChanged }: DetailProps) {
  const { t } = useI18n();
  const connector = c as ConnectorWithStructuredUi;
  const kind = connector.ui?.detail;
  const isPortal = kind === "crm_portals_privacy";
  const rows = isPortal ? connector.portals ?? [] : connector.accounts ?? [];
  const [shouldShowManual, setShowManual] = useState(false);

  return (
    <div data-testid="structured-connector-detail">
      <div className="flex items-center gap-3.5 mb-5">
        <ConnectorBadge connector={c} size={44} title={c.title} />
        <div className="min-w-0 flex-1">
          <h2 className="text-[20px] font-semibold tracking-tight leading-tight">{c.title}</h2>
          <div className="text-[12.5px] text-muted flex items-center gap-1.5">
            {c.connected ? (
              <>
                <span className="w-2 h-2 rounded-full bg-ok" />
                <span>{isPortal ? t("connectors.portalCount", { n: rows.length }) : t("connectors.accountCount", { n: rows.length })}</span>
              </>
            ) : (
              <span>{t("connectors.notConnected")}</span>
            )}
          </div>
        </div>
        <button
          className={PILL_ACCENT + (c.managed_paused ? " opacity-50" : "")}
          onClick={() => setShowManual((value) => !value)}
          disabled={c.managed_paused}
        >
          {c.managed_paused
            ? t("connectors.addAccountComingSoon")
            : isPortal
              ? t("connectors.addPortal")
              : t("connectors.addAccount")}
        </button>
      </div>

      {rows.length > 0 && (
        <>
          <div className={GRP_H + " !mt-0"}>{isPortal ? t("connectors.portals") : t("connectors.accounts")}</div>
          <div className={GRP} data-testid="structured-connector-rows">
            {rows.map((row) => (
              <StructuredRow key={rowId(row)} connector={c.name} row={row} portal={isPortal} onChanged={onChanged} />
            ))}
          </div>
        </>
      )}

      {(shouldShowManual || !c.connected) && (
        <>
          <div className={GRP_H + (rows.length ? "" : " !mt-0")}>
            {c.managed ? t("connectors.addManually") : t("connectors.addAnAccount")}
          </div>
          <div className={GRP}>
            <div className="px-1.5 py-1">
              <ConnectSetup
                c={c}
                onConnected={() => {
                  setShowManual(false);
                  onChanged();
                }}
              />
            </div>
          </div>
        </>
      )}

      {kind === "mail_accounts_privacy" && (
        <ListPrivacyGroup
          connector={c.name}
          title={t("connectors.neverShowAgents")}
          values={connector.filters ?? { senders: [], labels: [] }}
          onChanged={onChanged}
        />
      )}

      {kind === "crm_portals_privacy" && (
        <HiddenFieldsGroup
          connector={c.name}
          fields={connector.hidden_fields ?? []}
          onChanged={onChanged}
        />
      )}

      <ToolsDisclosure c={c} onChanged={onChanged} />
      <div className={FOOT + " mt-2"}>{t("connectors.accountsSeparate")}</div>
    </div>
  );
}

function StructuredRow({
  connector,
  row,
  portal,
  onChanged,
}: {
  connector: string;
  row: RecordValue;
  portal: boolean;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const [isBusy, setBusy] = useState(false);
  const id = rowId(row);
  const defaultRow = asBool(row.default);
  const actionIdKey = portal ? "hub_id" : "account_id";
  const disconnectAction = portal ? "disconnect_portal" : "disconnect_account";
  const defaultAction = portal ? "set_default_portal" : "set_default_account";

  const action = async (name: string) => {
    setBusy(true);
    try {
      await directConnectorAction(connector, name, { [actionIdKey]: id });
      onChanged();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className={ROW} data-testid="structured-connector-row">
      <span className="min-w-0 flex-1 flex items-center gap-2">
        <span className="text-[13px] font-medium truncate">{rowName(row)}</span>
        {defaultRow && <span className={TAG_ACCENT}>{t("common.default")}</span>}
        {asBool(row.needs_reauth) && <span className={TAG_WARN}>{t("connectors.signInAgain")}</span>}
        {asBool(row.sandbox) && <span className={TAG_WARN}>{t("connectors.sandbox")}</span>}
        {asText(row.access) && (
          <span className={TAG_QUIET}>
            {asText(row.access) === "write" ? t("connectors.readWrite") : t("connectors.readOnly")}
          </span>
        )}
      </span>
      {!defaultRow && (
        <button className="text-[12px] text-muted hover:text-ink shrink-0" disabled={isBusy} onClick={() => void action(defaultAction)}>
          {t("connectors.makeDefault")}
        </button>
      )}
      <button className={XBTN} disabled={isBusy} title={t("connectors.disconnectAccountTitle")} onClick={() => void action(disconnectAction)}>
        ×
      </button>
    </div>
  );
}

function ListPrivacyGroup({
  connector,
  title,
  values,
  onChanged,
}: {
  connector: string;
  title: string;
  values: { senders?: string[]; labels?: string[] };
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const senders = values.senders ?? [];
  const labels = values.labels ?? [];
  const save = async (patch: { senders?: string[]; labels?: string[] }) => {
    await directConnectorAction(connector, "set_filters", patch);
    onChanged();
  };
  return (
    <>
      <div className={GRP_H}>{title}</div>
      <div className={GRP}>
        <ChipListRow label={t("connectors.senders")} values={senders} onSave={(next) => save({ senders: next })} />
        <ChipListRow label={t("connectors.labels")} values={labels} onSave={(next) => save({ labels: next })} />
      </div>
    </>
  );
}

function HiddenFieldsGroup({ connector, fields, onChanged }: { connector: string; fields: string[]; onChanged: () => void }) {
  const { t } = useI18n();
  const save = async (next: string[]) => {
    await directConnectorAction(connector, "set_hidden_fields", { fields: next });
    onChanged();
  };
  return (
    <>
      <div className={GRP_H}>{t("connectors.accessPrivacy")}</div>
      <div className={GRP}>
        <ChipListRow label={t("connectors.hiddenFields")} values={fields} onSave={save} mono />
      </div>
    </>
  );
}

function ChipListRow({
  label,
  values,
  onSave,
  mono = false,
}: {
  label: string;
  values: string[];
  onSave: (next: string[]) => Promise<void>;
  mono?: boolean;
}) {
  const { t } = useI18n();
  const [draft, setDraft] = useState("");
  const add = async () => {
    const value = draft.trim();
    if (!value || values.includes(value)) return;
    setDraft("");
    await onSave([...values, value]);
  };
  return (
    <div className={ROW}>
      <span className="text-[12.5px] text-muted w-24 shrink-0">{label}</span>
      <span className="min-w-0 flex-1 flex flex-wrap items-center gap-1.5">
        {values.map((value) => (
          <span key={value} className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-paper border border-line text-[12.5px]${mono ? " font-mono" : ""}`}>
            {value}
            <button className={XBTN} title={t("common.remove")} onClick={() => void onSave(values.filter((item) => item !== value))}>
              ×
            </button>
          </span>
        ))}
        <input
          className="flex-1 min-w-[140px] bg-transparent text-[12.5px] outline-none placeholder:text-faint"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void add();
          }}
          onBlur={() => {
            if (draft.trim()) void add();
          }}
        />
      </span>
    </div>
  );
}
