import { useState } from "react";
import { McpTab } from "./ManageTabs";
import { ConnectorsSection } from "../features/connectors/components/ConnectorsSection";
import { PageLayout } from "./ui/PageLayout";
import { useI18n, type TranslationKey } from "@delta/i18n/I18nContext";

// The Connectors surface (renamed from "Integrations", §26) keeps the left sub-nav, now just
// Connectors · MCP. The old "Messaging routing" tab (and its ⚠ unrouted badge) moved whole to
// Inbox ▸ Configure (§28): inbox-delivery config belongs with the Inbox, and Unrouted is
// "messages that never reached you". The one remaining Activity is the audit log, reached from
// the account menu.
type IntTab = "connectors" | "mcp";

// Fixed sub-nav (UX-DECISIONS §21): connector detail lives as a SUBPAGE under
// Connectors, never as a nav item — the nav must not grow per connector.
const INT_TABS: { key: IntTab; labelKey: TranslationKey; icon: "puzzle" | "server" }[] = [
  { key: "connectors", labelKey: "connectors.appsTab", icon: "puzzle" },
  { key: "mcp", labelKey: "connectors.mcp", icon: "server" },
];

export function IntegrationsView() {
  const [tab, setTab] = useState<IntTab>("connectors");
  const { t } = useI18n();

  return (
    <PageLayout
      title={t("connectors.title")}
      icon="plug"
      tabs={INT_TABS.map((item) => ({ key: item.key, label: t(item.labelKey), icon: item.icon }))}
      active={tab}
      onChange={setTab}
      wide
    >
          {tab === "connectors" ? (
            <section>
              <PanelHead
                title={t("connectors.appsTab")}
                sub={t("connectors.subList")}
              />
              <ConnectorsSection />
            </section>
          ) : (
            <section>
              <PanelHead
                title={t("connectors.mcp")}
                sub={t("connectors.mcpSub")}
              />
              <McpTab />
            </section>
          )}
    </PageLayout>
  );
}

export function PanelHead({ title, sub }: { title: string; sub: string }) {
  return (
    <div className="ds-page-head">
      <h2>{title}</h2>
      <p>{sub}</p>
    </div>
  );
}
