import { useState } from 'react';
import clsx from 'clsx';
import { Settings as SettingsIcon, Cpu, Smartphone, Box, Activity } from 'lucide-react';

import LlmTab from './LlmTab';
import SystemTab from './SystemTab';
import MobileTab from './MobileTab';
import SkillManager from './SkillManager';
import AIDashboard from './AIDashboard';

type TabId = 'llm' | 'monitor' | 'system' | 'mobile' | 'skills';

export default function Settings() {
  const [activeTab, setActiveTab] = useState<TabId>('llm');

  const tabs = [
    { id: 'llm', label: 'AI Models', icon: Cpu },
    { id: 'monitor', label: 'AI Dashboard', icon: Activity },
    { id: 'skills', label: 'Marketplace', icon: Box },
    { id: 'system', label: 'System', icon: SettingsIcon },
    { id: 'mobile', label: 'Mobile Pairing', icon: Smartphone },
  ] as const;

  return (
    <div className="flex flex-col h-full bg-[var(--bg-app)] overflow-hidden">
      {/* Header & Tab Bar */}
      <div className="px-8 pt-8 pb-6 border-b border-[var(--border-default)] bg-[var(--bg-app)] flex flex-col shrink-0 z-10 shadow-sm relative">
        <div>
          <h1 className="text-2xl font-bold text-[var(--text-primary)]">Settings</h1>
          <p className="text-[var(--text-secondary)] mt-1">Configure Office Hub, AI models, and system integrations.</p>
        </div>
        
        <div className="flex space-x-2 mt-6 overflow-x-auto pb-1 hide-scrollbar">
          <div className="flex bg-[var(--bg-sidebar)] p-1.5 rounded-[20px] border border-[var(--border-default)] shadow-inner">
            {tabs.map(tab => {
              const Icon = tab.icon;
              const isActive = activeTab === tab.id;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id as TabId)}
                  className={clsx(
                    "flex items-center px-4 py-2 text-sm font-medium transition-all duration-200 rounded-2xl whitespace-nowrap",
                    isActive 
                      ? "bg-[var(--bg-card)] text-indigo-600 dark:text-indigo-400 shadow-sm border border-[var(--border-default)]" 
                      : "text-[var(--text-secondary)] hover:text-[var(--text-primary)] border border-transparent hover:bg-[var(--bg-hover)]"
                  )}
                >
                  <Icon size={16} className={clsx("mr-2", isActive ? "text-indigo-600 dark:text-indigo-400" : "text-[var(--text-muted)]")} />
                  {tab.label}
                </button>
              );
            })}
          </div>
        </div>
      </div>

      {/* Content Area */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-5xl mx-auto w-full p-8 h-full">
          {activeTab === 'llm' && <LlmTab />}
          {activeTab === 'monitor' && <AIDashboard />}
          {activeTab === 'skills' && <SkillManager />}
          {activeTab === 'system' && <SystemTab />}
          {activeTab === 'mobile' && <MobileTab />}
        </div>
      </div>
    </div>
  );
}
