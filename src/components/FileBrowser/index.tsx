import React, { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Folder, File as FileIcon, HardDrive, ArrowLeft, RefreshCw, Send, Loader2, Bot, User, ChevronRight, Trash2 } from 'lucide-react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import ReactMarkdown from 'react-markdown';
import toast from 'react-hot-toast';
import clsx from 'clsx';
import FilePreview from './FilePreview';
import { useWorkspaceStore } from '../../store/workspaceStore';

interface FileEntry {
  name: string;
  path: string;
  isDir: boolean;
  sizeBytes?: number;
  modifiedAt?: string;
  extension?: string;
}

interface ScanProgress {
  event: string;
  fileName: string;
  fileIndex: number;
  totalFiles: number;
  percent: number;
  stage: string;
}

interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestampMs: number;
  intent?: string;
  agentUsed?: string;
}

export default function FileBrowser() {
  // Tree / File Browser State
  const [currentPath, setCurrentPath] = useState<string>('C:\\');
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [selectedFile, setSelectedFile] = useState<FileEntry | null>(null);
  const [viewMode, setViewMode] = useState<'system' | 'artifacts'>('system');
  
  const activeWorkspaceId = useWorkspaceStore(state => state.activeWorkspaceId);
  const [workspaceRoot, setWorkspaceRoot] = useState<string>('C:\\');

  useEffect(() => {
    const fetchRoot = async () => {
      try {
        const root: string = await invoke('get_workspace_path', { workspaceId: activeWorkspaceId });
        let rootPath = root;
        if (!rootPath.endsWith('\\')) rootPath += '\\';
        setWorkspaceRoot(rootPath);
        setCurrentPath(rootPath);
      } catch (err) {
        console.error('Failed to get workspace root:', err);
      }
    };
    fetchRoot();
  }, [activeWorkspaceId]);
  
  // Chat State
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [isChatLoading, setIsChatLoading] = useState(false);
  const [sessionId, setSessionId] = useState<string | undefined>(undefined);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Scanning State (from agent)
  const [isScanning, setIsScanning] = useState(false);
  const [scanProgress, setScanProgress] = useState<ScanProgress | null>(null);

  useEffect(() => {
    loadDirectory(currentPath);
  }, [currentPath, viewMode]);

  useEffect(() => {
    const unlisten = listen<ScanProgress>('scan_progress', (event) => {
      setScanProgress(event.payload);
      if (event.payload.percent >= 100) {
        setIsScanning(false);
        toast.success('Folder scan completed successfully!');
        loadDirectory(currentPath);
      }
    });

    return () => {
      unlisten.then(f => f());
    };
  }, [currentPath]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, isChatLoading]);

  const loadDirectory = async (path: string) => {
    try {
      setIsLoading(true);
      if (viewMode === 'artifacts') {
        const entries: FileEntry[] = await invoke('list_artifacts');
        setFiles(entries);
      } else {
        const entries: FileEntry[] = await invoke('list_directory', { path });
        setFiles(entries);
      }
    } catch (error) {
      console.error('Failed to load directory:', error);
      toast.error(`Error: ${error}`);
    } finally {
      setIsLoading(false);
    }
  };

  const handleNavigateUp = () => {
    if (currentPath.length <= workspaceRoot.length || currentPath === workspaceRoot) {
      return; // Locked to workspace root
    }
    const parts = currentPath.split('\\').filter(Boolean);
    if (parts.length > 1) {
      parts.pop();
      const newPath = parts.join('\\') + '\\';
      if (newPath.startsWith(workspaceRoot)) {
        setCurrentPath(newPath);
      } else {
        setCurrentPath(workspaceRoot);
      }
      setSelectedFile(null);
    } else {
      setCurrentPath(workspaceRoot);
      setSelectedFile(null);
    }
  };

  const handleDeleteArtifact = async (e: React.MouseEvent, fileName: string) => {
    e.stopPropagation();
    if (window.confirm(`Are you sure you want to delete ${fileName}?`)) {
      try {
        await invoke('delete_file', { filename: fileName });
        toast.success('Artifact deleted');
        if (selectedFile?.name === fileName) {
          setSelectedFile(null);
        }
        loadDirectory(currentPath);
      } catch (error) {
        toast.error(`Failed to delete: ${error}`);
      }
    }
  };

  const handleSendChat = async () => {
    if (!input.trim() || isChatLoading) return;

    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: input.trim(),
      timestampMs: Date.now(),
    };

    setMessages((prev) => [...prev, userMessage]);
    setInput('');
    setIsChatLoading(true);

    try {
      const request = {
        sessionId,
        message: userMessage.content,
        contextFilePath: currentPath, // Passing the current folder as context
      };

      const response: any = await invoke('send_chat_message', { request });
      
      if (!sessionId) {
        setSessionId(response.sessionId);
      }

      const assistantMessage: ChatMessage = {
        id: crypto.randomUUID(),
        role: 'assistant',
        content: response.reply.content,
        timestampMs: response.reply.timestampMs,
        intent: response.intent,
        agentUsed: response.agentUsed,
      };

      setMessages((prev) => [...prev, assistantMessage]);
    } catch (error) {
      console.error('Failed to send message:', error);
      const errorMessage: ChatMessage = {
        id: crypto.randomUUID(),
        role: 'system',
        content: `Error: ${error instanceof Error ? error.message : String(error)}`,
        timestampMs: Date.now(),
      };
      setMessages((prev) => [...prev, errorMessage]);
    } finally {
      setIsChatLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSendChat();
    }
  };



  return (
    <div className="h-full w-full bg-[var(--bg-app)] transition-theme">
      <PanelGroup direction="horizontal">
        {/* LEFT PANEL: Directory Browser */}
        <Panel defaultSize={30} minSize={20}>
          <div className="flex flex-col h-full bg-[var(--bg-sidebar)] border-r border-[var(--border-default)] transition-theme">
            {/* Header & Path Bar */}
            <div className="p-4 border-b border-[var(--border-default)] flex flex-col shrink-0">
              <div className="flex items-center justify-between mb-3">
                <h2 className="text-lg font-bold text-slate-800 dark:text-slate-100">Browser</h2>
                <div className="flex bg-slate-100 dark:bg-slate-800 rounded-lg p-0.5">
                  <button 
                    onClick={() => setViewMode('system')} 
                    className={clsx("px-3 py-1 text-xs font-medium rounded-md transition-all", viewMode === 'system' ? "bg-white dark:bg-slate-700 shadow-sm text-blue-600 dark:text-blue-400" : "text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-200")}
                  >
                    System
                  </button>
                  <button 
                    onClick={() => setViewMode('artifacts')} 
                    className={clsx("px-3 py-1 text-xs font-medium rounded-md transition-all", viewMode === 'artifacts' ? "bg-white dark:bg-slate-700 shadow-sm text-blue-600 dark:text-blue-400" : "text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-200")}
                  >
                    Artifacts
                  </button>
                </div>
              </div>
              
              <div className="flex items-center space-x-2 bg-slate-100 dark:bg-slate-900 rounded-lg p-1.5 border border-slate-200 dark:border-slate-800">
                <button 
                  onClick={handleNavigateUp}
                  className="p-1 rounded hover:bg-slate-200 dark:hover:bg-slate-800 text-slate-600 dark:text-slate-400"
                  title="Go Up"
                >
                  <ArrowLeft size={16} />
                </button>
                
                <div className="flex-1 flex items-center overflow-x-auto no-scrollbar font-medium text-xs text-slate-700 dark:text-slate-300 whitespace-nowrap px-1">
                  <HardDrive size={14} className="mr-1.5 flex-shrink-0" />
                  {viewMode === 'artifacts' ? (
                    <span className="flex-shrink-0">Generated Artifacts</span>
                  ) : (
                    <>
                      <span className="font-semibold flex-shrink-0 text-blue-600 dark:text-blue-400">Workspace</span>
                      {currentPath.substring(workspaceRoot.length).split('\\').filter(Boolean).map((part, idx) => (
                        <div key={idx} className="flex items-center">
                          <ChevronRight size={14} className="mx-0.5 text-slate-400 flex-shrink-0" />
                          <span className="flex-shrink-0 truncate max-w-[80px]" title={part}>{part}</span>
                        </div>
                      ))}
                    </>
                  )}
                </div>
                
                <button 
                  onClick={() => loadDirectory(currentPath)}
                  className="p-1 rounded hover:bg-slate-200 dark:hover:bg-slate-800 text-slate-600 dark:text-slate-400"
                  title="Refresh"
                >
                  <RefreshCw size={16} className={isLoading ? "animate-spin" : ""} />
                </button>
              </div>
            </div>

            {/* File List */}
            <div className="flex-1 overflow-y-auto">
              {isLoading ? (
                <div className="flex items-center justify-center h-full">
                  <RefreshCw className="animate-spin text-slate-400" size={24} />
                </div>
              ) : (
                <div className="w-full">
                  <table className="w-full text-left text-sm text-[var(--text-secondary)]">
                    <tbody className="divide-y divide-[var(--border-default)]">
                      {files.map((file, i) => (
                        <tr 
                          key={i} 
                          onClick={() => {
                            if (file.isDir) {
                              setCurrentPath(file.path);
                              setSelectedFile(null);
                            } else {
                              setSelectedFile(file);
                            }
                          }}
                          className={clsx(
                            "hover:bg-[var(--bg-hover)] transition-colors group",
                            file.isDir ? "cursor-pointer" : "cursor-default"
                          )}
                        >
                          <td className="px-4 py-2.5 font-medium text-[var(--text-primary)] flex items-center justify-between">
                            <div className="flex items-center">
                              {file.isDir ? (
                                <Folder className="mr-3 text-blue-500 flex-shrink-0" size={16} />
                              ) : (
                                <FileIcon className="mr-3 text-[var(--text-muted)] flex-shrink-0" size={16} />
                              )}
                              <div className="truncate max-w-[200px]" title={file.name}>{file.name}</div>
                            </div>
                            {viewMode === 'artifacts' && !file.isDir && (
                              <button 
                                onClick={(e) => handleDeleteArtifact(e, file.name)}
                                className="p-1.5 rounded-md text-[var(--text-muted)] hover:text-red-500 hover:bg-red-500/10 opacity-0 group-hover:opacity-100 transition-all"
                                title="Delete artifact"
                              >
                                <Trash2 size={16} />
                              </button>
                            )}
                          </td>
                        </tr>
                      ))}
                      
                      {files.length === 0 && (
                        <tr>
                          <td className="px-4 py-8 text-center text-[var(--text-muted)] text-sm">
                            Empty folder
                          </td>
                        </tr>
                      )}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          </div>
        </Panel>

        <PanelResizeHandle className="w-1 bg-slate-200 dark:bg-slate-800 hover:bg-blue-400 transition-colors cursor-col-resize" />

        {/* RIGHT PANEL: Preview + Chat Context */}
        <Panel defaultSize={70}>
          <PanelGroup direction="vertical">
            {/* Top: File Preview */}
            <Panel defaultSize={50} minSize={20}>
              <FilePreview file={selectedFile} />
            </Panel>

            <PanelResizeHandle className="h-1 bg-slate-200 dark:bg-slate-800 hover:bg-blue-400 transition-colors cursor-row-resize z-20" />

            {/* Bottom: Chat Area */}
            <Panel defaultSize={50} minSize={20}>
              <div className="flex flex-col h-full bg-[var(--bg-main)] relative transition-theme">
                
                {/* Header */}
                <div className="px-6 py-3 border-b border-[var(--border-default)] bg-[var(--bg-card)] flex flex-col justify-center shrink-0">
                  <h2 className="text-base font-bold text-slate-800 dark:text-slate-100">Folder Actions</h2>
                  <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5 truncate">
                    Context: <span className="font-mono text-slate-700 dark:text-slate-300">{currentPath}</span>
                  </p>
                </div>

                {/* Progress Panel Overlay (if scanning) */}
                {isScanning && scanProgress && (
                  <div className="bg-blue-50 dark:bg-blue-900/20 border-b border-blue-200 dark:border-blue-800 p-4 shrink-0 absolute top-[64px] left-0 right-0 z-10 shadow-sm">
                <div className="max-w-4xl mx-auto flex flex-col space-y-2">
                  <div className="flex justify-between items-center text-sm font-medium text-blue-900 dark:text-blue-100">
                    <span className="flex items-center">
                      <RefreshCw className="animate-spin mr-2" size={16} />
                      Stage: {scanProgress.stage}
                    </span>
                    <span>{Math.round(scanProgress.percent)}% ({scanProgress.fileIndex}/{scanProgress.totalFiles} files)</span>
                  </div>
                  <div className="w-full bg-blue-200 dark:bg-blue-950 rounded-full h-2.5 overflow-hidden">
                    <div 
                      className="bg-blue-600 h-2.5 rounded-full transition-all duration-300 ease-out" 
                      style={{ width: `${scanProgress.percent}%` }}
                    ></div>
                  </div>
                  <div className="text-xs text-blue-700 dark:text-blue-300 truncate">
                    {scanProgress.event === 'file_processing' ? `Processing: ${scanProgress.fileName}` : scanProgress.fileName}
                  </div>
                </div>
              </div>
            )}

            {/* Messages Area */}
            <div className="flex-1 overflow-y-auto p-6 space-y-6">
              {messages.length === 0 ? (
                <div className="flex flex-col items-center justify-center h-full text-slate-500 space-y-4">
                  <div className="w-16 h-16 bg-blue-100 dark:bg-blue-900/30 rounded-2xl flex items-center justify-center">
                    <Bot size={32} className="text-blue-600 dark:text-blue-400" />
                  </div>
                  <p className="text-center text-[var(--text-secondary)] max-w-sm">
                    Chat with AI to analyze, summarize, or aggregate files in <strong>{currentPath}</strong>.
                  </p>
                  <div className="flex flex-wrap justify-center gap-2 mt-4 max-w-lg">
                    <button onClick={() => setInput('Tổng hợp nội dung các file trong thư mục này thành 1 file báo cáo Word')} className="px-3 py-1.5 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg text-sm hover:border-[var(--accent)] transition-colors">
                      Tổng hợp báo cáo Word
                    </button>
                    <button onClick={() => setInput('Trích xuất dữ liệu từ tất cả file Excel trong này')} className="px-3 py-1.5 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg text-sm hover:border-[var(--accent)] transition-colors">
                      Trích xuất dữ liệu Excel
                    </button>
                  </div>
                </div>
              ) : (
                messages.map((msg) => (
                  <div
                    key={msg.id}
                    className={clsx(
                      "flex w-full",
                      msg.role === 'user' ? "justify-end" : "justify-start"
                    )}
                  >
                    <div className={clsx(
                      "flex max-w-[85%] space-x-3",
                      msg.role === 'user' ? "flex-row-reverse space-x-reverse" : "flex-row"
                    )}>
                      {/* Avatar */}
                      <div className="flex-shrink-0 mt-1">
                        {msg.role === 'user' ? (
                          <div className="w-8 h-8 bg-blue-600 rounded-full flex items-center justify-center text-white">
                            <User size={16} />
                          </div>
                        ) : msg.role === 'assistant' ? (
                          <div className="w-8 h-8 bg-gradient-to-br from-indigo-500 to-purple-600 rounded-full flex items-center justify-center text-white">
                            <Bot size={16} />
                          </div>
                        ) : null}
                      </div>
                      
                      {/* Message Bubble */}
                      <div className={clsx(
                        "px-4 py-3 rounded-2xl shadow-sm",
                        msg.role === 'user' 
                          ? "bg-[var(--accent)] text-white rounded-tr-none" 
                          : msg.role === 'system'
                          ? "bg-red-500/10 text-red-500 border border-red-500/30"
                          : "bg-[var(--bg-input)] text-[var(--text-primary)] rounded-tl-none border border-[var(--border-default)]"
                      )}>
                        {msg.role === 'assistant' ? (
                          <div className="prose prose-sm dark:prose-invert max-w-none prose-p:leading-relaxed prose-pre:bg-slate-100 dark:prose-pre:bg-slate-900 prose-pre:border prose-pre:border-slate-200 dark:prose-pre:border-slate-800">
                            <ReactMarkdown>{msg.content}</ReactMarkdown>
                          </div>
                        ) : (
                          <div className="whitespace-pre-wrap text-sm leading-relaxed">{msg.content}</div>
                        )}
                        
                        {msg.role === 'assistant' && (msg.intent || msg.agentUsed) && (
                          <div className="mt-3 pt-3 border-t border-slate-100 dark:border-slate-700 flex flex-wrap gap-2 text-xs">
                            {msg.agentUsed && (
                              <span className="bg-slate-100 dark:bg-slate-700 text-slate-600 dark:text-slate-300 px-2 py-1 rounded-md">
                                Agent: {msg.agentUsed}
                              </span>
                            )}
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                ))
              )}
              
              {isChatLoading && (
                <div className="flex justify-start">
                  <div className="flex space-x-3">
                    <div className="flex-shrink-0 w-8 h-8 bg-gradient-to-br from-indigo-500 to-purple-600 rounded-full flex items-center justify-center text-white">
                      <Bot size={16} />
                    </div>
                    <div className="px-4 py-4 rounded-2xl rounded-tl-none bg-[var(--bg-input)] border border-[var(--border-default)] shadow-sm flex items-center space-x-2">
                      <div className="w-2 h-2 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                      <div className="w-2 h-2 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                      <div className="w-2 h-2 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
                    </div>
                  </div>
                </div>
              )}
              <div ref={messagesEndRef} />
            </div>

            {/* Input Area */}
            <div className="p-4 bg-[var(--bg-card)] border-t border-[var(--border-default)]">
              <div className="relative flex items-end shadow-sm bg-[var(--bg-input)] border border-[var(--border-default)] rounded-2xl overflow-hidden focus-within:ring-2 focus-within:ring-[var(--accent)] focus-within:border-transparent transition-all">
                <textarea
                  value={input}
                  onChange={(e) => setInput(e.target.value)}
                  onKeyDown={handleKeyDown}
                  placeholder={`Action in ${currentPath.split('\\').pop() || 'folder'}...`}
                  className="w-full max-h-32 min-h-[56px] py-4 pl-4 pr-12 bg-transparent border-none resize-none focus:outline-none text-[var(--text-primary)] placeholder-[var(--text-muted)]"
                  rows={1}
                  style={{ minHeight: '56px' }}
                />
                <button
                  onClick={handleSendChat}
                  disabled={!input.trim() || isChatLoading}
                  className="absolute right-2 bottom-2 p-2 bg-blue-600 hover:bg-blue-700 disabled:bg-slate-300 dark:disabled:bg-slate-700 disabled:text-slate-500 text-white rounded-xl transition-colors"
                >
                  {isChatLoading ? <Loader2 size={18} className="animate-spin" /> : <Send size={18} />}
                </button>
              </div>
            </div>

              </div>
            </Panel>
          </PanelGroup>
        </Panel>
      </PanelGroup>
    </div>
  );
}
