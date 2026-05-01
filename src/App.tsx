import { useState, useEffect, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { Suspense, lazy } from 'react';
const ChatPane = lazy(() => import('./components/ChatPane'));
const SettingsPane = lazy(() => import('./components/Settings'));
const WorkflowBuilder = lazy(() => import('./components/WorkflowBuilder'));
const AgentManager = lazy(() => import('./components/AgentManager'));
const FileBrowser = lazy(() => import('./components/FileBrowser'));
const KnowledgeBase = lazy(() => import('./components/KnowledgeBase'));
const AgentMonitor = lazy(() => import('./components/AgentMonitor/AgentMonitor'));
import {
  Menu, Settings, MessageSquare, Folders, Workflow,
  Store, BookOpen, Activity, Sun, Moon, Monitor
} from 'lucide-react';
import clsx from 'clsx';
import { Toaster } from 'react-hot-toast';
import { ErrorBoundary } from './ErrorBoundary';
import WorkspaceSwitcher from './components/WorkspaceSwitcher';
import { useWorkspaceStore } from './store/workspaceStore';
import { ChartRenderer } from './components/ChartRenderer';
import { OfficeHubLogo } from './components/OfficeHubLogo';
import './App.css';

type ThemeMode = 'light' | 'dark' | 'system';

const SIDEBAR_MIN = 64;
const SIDEBAR_COLLAPSED = 64;
const SIDEBAR_DEFAULT = 220;
const SIDEBAR_MAX = 320;
const SIDEBAR_COMPACT_THRESHOLD = 100; // below this, hide labels

function applyTheme(mode: ThemeMode) {
  const root = document.documentElement;
  if (mode === 'dark') {
    root.classList.add('dark');
    root.classList.remove('light');
  } else if (mode === 'light') {
    root.classList.remove('dark');
    root.classList.add('light');
  } else {
    // system
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    if (prefersDark) {
      root.classList.add('dark');
      root.classList.remove('light');
    } else {
      root.classList.remove('dark');
      root.classList.add('light');
    }
  }
}

function App() {
  const [activeTab, setActiveTab] = useState('chat');
  const [sidebarWidth, setSidebarWidth] = useState<number>(() => {
    const saved = localStorage.getItem('oh-sidebar-width');
    return saved ? parseInt(saved, 10) : SIDEBAR_DEFAULT;
  });
  const [isResizing, setIsResizing] = useState(false);
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => {
    return (localStorage.getItem('oh-theme') as ThemeMode) || 'dark';
  });

  const sidebarRef = useRef<HTMLElement>(null);
  const startXRef = useRef(0);
  const startWidthRef = useRef(0);

  const activeWorkspaceId = useWorkspaceStore(state => state.activeWorkspaceId);
  const isCompact = sidebarWidth < SIDEBAR_COMPACT_THRESHOLD;

  // Apply theme on mount and change
  useEffect(() => {
    applyTheme(themeMode);
    localStorage.setItem('oh-theme', themeMode);
  }, [themeMode]);

  // Listen for system theme changes if in 'system' mode
  useEffect(() => {
    if (themeMode !== 'system') return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => applyTheme('system');
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, [themeMode]);

  // Sidebar resize logic
  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    startXRef.current = e.clientX;
    startWidthRef.current = sidebarWidth;
    setIsResizing(true);
  }, [sidebarWidth]);

  useEffect(() => {
    if (!isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      const delta = e.clientX - startXRef.current;
      const newWidth = Math.max(SIDEBAR_MIN, Math.min(SIDEBAR_MAX, startWidthRef.current + delta));
      setSidebarWidth(newWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
      // Snap to collapsed if dragged below threshold
      setSidebarWidth(prev => {
        const snapped = prev < 90 ? SIDEBAR_COLLAPSED : prev;
        localStorage.setItem('oh-sidebar-width', String(snapped));
        return snapped;
      });
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizing]);

  const tabs = [
    { id: 'chat', icon: MessageSquare, label: 'Chat' },
    { id: 'files', icon: Folders, label: 'Files' },
    { id: 'workflows', icon: Workflow, label: 'Workflows' },
    { id: 'marketplace', icon: Store, label: 'AI Dashboard' },
    { id: 'knowledge', icon: BookOpen, label: 'Knowledge' },
    { id: 'monitor', icon: Activity, label: 'Monitor' },
    { id: 'settings', icon: Settings, label: 'Settings' },
  ];

  useEffect(() => {
    const unlisten = listen<{ tab: string }>('navigate', (event) => {
      if (event.payload?.tab) setActiveTab(event.payload.tab);
    });
    return () => { unlisten.then(f => f()); };
  }, []);

  const visibleTabs = activeWorkspaceId === 'default' || activeWorkspaceId === 'Global'
    ? tabs
    : tabs.filter(t => t.id === 'chat' || t.id === 'files');

  useEffect(() => {
    if (activeWorkspaceId !== 'default' && activeWorkspaceId !== 'Global') {
      if (activeTab !== 'chat' && activeTab !== 'files') setActiveTab('chat');
    }
  }, [activeWorkspaceId, activeTab]);

  const cycleTheme = () => {
    setThemeMode(prev => prev === 'dark' ? 'light' : prev === 'light' ? 'system' : 'dark');
  };

  const ThemeIcon = themeMode === 'dark' ? Moon : themeMode === 'light' ? Sun : Monitor;
  const themeLabel = themeMode === 'dark' ? 'Dark' : themeMode === 'light' ? 'Light' : 'System';

  const navTabs = visibleTabs.filter(t => t.id !== 'settings');
  const settingsTab = visibleTabs.find(t => t.id === 'settings');

  return (
    <ErrorBoundary>
      <div
        className={clsx(
          "flex h-screen w-full overflow-hidden transition-theme",
          isResizing && "select-none cursor-col-resize"
        )}
        style={{ background: 'var(--bg-app)', color: 'var(--text-primary)' }}
      >
        {/* ── Sidebar ─────────────────────────────────────────── */}
        <aside
          ref={sidebarRef}
          className="flex flex-col relative z-20 transition-theme"
          style={{
            width: sidebarWidth,
            minWidth: sidebarWidth,
            background: 'var(--bg-sidebar)',
            borderRight: '1px solid var(--border-default)',
            boxShadow: '2px 0 8px rgba(0,0,0,0.06)',
            transition: isResizing ? 'none' : 'width 0.15s ease',
          }}
        >
          {/* Logo + Toggle */}
          <div style={{ borderBottom: '1px solid var(--border-default)' }}>
            <div className={clsx("flex items-center px-3 py-3.5", isCompact ? "justify-center" : "justify-between")}>
              {!isCompact && (
                <div className="flex items-center gap-2.5 overflow-hidden">
                  <OfficeHubLogo size={26} className="flex-shrink-0" />
                  <span
                    className="font-bold text-base tracking-tight truncate"
                    style={{ color: 'var(--text-primary)' }}
                  >
                    Office Hub
                  </span>
                </div>
              )}
              <button
                onClick={() => setSidebarWidth(prev => {
                  const next = prev <= SIDEBAR_COLLAPSED ? SIDEBAR_DEFAULT : SIDEBAR_COLLAPSED;
                  localStorage.setItem('oh-sidebar-width', String(next));
                  return next;
                })}
                className="p-1.5 rounded-lg flex-shrink-0 transition-colors"
                style={{ color: 'var(--text-secondary)' }}
                title="Toggle Sidebar"
              >
                <Menu size={18} />
              </button>
            </div>

            {!isCompact && (
              <div className="px-3 pb-3">
                <WorkspaceSwitcher sidebarOpen={true} />
              </div>
            )}
          </div>

          {/* Nav Items */}
          <nav className="flex-1 py-3 flex flex-col gap-1 px-2 overflow-y-auto">
            {navTabs.map((tab) => {
              const Icon = tab.icon;
              const isActive = activeTab === tab.id;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id)}
                  title={tab.label}
                  className={clsx(
                    "flex items-center gap-2.5 rounded-xl transition-all duration-150 w-full text-left cursor-pointer",
                    isCompact ? "p-2.5 justify-center" : "px-3 py-2.5"
                  )}
                  style={{
                    background: isActive ? 'var(--accent-subtle)' : 'transparent',
                    color: isActive ? 'var(--accent-text)' : 'var(--text-secondary)',
                    fontWeight: isActive ? 600 : 500,
                    fontSize: '0.875rem',
                    border: '1px solid',
                    borderColor: isActive ? 'transparent' : 'transparent',
                  }}
                  onMouseEnter={e => {
                    if (!isActive) (e.currentTarget as HTMLElement).style.background = 'var(--bg-hover)';
                  }}
                  onMouseLeave={e => {
                    if (!isActive) (e.currentTarget as HTMLElement).style.background = 'transparent';
                  }}
                >
                  <Icon
                    size={18}
                    className="flex-shrink-0"
                    style={{ color: isActive ? 'var(--accent)' : 'var(--text-muted)' }}
                  />
                  {!isCompact && <span className="truncate">{tab.label}</span>}
                </button>
              );
            })}
          </nav>

          {/* Footer: Settings + Theme Toggle */}
          <div className="px-2 pb-3 flex flex-col gap-1" style={{ borderTop: '1px solid var(--border-default)' }}>
            <div className="pt-2" />

            {settingsTab && (() => {
              const Icon = settingsTab.icon;
              const isActive = activeTab === 'settings';
              return (
                <button
                  onClick={() => setActiveTab('settings')}
                  title="Settings"
                  className={clsx(
                    "flex items-center gap-2.5 rounded-xl transition-all duration-150 w-full text-left",
                    isCompact ? "p-2.5 justify-center" : "px-3 py-2.5"
                  )}
                  style={{
                    background: isActive ? 'var(--accent-subtle)' : 'transparent',
                    color: isActive ? 'var(--accent-text)' : 'var(--text-secondary)',
                    fontWeight: 500,
                    fontSize: '0.875rem',
                  }}
                  onMouseEnter={e => {
                    if (!isActive) (e.currentTarget as HTMLElement).style.background = 'var(--bg-hover)';
                  }}
                  onMouseLeave={e => {
                    if (!isActive) (e.currentTarget as HTMLElement).style.background = 'transparent';
                  }}
                >
                  <Icon size={18} className="flex-shrink-0" style={{ color: isActive ? 'var(--accent)' : 'var(--text-muted)' }} />
                  {!isCompact && <span>Settings</span>}
                </button>
              );
            })()}

            {/* Theme toggle */}
            <button
              onClick={cycleTheme}
              title={`Theme: ${themeLabel} (click to cycle)`}
              className={clsx(
                "flex items-center gap-2.5 rounded-xl transition-all duration-150 w-full text-left",
                isCompact ? "p-2.5 justify-center" : "px-3 py-2.5"
              )}
              style={{
                color: 'var(--text-secondary)',
                fontWeight: 500,
                fontSize: '0.875rem',
                background: 'transparent',
              }}
              onMouseEnter={e => { (e.currentTarget as HTMLElement).style.background = 'var(--bg-hover)'; }}
              onMouseLeave={e => { (e.currentTarget as HTMLElement).style.background = 'transparent'; }}
            >
              <ThemeIcon size={18} className="flex-shrink-0" style={{ color: 'var(--text-muted)' }} />
              {!isCompact && <span className="text-xs">{themeLabel} mode</span>}
            </button>

            {!isCompact && (
              <div
                className="text-center py-1 text-[10px] font-medium uppercase tracking-widest"
                style={{ color: 'var(--text-muted)' }}
              >
                v1.0.0
              </div>
            )}
          </div>

          {/* Resize handle */}
          <div
            className={clsx("sidebar-resize-handle", isResizing && "dragging")}
            onMouseDown={handleResizeStart}
          />
        </aside>

        {/* ── Main Content ─────────────────────────────────────── */}
        <main
          className="flex-1 flex flex-col overflow-hidden relative transition-theme"
          style={{ background: 'var(--bg-main)' }}
        >
          <Toaster position="bottom-right" />
          <ChartRenderer />

          <Suspense fallback={
            <div
              className="flex items-center justify-center h-full text-sm"
              style={{ color: 'var(--text-secondary)' }}
            >
              Loading...
            </div>
          }>
            {activeTab === 'chat' && <ChatPane />}
            {activeTab === 'settings' && <SettingsPane />}
            {activeTab === 'workflows' && <WorkflowBuilder />}
            {activeTab === 'marketplace' && <AgentManager />}
            {activeTab === 'files' && <FileBrowser />}
            {activeTab === 'knowledge' && <KnowledgeBase />}
            {activeTab === 'monitor' && <AgentMonitor />}

            {!['chat','settings','workflows','marketplace','files','knowledge','monitor'].includes(activeTab) && (
              <div className="flex flex-col items-center justify-center h-full" style={{ color: 'var(--text-secondary)' }}>
                <h2 className="text-2xl font-semibold mb-2">{tabs.find(t => t.id === activeTab)?.label}</h2>
                <p>This module is under development.</p>
              </div>
            )}
          </Suspense>
        </main>
      </div>
    </ErrorBoundary>
  );
}

export default App;
