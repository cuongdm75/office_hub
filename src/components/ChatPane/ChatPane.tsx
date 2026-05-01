import React, { useState, useRef, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Send, Bot, User, Loader2, Paperclip, X } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import clsx from 'clsx';
import ReactMarkdown from 'react-markdown';
import HistorySidebar from './HistorySidebar';
import { useWorkspaceStore } from '../../store/workspaceStore';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestampMs: number;
  intent?: string;
  agentUsed?: string;
  tokensUsed?: number;
}

interface WorkflowProgressUpdate {
  type: 'run' | 'step' | 'thought';
  run_id: string;
  workflow_id: string;
  step_id?: string;
  step_name?: string;
  status?: string; // 'pending' | 'running' | 'success' | 'failed' | 'aborted'
  message?: string;
  thought?: string;
  updated_at?: string;
}

interface SendChatRequest {
  sessionId?: string;
  message: string;
  contextFilePath?: string;
  workspaceId?: string;
}

interface SendChatResponse {
  sessionId: string;
  reply: {
    role: string;
    content: string;
    timestampMs: number;
  };
  intent?: string;
  agentUsed?: string;
  tokensUsed?: number;
}

export default function ChatPane() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [sessionId, setSessionId] = useState<string | undefined>(undefined);
  const [attachedFile, setAttachedFile] = useState<string | null>(null);
  const [activeTasks, setActiveTasks] = useState<Record<string, WorkflowProgressUpdate>>({});
  const [llmThought, setLlmThought] = useState<string | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  
  // Mention states
  const activeWorkspaceId = useWorkspaceStore(state => state.activeWorkspaceId);
  const [showMention, setShowMention] = useState(false);
  const [mentionQuery, setMentionQuery] = useState('');
  const [workspaceFiles, setWorkspaceFiles] = useState<any[]>([]);
  const [mentionCursorIndex, setMentionCursorIndex] = useState(-1);

  const handleAttachFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        title: 'Attach File',
      });
      if (selected && typeof selected === 'string') {
        setAttachedFile(selected);
      }
    } catch (err) {
      console.error('Failed to open file dialog', err);
    }
  };

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages, isLoading, llmThought]);

  // Fetch workspace files for mention when workspace changes
  useEffect(() => {
    if (activeWorkspaceId && activeWorkspaceId !== 'default' && activeWorkspaceId !== 'Global') {
      invoke<any[]>('list_knowledge', { workspaceId: activeWorkspaceId })
        .then(files => {
          setWorkspaceFiles(files || []);
        })
        .catch(console.error);
    } else {
      setWorkspaceFiles([]);
    }
  }, [activeWorkspaceId]);

  useEffect(() => {
    const unlistenPromise = listen<{ session_id: string; message: ChatMessage }>('chat_message_received', (event) => {
      const { session_id, message } = event.payload;
      setSessionId((prevSessionId) => {
        if (!prevSessionId || prevSessionId === session_id) {
          setMessages((prev) => {
            // Avoid duplicate messages
            if (!prev.find((m) => m.id === message.id)) {
               return [...prev, message];
            }
            return prev;
          });
          return session_id;
        }
        return prevSessionId; // Ignored if it's for another session
      });
    });

    const unlistenProgress = listen<WorkflowProgressUpdate>('workflow_progress', (event) => {
      const update = event.payload;
      if (update.type === 'thought') {
        setLlmThought(update.thought || null);
      } else if (update.type === 'step' && update.step_id && update.status) {
        setActiveTasks((prev) => {
          const newTasks = { ...prev };
          if (['success', 'failed', 'aborted'].includes(update.status!.toLowerCase())) {
            delete newTasks[update.step_id!];
          } else {
            newTasks[update.step_id!] = update;
          }
          return newTasks;
        });
      }
    });

    return () => {
      unlistenPromise.then((fn) => fn());
      unlistenProgress.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (sessionId) {
      setIsLoading(true);
      setActiveTasks({});
      invoke<ChatMessage[]>('get_session_history', { sessionId })
        .then(history => {
          setMessages(history);
        })
        .catch(err => {
          console.error("Failed to fetch session history", err);
        })
        .finally(() => {
          setIsLoading(false);
        });
    } else {
      setMessages([]);
    }
  }, [sessionId]);

  const handleSend = async () => {
    if (!input.trim() || isLoading) return;

    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: input.trim(),
      timestampMs: Date.now(),
    };

    setMessages((prev) => [...prev, userMessage]);
    setInput('');
    setIsLoading(true);
    setActiveTasks({});

    try {
      const request: SendChatRequest = {
        sessionId,
        message: userMessage.content,
        contextFilePath: attachedFile || undefined,
        workspaceId: (activeWorkspaceId && activeWorkspaceId !== 'default' && activeWorkspaceId !== 'Global') ? activeWorkspaceId : undefined,
      };

      setAttachedFile(null);

      const response: SendChatResponse = await invoke('send_chat_message', { request });
      
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
        tokensUsed: response.tokensUsed,
      };

      setMessages((prev) => [...prev, assistantMessage]);
      setLlmThought(null); // Clear thought when response arrives
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
      setIsLoading(false);
      setActiveTasks({});
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (showMention) {
      if (e.key === 'Escape') {
        setShowMention(false);
        return;
      }
      // Simple handling for mention selection (can be expanded)
    }
    
    if (e.key === 'Enter' && !e.shiftKey && !showMention) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = e.target.value;
    setInput(val);
    
    // Detect @ mention
    if (activeWorkspaceId !== 'default' && activeWorkspaceId !== 'Global') {
      const cursor = e.target.selectionStart;
      const textBeforeCursor = val.slice(0, cursor);
      const match = textBeforeCursor.match(/@([a-zA-Z0-9_.-]*)$/);
      
      if (match) {
        setShowMention(true);
        setMentionQuery(match[1]!.toLowerCase());
        setMentionCursorIndex(cursor - match[1]!.length - 1);
      } else {
        setShowMention(false);
      }
    }
  };

  const handleSelectMention = (fileName: string) => {
    const before = input.slice(0, mentionCursorIndex);
    // Assuming cursor is at the end of the query, we need to find where the query ends
    // For simplicity, just replace the current query part
    const currentCursor = input.indexOf('@', mentionCursorIndex);
    const after = input.slice(currentCursor + mentionQuery.length + 1);
    
    setInput(`${before}@${fileName} ${after}`);
    setShowMention(false);
    
    // Attempt to focus and move cursor back
    setTimeout(() => {
      const ta = document.querySelector('textarea');
      if (ta) ta.focus();
    }, 10);
  };

  const filteredFiles = workspaceFiles.filter(f => f.name.toLowerCase().includes(mentionQuery));

  return (
    <div className="flex h-full w-full">
      <HistorySidebar
        currentSessionId={sessionId}
        onSelectSession={setSessionId}
        onNewSession={() => {
          setSessionId(undefined);
          setMessages([]);
        }}
      />
      
      <div className="flex-1 flex flex-col h-full bg-[var(--bg-app)] border-l border-[var(--border-default)] relative transition-theme">
        {/* Header */}
      <div className="px-6 py-4 border-b border-[var(--border-default)] bg-[var(--bg-main)]/80 backdrop-blur-md flex items-center justify-between z-10 sticky top-0">
        <div>
          <h2 className="text-lg font-bold text-[var(--text-primary)] tracking-tight">Office Hub Assistant</h2>
          <p className="text-xs text-slate-500 dark:text-slate-400 font-medium mt-0.5 flex items-center gap-2">
            <span className="w-1.5 h-1.5 rounded-full bg-green-500"></span>
            Automated Workspace Ready
          </p>
        </div>
      </div>

      {/* Messages Area */}
      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        {messages.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-slate-500 space-y-4">
            <div className="w-16 h-16 bg-[var(--bg-card)] rounded-full flex items-center justify-center shadow-sm border border-[var(--border-default)] mb-4">
              <Bot size={32} className="text-[var(--accent)]" />
            </div>
            <p className="text-center text-[var(--text-secondary)] max-w-sm">
              Hello! I'm your Office Hub assistant. How can I help you today?
            </p>
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
                    <div className="w-8 h-8 bg-indigo-600 rounded-full flex items-center justify-center text-white shadow-sm">
                      <User size={16} />
                    </div>
                  ) : msg.role === 'assistant' ? (
                    <div className="w-8 h-8 bg-white dark:bg-slate-800 border border-slate-200/60 dark:border-slate-700 rounded-full flex items-center justify-center text-indigo-600 dark:text-indigo-400 shadow-sm">
                      <Bot size={16} />
                    </div>
                  ) : null}
                </div>
                
                {/* Message Bubble */}
                <div className={clsx(
                  "px-5 py-3.5 shadow-sm text-sm transition-all",
                  msg.role === 'user' 
                    ? "bg-[var(--accent)] text-white rounded-[24px] rounded-tr-sm" 
                    : msg.role === 'system'
                    ? "bg-red-500/10 text-red-500 border border-red-500/30 rounded-2xl"
                    : "bg-[var(--bg-card)] text-[var(--text-primary)] rounded-[24px] rounded-tl-sm border border-[var(--border-default)]"
                )}>
                  {msg.role === 'assistant' ? (
                    <div className="prose prose-sm dark:prose-invert max-w-none prose-p:leading-relaxed prose-pre:bg-slate-50 dark:prose-pre:bg-slate-950 prose-pre:border prose-pre:border-slate-200/60 dark:prose-pre:border-slate-800/60 prose-pre:rounded-xl">
                      <ReactMarkdown>{msg.content}</ReactMarkdown>
                    </div>
                  ) : (
                    <div className="whitespace-pre-wrap text-sm leading-relaxed">{msg.content}</div>
                  )}

                  {/* Metadata for assistant messages */}
                  {msg.role === 'assistant' && (msg.intent || msg.agentUsed) && (
                    <div className="mt-3 pt-3 border-t border-slate-100 dark:border-slate-700 flex flex-wrap gap-2 text-xs">
                      {msg.agentUsed && (
                        <span className="bg-slate-100 dark:bg-slate-700 text-slate-600 dark:text-slate-300 px-2 py-1 rounded-md">
                          Agent: {msg.agentUsed}
                        </span>
                      )}
                      {msg.intent && (
                        <span className="bg-indigo-50 dark:bg-indigo-900/30 text-indigo-600 dark:text-indigo-400 px-2 py-1 rounded-md">
                          Intent: {msg.intent}
                        </span>
                      )}
                    </div>
                  )}
                </div>
              </div>
            </div>
          ))
        )}
        
        {/* Render LLM Thought */}
        {llmThought && (
          <div className="flex w-full mb-6 justify-start">
            <div className="w-8 h-8 bg-white dark:bg-slate-800 border border-slate-200/60 dark:border-slate-700 rounded-full flex items-center justify-center text-indigo-600 dark:text-indigo-400 shadow-sm mr-3 flex-shrink-0 mt-1">
              <Bot size={16} />
            </div>
            <div className="max-w-[80%] rounded-[24px] rounded-tl-sm px-5 py-4 shadow-sm bg-slate-100 dark:bg-slate-800/50 text-slate-700 dark:text-slate-300 border border-slate-200/50 dark:border-slate-700/50">
              <div className="text-xs text-indigo-600 dark:text-indigo-400 font-semibold mb-2 uppercase tracking-wider flex items-center gap-2">
                <Loader2 size={12} className="animate-spin" />
                Suy nghĩ...
              </div>
              <div className="text-sm italic opacity-90 leading-relaxed">
                {llmThought}
              </div>
            </div>
          </div>
        )}

        {isLoading && (
          <div className="flex justify-start">
            <div className="flex space-x-3">
              <div className="w-8 h-8 bg-white dark:bg-slate-800 border border-slate-200/60 dark:border-slate-700 rounded-full flex items-center justify-center text-indigo-600 dark:text-indigo-400 shadow-sm flex-shrink-0">
                <Bot size={16} />
              </div>
              <div className="flex flex-col space-y-2 max-w-[80%]">
                {Object.values(activeTasks).map((task) => (
                  <div key={task.step_id} className="px-4 py-2.5 bg-white dark:bg-slate-900 rounded-2xl text-xs font-medium text-slate-600 dark:text-slate-400 flex items-center space-x-2 border border-slate-200/60 dark:border-slate-800/60 shadow-sm">
                    <Loader2 size={12} className="animate-spin text-indigo-500 flex-shrink-0" />
                    <span className="truncate">{task.message || `Running ${task.step_name}...`}</span>
                  </div>
                ))}
                {Object.keys(activeTasks).length === 0 && (
                  <div className="px-5 py-4 rounded-[24px] rounded-tl-sm bg-white dark:bg-slate-900 shadow-sm border border-slate-200/60 dark:border-slate-800/60 flex items-center space-x-2">
                    <div className="w-1.5 h-1.5 bg-indigo-500 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                    <div className="w-1.5 h-1.5 bg-indigo-500 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                    <div className="w-1.5 h-1.5 bg-indigo-500 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
                  </div>
                )}
              </div>
            </div>
          </div>
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* Input Area */}
      <div className="p-4 bg-[var(--bg-main)] border-t border-[var(--border-default)] z-10 transition-theme">
        <div className="max-w-4xl mx-auto relative">
          {attachedFile && (
            <div className="mb-3 flex items-center absolute -top-12 left-0">
              <div className="inline-flex items-center space-x-2 bg-white dark:bg-slate-800 text-indigo-700 dark:text-indigo-400 px-3 py-1.5 rounded-xl text-xs font-medium border border-slate-200 dark:border-slate-700 shadow-sm">
                <Paperclip size={14} />
                <span className="truncate max-w-[200px]" title={attachedFile}>
                  {attachedFile.split(/[/\\]/).pop()}
                </span>
                <button
                  onClick={() => setAttachedFile(null)}
                  className="p-1 hover:bg-slate-100 dark:hover:bg-slate-700 rounded-full transition-colors ml-1"
                >
                  <X size={14} />
                </button>
              </div>
            </div>
          )}
          
          {/* Mention Popup */}
          {showMention && filteredFiles.length > 0 && (
            <div className="absolute bottom-[100%] left-4 mb-2 w-72 max-h-48 overflow-y-auto bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl shadow-xl z-50">
              <div className="px-4 py-2 border-b border-slate-100 dark:border-slate-800/50 bg-slate-50 dark:bg-slate-800/50 sticky top-0">
                <span className="text-xs font-semibold text-slate-500 dark:text-slate-400 uppercase tracking-widest">Workspace Files</span>
              </div>
              <ul className="py-2">
                {filteredFiles.map((file, i) => (
                  <li 
                    key={i}
                    onClick={() => handleSelectMention(file.name)}
                    className="px-4 py-2.5 text-sm text-slate-700 dark:text-slate-300 hover:bg-indigo-50 dark:hover:bg-slate-800 cursor-pointer flex items-center gap-3 truncate transition-colors"
                  >
                    <span className="text-[10px] font-medium bg-slate-100 dark:bg-slate-800 px-2 py-0.5 rounded-md text-slate-500">{file.category || 'doc'}</span>
                    {file.name}
                  </li>
                ))}
              </ul>
            </div>
          )}

          <div className="relative flex items-end shadow-sm bg-[var(--bg-input)] border border-[var(--border-default)] rounded-[28px] overflow-hidden focus-within:ring-2 focus-within:ring-[var(--accent)] focus-within:border-transparent transition-all">
            <button
              onClick={handleAttachFile}
              className="flex-shrink-0 p-4 text-slate-400 hover:text-indigo-600 dark:hover:text-indigo-400 transition-colors self-end"
              title="Attach file"
            >
              <Paperclip size={20} />
            </button>
            <textarea
              value={input}
              onChange={handleInputChange}
              onKeyDown={handleKeyDown}
              placeholder="Ask anything or use @ to mention files..."
              className="w-full max-h-32 min-h-[56px] py-4 pl-2 pr-14 bg-transparent border-none resize-none focus:outline-none text-sm text-[var(--text-primary)] placeholder-[var(--text-muted)]"
              rows={1}
              style={{ minHeight: '56px' }}
            />
            <button
              onClick={handleSend}
              disabled={!input.trim() || isLoading}
              className="absolute right-2 bottom-2 p-2.5 bg-[var(--accent)] hover:bg-[var(--accent-hover)] disabled:bg-slate-200 dark:disabled:bg-slate-800 disabled:text-slate-400 text-white rounded-[20px] transition-colors"
            >
              {isLoading ? <Loader2 size={20} className="animate-spin" /> : <Send size={20} />}
            </button>
          </div>
          <div className="text-center mt-3">
            <span className="text-[11px] font-medium text-slate-400 dark:text-slate-500 tracking-wide">Shift + Enter for new line • Enter to send</span>
          </div>
        </div>
      </div>
      </div>
    </div>
  );
}
