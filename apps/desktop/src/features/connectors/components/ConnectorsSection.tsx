import { useEffect, useState, type JSX } from "react";
import { disconnectConnector, getConnectors, type Connector } from "../../../api";
import { ConnectorBadge } from "../ConnectorIcon";
import { AllowlistBlock, ConnectorTools, ListeningSessionsBlock, UnauthorizedBlock } from "../../../components/ManageTabs";
import { AccountsDetail } from "./AccountsDetail";
import { AvailableDetail } from "./AvailableDetail";
import { ConnectorsList } from "./ConnectorsList";
import { StructuredDetail } from "./StructuredDetail";
import { WorkspaceChatDetail } from "./WorkspaceChatDetail";
import { GRP } from "./ui";
import { useI18n } from "@delta/i18n/I18nContext";

// Connectors surface = LIST ⇄ per-connector DETAIL SUBPAGE (UX-DECISIONS §21). The
// Integrations sub-nav never grows per-connector items; detail pages live behind a
// `‹ Connectors` breadcrumb. Concrete connector identity never selects a page here:
// extensions declare a Foundation-validated UI capability in the catalog manifest.

export interface DetailProps {
  c: Connector;
  onChanged: () => void;
}

type ConnectorUiDetail =
  | "generic"
  | "accounts"
  | "mail_accounts_privacy"
  | "calendar_accounts"
  | "crm_portals_privacy"
  | "workspace_chat"
  | "code_host_installations";

type ConnectorWithUi = Connector & { ui?: { detail?: ConnectorUiDetail } };

// These are stable Foundation UI capabilities, not connector/vendor names. The
// manifest can select only one of the Rust-validated keys below; it cannot inject
// components, HTML, or code into the desktop process. Capabilities without a
// Foundation renderer intentionally fall back to GenericDetail.
const DETAIL_PAGES: Partial<Record<ConnectorUiDetail, (p: DetailProps) => JSX.Element>> = {
  workspace_chat: (p) => <WorkspaceChatDetail {...p} />,
  mail_accounts_privacy: (p) => <StructuredDetail {...p} />,
  calendar_accounts: (p) => <StructuredDetail {...p} />,
  crm_portals_privacy: (p) => <StructuredDetail {...p} />,
  accounts: (p) => <AccountsDetail {...p} />,
};

function detailPageFor(c: Connector | undefined): ((p: DetailProps) => JSX.Element) | undefined {
  const detail = (c as ConnectorWithUi | undefined)?.ui?.detail ?? "generic";
  return DETAIL_PAGES[detail];
}

export function ConnectorsSection() {
  const { t } = useI18n();
  const [detail, setDetail] = useState<string | null>(null);
  const [connectors, setConnectors] = useState<Connector[]>([]);

  const refresh = () => {
    getConnectors().then(setConnectors).catch(() => setConnectors([]));
  };
  useEffect(() => {
    refresh();
    // Poll: recent senders/parked arrive over time; managed connects finish
    // in the system browser and surface on the next tick.
    const timer = setInterval(refresh, 5000);
    return () => clearInterval(timer);
  }, []);

  if (detail) {
    const c = connectors.find((x) => x.name === detail);
    const Page = detailPageFor(c);
    return (
      <div>
        <button
          className="text-[13px] text-accent mb-3"
          data-testid="connectors-breadcrumb"
          onClick={() => setDetail(null)}
        >
          ‹ {t("connectors.title")}
        </button>
        {!c ? (
          <div className="text-[13px] text-muted">{t("common.loading")}</div>
        ) : !c.connected ? (
          <AvailableDetail c={c} onChanged={refresh} />
        ) : Page ? (
          <Page c={c} onChanged={refresh} />
        ) : (
          <GenericDetail c={c} onChanged={refresh} onGone={() => setDetail(null)} />
        )}
      </div>
    );
  }

  return (
    <ConnectorsList connectors={connectors} onOpen={setDetail} onChanged={refresh} />
  );
}

// Fallback detail page: status header + generic config blocks. Product-specific
// identity stays in the manifest; Foundation renders only declared capabilities.
function GenericDetail({
  c,
  onChanged,
  onGone,
}: DetailProps & { onGone: () => void }) {
  const { t } = useI18n();
  return (
    <div data-testid="generic-connector-detail">
      <div className="flex items-center gap-3.5 mb-5">
        <ConnectorBadge connector={c} size={44} title={c.title} />
        <div className="min-w-0 flex-1">
          <h2 className="text-[20px] font-semibold tracking-tight leading-tight">{c.title}</h2>
          <div className="text-[12.5px] text-muted flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-full bg-ok" />
            {c.account || (c.auth === "none" ? t("connectors.statusBuiltIn") : t("connectors.connected"))}
          </div>
        </div>
        {c.auth !== "none" && (
          <button
            className="text-[12.5px] text-danger/80 hover:text-danger shrink-0"
            onClick={async () => {
              await disconnectConnector(c.name);
              onChanged();
              onGone();
            }}
          >
            {t("connectors.disconnect")}
          </button>
        )}
      </div>

      <div className={GRP}>
        <ConnectorTools c={c} onChanged={onChanged} />
      </div>

      {c.two_way && (
        <div className={GRP + " mt-4"}>
          <AllowlistBlock c={c} onChanged={onChanged} />
          <UnauthorizedBlock c={c} onChanged={onChanged} />
          {c.channels && <ListeningSessionsBlock c={c} />}
        </div>
      )}
    </div>
  );
}
