import type { ReactNode } from "react";
import { Icon, type IconName } from "../Icon";

export interface PageNavTab<Key extends string> {
  key: Key;
  label: string;
  icon: IconName;
}

export function PageLayout<Key extends string>({
  title,
  icon,
  tabs,
  active,
  onChange,
  wide = false,
  children,
}: {
  title: string;
  icon: IconName;
  tabs: readonly PageNavTab<Key>[];
  active: Key;
  onChange: (key: Key) => void;
  wide?: boolean;
  children: ReactNode;
}) {
  return (
    <main className="ds-page flex min-w-0 flex-1">
      <nav className="page-subnav ds-page-subnav" aria-label={title}>
        <div className="ds-page-subnav-heading">
          <Icon name={icon} size={16} />
          <span>{title}</span>
        </div>
        <div className="ds-page-subnav-items">
          {tabs.map((tab) => (
            <button
              type="button"
              key={tab.key}
              className="ds-nav-button"
              aria-current={active === tab.key ? "page" : undefined}
              onClick={() => onChange(tab.key)}
            >
              <Icon name={tab.icon} size={15} className="shrink-0" />
              <span className="min-w-0 truncate">{tab.label}</span>
            </button>
          ))}
        </div>
      </nav>
      <div className="ds-page-scroll hairline-scroll">
        <div className="ds-page-content" data-wide={wide ? "true" : undefined}>
          {children}
        </div>
      </div>
    </main>
  );
}
