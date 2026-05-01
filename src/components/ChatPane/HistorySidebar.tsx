import { useEffect, useState, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { MessageSquare, Folder, ChevronRight, Plus, Trash2, Search, History } from 'lucide-react';
import clsx from 'clsx';

export interface SessionSummaryInfo {
  id: string;
  title: string;
  status: string;
  turnCount: number;
  lastActiveAt: string;
  contextUtilization: number;
  topicId: string | null;
  workspaceId: string | null;
}

interface HistorySidebarProps {
  currentSessionId?: string;
  onSelectSession: (sessionId: string) => void;
  onNewSession: () => void;
}

import { useWorkspaceStore } from '../../store/workspaceStore';

export default function HistorySidebar({ currentSessionId, onSelectSession, onNewSession }: HistorySidebarProps) {
  const [sessions, setSessions] = useState<SessionSummaryInfo[]>([]);
  const [expandedTopics, setExpandedTopics] = useState<Record<string, boolean>>({});
  const [searchQuery, setSearchQuery] = useState('');
  const activeWorkspaceId = useWorkspaceStore(state => state.activeWorkspaceId);

  const fetchSessions = async () => {
    try {
      const list = await invoke<SessionSummaryInfo[]>('list_sessions');
      setSessions(list);
    } catch (e) {
      console.error('Failed to fetch sessions', e);
    }
  };

  useEffect(() => {
    fetchSessions();
  }, []);

  const handleDeleteSession = async (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    try {
      await invoke('delete_session', { sessionId });
      if (currentSessionId === sessionId) {
        onNewSession();
      }
      fetchSessions();
    } catch (err) {
      console.error('Failed to delete session', err);
    }
  };

  const filteredSessions = useMemo(() => {
    // 1. Filter by workspace
    let wsFiltered = sessions;
    if (activeWorkspaceId === 'default' || activeWorkspaceId === 'Global') {
      wsFiltered = sessions.filter(s => !s.workspaceId || s.workspaceId === 'default');
    } else {
      wsFiltered = sessions.filter(s => s.workspaceId === activeWorkspaceId);
    }

    // 2. Filter by search query
    if (!searchQuery.trim()) return wsFiltered;
    
    const lowerQuery = searchQuery.toLowerCase();
    return wsFiltered.filter(
      s => s.title.toLowerCase().includes(lowerQuery) || 
           (s.topicId && s.topicId.toLowerCase().includes(lowerQuery))
    );
  }, [sessions, searchQuery, activeWorkspaceId]);

  // Group by topic
  const grouped: Record<string, SessionSummaryInfo[]> = {};
  const unassigned: SessionSummaryInfo[] = [];

  filteredSessions.forEach(session => {
    if (session.topicId) {
      const topicArr = grouped[session.topicId] || [];
      topicArr.push(session);
      grouped[session.topicId] = topicArr;
    } else {
      unassigned.push(session);
    }
  });

  const toggleTopic = (topic: string) => {
    setExpandedTopics(prev => ({ ...prev, [topic]: !prev[topic] }));
  };

  const renderSessionItem = (session: SessionSummaryInfo) => {
    const isActive = currentSessionId === session.id;
    return (
      <div
        key={session.id}
        onClick={() => onSelectSession(session.id)}
        className={clsx(
          "group relative w-full flex flex-col px-4 py-3 rounded-2xl text-left text-sm transition-all duration-200 cursor-pointer border",
          isActive
            ? "bg-[var(--bg-card)] border-[var(--border-default)] shadow-sm"
            : "bg-transparent border-transparent hover:bg-[var(--bg-hover)]"
        )}
      >
        <div className={clsx(
          "flex items-start gap-3 w-full",
          isActive
            ? "text-[var(--accent)] font-medium"
            : "text-[var(--text-secondary)] group-hover:text-[var(--text-primary)]"
        )}>
          <MessageSquare size={16} className={clsx("flex-shrink-0 mt-0.5", isActive ? "text-[var(--accent)]" : "text-[var(--text-muted)]")} />
          <div className="flex flex-col flex-1 truncate text-left min-w-0">
            <span className="truncate leading-tight">{session.title}</span>
            <span className={clsx(
              "text-[11px] font-normal truncate mt-1 transition-colors",
              isActive ? "text-[var(--accent)]" : "text-[var(--text-muted)]"
            )}>
              {new Date(session.lastActiveAt).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })} 
              {' • '}{session.turnCount} turns
            </span>
          </div>
        </div>
        <button
          onClick={(e) => handleDeleteSession(e, session.id)}
          className="absolute right-2 top-1/2 -translate-y-1/2 p-1.5 opacity-0 group-hover:opacity-100 bg-[var(--bg-main)] hover:bg-red-500/10 text-[var(--text-muted)] hover:text-red-500 backdrop-blur-sm rounded-md transition-all shadow-sm"
          title="Delete Session"
        >
          <Trash2 size={14} />
        </button>
      </div>
    );
  };

  return (
    <div className="w-[280px] border-r border-[var(--border-default)] bg-[var(--bg-sidebar)] flex flex-col h-full z-10 shadow-sm transition-theme">
      {/* Header */}
      <div className="p-4 flex flex-col gap-4">
        <div className="flex justify-between items-center px-1">
          <div className="flex items-center gap-2">
            <History size={18} className="text-[var(--text-muted)]" />
            <h3 className="font-semibold text-[var(--text-primary)] tracking-tight">History</h3>
          </div>
          <button
            onClick={onNewSession}
            className="p-1.5 rounded-xl hover:bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:text-[var(--accent)] transition-colors"
            title="New Chat"
          >
            <Plus size={18} />
          </button>
        </div>
        
        {/* Search Bar */}
        <div className="relative flex items-center">
          <Search size={16} className="absolute left-3.5 text-[var(--text-muted)]" />
          <input 
            type="text"
            placeholder="Search sessions..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full bg-[var(--bg-input)] border border-[var(--border-default)] text-sm rounded-2xl pl-10 pr-4 py-2.5 text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] transition-all shadow-sm"
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-3 pb-4 space-y-4">
        {Object.entries(grouped).map(([topic, groupSessions]) => {
          const isExpanded = expandedTopics[topic] ?? true;
          return (
            <div key={topic} className="flex flex-col">
              <button
                onClick={() => toggleTopic(topic)}
                className="w-full flex items-center gap-2 px-3 py-2 text-[11px] font-bold text-[var(--text-secondary)] uppercase tracking-widest hover:text-[var(--text-primary)] rounded-lg transition-colors group"
              >
                <ChevronRight 
                  size={14} 
                  className={clsx("transition-transform duration-200 text-[var(--text-muted)] group-hover:text-[var(--text-primary)]", isExpanded && "rotate-90")} 
                />
                <Folder size={14} className="text-indigo-400" />
                <span className="truncate">{topic}</span>
                <span className="ml-auto text-[10px] font-medium bg-[var(--bg-input)] text-[var(--text-secondary)] px-2 py-0.5 rounded-md border border-[var(--border-default)]">
                  {groupSessions.length}
                </span>
              </button>
              
              <div 
                className={clsx(
                  "grid transition-all duration-300 ease-in-out",
                  isExpanded ? "grid-rows-[1fr] opacity-100" : "grid-rows-[0fr] opacity-0"
                )}
              >
                <div className="overflow-hidden">
                  <div className="pl-4 pr-1 mt-1 space-y-1 relative before:absolute before:left-6 before:top-2 before:bottom-2 before:w-[1px] before:bg-[var(--border-default)]">
                    {groupSessions.map(renderSessionItem)}
                  </div>
                </div>
              </div>
            </div>
          );
        })}

        {unassigned.length > 0 && (
          <div className="flex flex-col mt-4">
            <div className="px-3 py-2 text-xs font-semibold text-[var(--text-secondary)] uppercase tracking-wider flex items-center justify-between">
              <span>Recent</span>
              <span className="text-[10px] font-medium bg-[var(--bg-input)] text-[var(--text-muted)] border border-[var(--border-default)] px-1.5 py-0.5 rounded-full">
                {unassigned.length}
              </span>
            </div>
            <div className="px-1 mt-1 space-y-1">
              {unassigned.map(renderSessionItem)}
            </div>
          </div>
        )}

        {sessions.length === 0 && !searchQuery && (
          <div className="flex flex-col items-center justify-center py-10 px-4 text-center">
            <div className="w-12 h-12 bg-[var(--bg-input)] rounded-full flex items-center justify-center mb-3 border border-[var(--border-default)]">
              <MessageSquare size={20} className="text-[var(--text-muted)]" />
            </div>
            <p className="text-sm font-medium text-[var(--text-primary)]">No chat history</p>
            <p className="text-xs text-[var(--text-secondary)] mt-1">Start a new conversation to see it here.</p>
          </div>
        )}

        {searchQuery && filteredSessions.length === 0 && (
          <div className="text-center py-10 px-4 text-sm text-slate-500">
            No sessions found matching "{searchQuery}"
          </div>
        )}
      </div>
    </div>
  );
}
