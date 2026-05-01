import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Briefcase, Plus, Check, ChevronDown, FolderOpen } from 'lucide-react';
import { useWorkspaceStore } from '../store/workspaceStore';
import clsx from 'clsx';

interface Workspace {
  id: string;
  name: string;
  created_at: number;
}

export default function WorkspaceSwitcher({ sidebarOpen }: { sidebarOpen: boolean }) {
  const { activeWorkspaceId, setActiveWorkspaceId } = useWorkspaceStore();
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [isOpen, setIsOpen] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newWorkspaceName, setNewWorkspaceName] = useState('');
  const dropdownRef = useRef<HTMLDivElement>(null);

  const fetchWorkspaces = async () => {
    try {
      const data: Workspace[] = await invoke('list_workspaces');
      setWorkspaces(data);
    } catch (e) {
      console.error('Failed to fetch workspaces:', e);
    }
  };

  useEffect(() => {
    fetchWorkspaces();
  }, []);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
        setIsCreating(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleCreateWorkspace = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newWorkspaceName.trim()) return;
    
    try {
      const newWs: Workspace = await invoke('create_workspace', { name: newWorkspaceName.trim() });
      await fetchWorkspaces();
      setActiveWorkspaceId(newWs.id);
      setIsCreating(false);
      setNewWorkspaceName('');
      setIsOpen(false);
    } catch (e) {
      console.error('Failed to create workspace:', e);
    }
  };

  const activeWorkspace = workspaces.find(w => w.id === activeWorkspaceId) || workspaces.find(w => w.id === 'default');

  if (!sidebarOpen) {
    return (
      <div className="flex items-center justify-center w-full h-full text-slate-400">
        <Briefcase size={20} />
      </div>
    );
  }

  return (
    <div className="relative w-full" ref={dropdownRef}>
      <button 
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center justify-between w-full p-2.5 bg-white dark:bg-slate-900 rounded-xl hover:bg-slate-100 dark:hover:bg-slate-800 transition-all border border-slate-200 dark:border-slate-800/60 shadow-sm text-left group"
      >
        <div className="flex items-center gap-2 overflow-hidden">
          <Briefcase size={16} className="text-indigo-500 flex-shrink-0" />
          <span className="font-medium text-slate-700 dark:text-slate-200 text-sm truncate group-hover:text-indigo-600 dark:group-hover:text-indigo-400 transition-colors">
            {activeWorkspace?.name || 'Loading...'}
          </span>
        </div>
        <ChevronDown size={14} className="text-slate-400 group-hover:text-indigo-500 transition-colors flex-shrink-0" />
      </button>

      {isOpen && (
        <div className="absolute top-full left-0 w-full mt-2 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800/60 rounded-xl shadow-lg shadow-slate-200/50 dark:shadow-black/50 z-50 overflow-hidden">
          <div className="max-h-60 overflow-y-auto py-2">
            <div className="px-3 py-1 mb-1 text-[10px] font-bold text-slate-400 dark:text-slate-500 uppercase tracking-widest">
              Workspaces
            </div>
            {workspaces.map((ws) => (
              <button
                key={ws.id}
                onClick={() => {
                  setActiveWorkspaceId(ws.id);
                  setIsOpen(false);
                }}
                className={clsx(
                  "flex items-center justify-between w-full px-3 py-2.5 text-sm transition-colors text-left",
                  activeWorkspaceId === ws.id ? "bg-indigo-50 dark:bg-indigo-900/20 text-indigo-700 dark:text-indigo-400 font-medium" : "hover:bg-slate-50 dark:hover:bg-slate-800/50 text-slate-600 dark:text-slate-300"
                )}
              >
                <div className="flex items-center gap-2 truncate">
                  <FolderOpen size={14} className={activeWorkspaceId === ws.id ? "text-indigo-500 dark:text-indigo-400" : "text-slate-400"} />
                  <span className="truncate">{ws.name}</span>
                </div>
                {activeWorkspaceId === ws.id && <Check size={14} className="text-indigo-600 dark:text-indigo-400" />}
              </button>
            ))}
          </div>
          
          <div className="border-t border-slate-200 dark:border-slate-800/60 p-2 bg-slate-50/50 dark:bg-slate-900/50">
            {isCreating ? (
              <form onSubmit={handleCreateWorkspace} className="flex gap-2">
                <input
                  type="text"
                  autoFocus
                  value={newWorkspaceName}
                  onChange={(e) => setNewWorkspaceName(e.target.value)}
                  placeholder="Workspace name..."
                  className="flex-1 bg-white dark:bg-slate-950 border border-slate-200 dark:border-slate-700 rounded-lg px-3 py-1.5 text-sm text-slate-800 dark:text-slate-200 focus:outline-none focus:ring-2 focus:ring-indigo-500/50 focus:border-indigo-500/50 transition-all shadow-sm"
                />
                <button 
                  type="submit"
                  disabled={!newWorkspaceName.trim()}
                  className="bg-indigo-600 hover:bg-indigo-500 disabled:bg-slate-300 dark:disabled:bg-slate-700 disabled:text-slate-500 text-white p-1.5 rounded-lg transition-colors flex items-center justify-center shadow-sm"
                >
                  <Plus size={16} />
                </button>
              </form>
            ) : (
              <button
                onClick={() => setIsCreating(true)}
                className="flex items-center gap-2 text-sm text-slate-500 hover:text-indigo-600 dark:hover:text-indigo-400 w-full py-2 px-2 transition-colors font-medium rounded-lg hover:bg-white dark:hover:bg-slate-800 border border-transparent hover:border-slate-200 dark:hover:border-slate-700"
              >
                <Plus size={16} className="bg-slate-100 dark:bg-slate-800 p-0.5 rounded-md" />
                <span>Create Workspace</span>
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
