import { MENU_ITEMS, type AppTab } from "../lib/inspect";

type SidebarNavProps = {
  activeTab: AppTab;
  collapsed: boolean;
  onSelectTab: (tab: AppTab) => void;
  onToggleCollapse: () => void;
  onReindex: () => void;
};

export function SidebarNav({
  activeTab,
  collapsed,
  onSelectTab,
  onToggleCollapse,
  onReindex,
}: SidebarNavProps) {
  return (
    <aside className={collapsed ? "nav collapsed" : "nav"}>
      <div className="brand">
        <div className="brand-row">
          <div className="brand-copy">
            <span className="brand-mark">H</span>
            {!collapsed ? <span className="brand-name">Harness Inspector</span> : null}
          </div>
          <button
            className="collapse-toggle"
            onClick={onToggleCollapse}
            aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          >
            {collapsed ? "»" : "«"}
          </button>
        </div>
      </div>

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
      <button className="scan" onClick={onReindex} title="Reindex">
        <span className="tab-emoji">🔁</span>
        {!collapsed ? <span>Reindex</span> : null}
      </button>
    </aside>
  );
}
