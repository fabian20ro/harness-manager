import { MENU_ITEMS, type AppTab } from "../lib/inspect";

type SidebarNavProps = {
  activeTab: AppTab;
  collapsed: boolean;
  globalReindexLabel?: string;
  isGlobalReindexRunning?: boolean;
  onSelectTab: (tab: AppTab) => void;
  onToggleCollapse: () => void;
  onReindex: () => void;
};

export function SidebarNav({
  activeTab,
  collapsed,
  globalReindexLabel = "Reindex all",
  isGlobalReindexRunning = false,
  onSelectTab,
  onToggleCollapse,
  onReindex,
}: SidebarNavProps) {
  return (
    <aside className={collapsed ? "nav collapsed" : "nav"}>
      <div className="brand">
        <div className="brand-copy">
          <span className="brand-mark">HI</span>
          {!collapsed ? (
            <div className="brand-text">
              <span className="brand-name">Harness Inspector</span>
              <span className="brand-tag">Inspect local harness state</span>
            </div>
          ) : null}
        </div>
      </div>

      <div className="nav-tabs">
        {MENU_ITEMS.map((item) => (
          <button
            key={item.id}
            className={item.id === activeTab ? "tab active" : "tab"}
            onClick={() => onSelectTab(item.id)}
            title={collapsed ? item.label : undefined}
            aria-label={item.label}
          >
            <span className="tab-emoji">{item.emoji}</span>
            {!collapsed ? <span>{item.label}</span> : null}
          </button>
        ))}
      </div>

      <div className="nav-footer">
        <button
          className="scan"
          onClick={onReindex}
          title={globalReindexLabel}
          disabled={isGlobalReindexRunning}
        >
          <span className="tab-emoji">{isGlobalReindexRunning ? "◌" : "🔁"}</span>
          {!collapsed ? <span>{globalReindexLabel}</span> : null}
        </button>
        <button
          className="collapse-toggle"
          onClick={onToggleCollapse}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          {collapsed ? "»" : "Collapse"}
        </button>
      </div>
    </aside>
  );
}
