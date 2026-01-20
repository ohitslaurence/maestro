export type Tab = {
  id: string;
  label: string;
  disabled?: boolean;
};

type TabBarProps = {
  tabs: Tab[];
  activeTab: string;
  onTabChange: (tabId: string) => void;
};

export function TabBar({ tabs, activeTab, onTabChange }: TabBarProps) {
  return (
    <div className="tab-bar">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          className={`tab-bar__tab ${activeTab === tab.id ? "tab-bar__tab--active" : ""}`}
          onClick={() => onTabChange(tab.id)}
          disabled={tab.disabled}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}
