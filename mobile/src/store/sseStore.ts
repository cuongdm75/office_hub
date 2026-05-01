// ============================================================================
// Office Hub – sseStore.ts
//
// MCP-Hybrid Transport: SSE (downlink) + REST (uplink)
//
// Downlink: react-native-sse EventSource → GET /api/v1/stream
// Uplink:   fetch POST → /api/v1/command | /api/v1/tool_call | /api/v1/files/upload
//
// react-native-sse is used because the browser's native EventSource
// does NOT exist in React Native's JS environment.
// ============================================================================

import { create } from 'zustand';
import EventSource from 'react-native-sse';
import * as SecureStore from 'expo-secure-store';

/** Collision-safe ID: timestamp prefix + full random suffix */
const uid = () => `${Date.now().toString(36)}_${Math.random().toString(36).substring(2)}`;

// Max messages kept in JS memory. Older messages are trimmed to prevent
// React Native heap overflow during long sessions (Tier C memory fix).
const MAX_MESSAGES = 200;

// ── Domain types ─────────────────────────────────────────────────────────────

export interface HitlRequest {
  action_id: string;
  description: string;
  risk_level: string;
  timeout_seconds: number;
  payload?: any;
}

export interface ChatMessage {
  id: string;
  text: string;
  sender: 'user' | 'agent';
  agent_used?: string;
  timestamp: string;
  metadata?: any;
}

export interface WorkflowStatus {
  run_id: string;
  workflow_id: string;
  workflow_name?: string;
  step_name?: string;
  status: string;
  message?: string;
  updated_at: string;
}

export interface Session {
  id: string;
  title: string;
  lastActive: string;
  workspaceId?: string | null;
}

export interface Workspace {
  id: string;
  name: string;
  created_at: number;
}

// ── SSE event shape (matches SseEvent on backend) ───────────────────────────

interface SseEvent {
  event_type: string;
  call_id?: string;
  payload: any;
}

// ── Store state ──────────────────────────────────────────────────────────────

interface SseState {
  // Connection
  baseUrls: string[];
  currentBaseUrl: string;
  token: string;
  isConnected: boolean;
  isConnecting: boolean;
  error: string | null;

  // Domain state
  activeHitlRequest: HitlRequest | null;
  messages: ChatMessage[];
  llmThought: string | null;
  activeTasks: Record<string, WorkflowStatus>;
  sessions: Session[];
  currentSessionId: string | null;
  workspaces: Workspace[];
  activeWorkspaceId: string | null;

  // Actions
  connect: (urls: string[], token: string) => void;
  disconnect: () => void;
  sendCommand: (text: string, attachedFile?: { name: string; base64?: string; file_path?: string } | null, sessionId?: string) => void;
  sendVoiceCommand: (audioBase64: string, sessionId?: string) => void;
  sendHitlResponse: (requestId: string, approved: boolean, input?: string) => void;
  listSessions: () => Promise<void>;
  getSessionHistory: (sessionId: string) => Promise<void>;
  deleteSession: (sessionId: string) => void;
  setCurrentSessionId: (sessionId: string | null) => void;
  listWorkspaces: () => void;
  setActiveWorkspaceId: (workspaceId: string | null) => void;
  fetchWorkspaceFiles: (workspaceId: string) => Promise<any[]>;
  clearError: () => void;
  initStore: () => Promise<void>;
}

// ── Module-level refs ────────────────────────────────────────────────────────

let _es: InstanceType<typeof EventSource> | null = null;
let _reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let _deliberateDisconnect = false;
let _reconnectAttempts = 0;

const MAX_RECONNECT_ATTEMPTS = 10;
const MAX_RECONNECT_DELAY_MS = 30_000;

function clearTimers() {
  if (_reconnectTimer) { clearTimeout(_reconnectTimer); _reconnectTimer = null; }
}

function closeStream() {
  if (_es) { _es.close(); _es = null; }
}

// ── REST helper ───────────────────────────────────────────────────────────────

async function restPost(url: string, token: string, body: unknown): Promise<any> {
  try {
    const res = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
      body: JSON.stringify(body),
    });
    return res.ok ? await res.json() : null;
  } catch (e) {
    console.warn('[SSE] REST POST failed:', e);
    return null;
  }
}

async function restGet(url: string, token: string): Promise<any> {
  try {
    const res = await fetch(url, {
      method: 'GET',
      headers: {
        'Authorization': `Bearer ${token}`,
      },
    });
    return res.ok ? await res.json() : null;
  } catch (e) {
    console.warn('[SSE] REST GET failed:', e);
    return null;
  }
}

// ── Store ─────────────────────────────────────────────────────────────────────

export const useSseStore = create<SseState>((set, get) => ({
  baseUrls: [],
  currentBaseUrl: '',
  token: '',
  isConnected: false,
  isConnecting: false,
  error: null,
  activeHitlRequest: null,
  messages: [],
  llmThought: null,
  activeTasks: {},
  sessions: [],
  currentSessionId: null,
  workspaces: [],
  activeWorkspaceId: null,

  // ── Connect ───────────────────────────────────────────────────────────────

  initStore: async () => {
    try {
      const id = await SecureStore.getItemAsync('currentSessionId');
      if (id) {
        set({ currentSessionId: id });
      }
    } catch (e) {
      console.warn('[SSE] Failed to load session ID from storage', e);
    }
  },

  connect: (urls: string[], token: string) => {
    _deliberateDisconnect = false;
    _reconnectAttempts = 0;
    closeStream();
    clearTimers();

    if (!urls || urls.length === 0) {
      set({ error: 'No server URL configured.' });
      return;
    }

    const tryConnect = (urlIndex: number) => {
      if (urlIndex >= urls.length) {
        urlIndex = 0;
        _reconnectAttempts++;
        if (_reconnectAttempts > MAX_RECONNECT_ATTEMPTS) {
          set({
            error: `Cannot connect.\n\nTried:\n• ${urls.join('\n• ')}\n\nPlease check your network.`,
            isConnecting: false,
          });
          return;
        }
        const delay = Math.min(Math.pow(2, _reconnectAttempts) * 1000, MAX_RECONNECT_DELAY_MS);
        console.log(`[SSE] Backoff retry ${_reconnectAttempts}/${MAX_RECONNECT_ATTEMPTS} in ${delay / 1000}s`);
        _reconnectTimer = setTimeout(() => tryConnect(0), delay);
        return;
      }

      const rawUrl = urls[urlIndex];
      const baseUrl = rawUrl
        .replace(/^ws:\/\//, 'http://')
        .replace(/^wss:\/\//, 'https://')
        .replace(/\/$/, '');

      const sseUrl = `${baseUrl}/api/v1/stream`;
      console.log(`[SSE] Connecting to ${sseUrl}`);
      set({ isConnecting: true, currentBaseUrl: baseUrl, baseUrls: urls, token, error: null });

      closeStream();

      // react-native-sse v1.x does NOT support the headers option.
      // Pass token via ?token= query param — backend accepts both
      // Authorization header AND ?token= (see check_auth_full in sse_server.rs).
      const { currentSessionId } = get();
      const sseUrlWithToken = `${sseUrl}?token=${encodeURIComponent(token)}${currentSessionId ? `&session_id=${encodeURIComponent(currentSessionId)}` : ''}`;
      const es = new EventSource(sseUrlWithToken);
      _es = es;

      // Timeout: 15s to allow for slow LAN/Tailscale connections
      const connTimeout = setTimeout(() => {
        if (_es === es) {
          console.warn('[SSE] Connection timeout — trying next URL');
          es.close();
          _es = null;
          set({ isConnecting: false });
          tryConnect(urlIndex + 1);
        }
      }, 15000);

      es.addEventListener('open', () => {
        clearTimeout(connTimeout);
        _reconnectAttempts = 0;
        console.log(`[SSE] Connected to ${baseUrl}`);
        set({ isConnected: true, isConnecting: false, error: null });
      });

      es.addEventListener('error', (e: any) => {
        clearTimeout(connTimeout);
        if (_deliberateDisconnect) return;
        console.warn(`[SSE] Error on ${baseUrl}:`, e?.message);
        es.close();
        _es = null;
        set({ isConnected: false, isConnecting: false });
        tryConnect(urlIndex + 1);
      });

      // Named SSE events from backend
      const knownEvents = ['connected', 'status', 'log', 'result', 'progress', 'approval_request', 'error', 'session_list', 'session_history'];
      for (const evtType of knownEvents) {
        es.addEventListener(evtType, (e: any) => {
          try {
            const evt: SseEvent = JSON.parse(e.data);
            dispatchSseEvent(evtType, evt, set, get);
          } catch (err) {
            console.warn('[SSE] Parse error:', err);
          }
        });
      }

      // Fallback for unnamed messages
      es.addEventListener('message', (e: any) => {
        try {
          const evt: SseEvent = JSON.parse(e.data);
          dispatchSseEvent(evt.event_type || 'unknown', evt, set, get);
        } catch (err) {
          console.warn('[SSE] Fallback parse error:', err);
        }
      });
    };

    tryConnect(0);
  },

  // ── Disconnect ────────────────────────────────────────────────────────────

  disconnect: () => {
    _deliberateDisconnect = true;
    clearTimers();
    closeStream();
    set({
      isConnected: false,
      isConnecting: false,
      baseUrls: [],
      currentBaseUrl: '',
      token: '',
      error: null,
      activeHitlRequest: null,
      messages: [],
      activeTasks: {},
      sessions: [],
      currentSessionId: null,
      workspaces: [],
      activeWorkspaceId: null,
    });
  },

  // ── Send command ──────────────────────────────────────────────────────────

  sendCommand: (text: string, attachedFile?: { name: string; base64?: string; file_path?: string } | null, sessionId?: string) => {
    const { currentBaseUrl, token, currentSessionId } = get();
    if (!currentBaseUrl) { set({ error: 'Cannot send: Not connected' }); return; }

    const commandId = uid();
    const resolvedSessionId = sessionId || currentSessionId || undefined;

    set((s) => {
      const updated = [...s.messages, {
        id: commandId,
        text: attachedFile ? `[Attached: ${attachedFile.name}]\n${text}` : text,
        sender: 'user' as const,
        timestamp: new Date().toISOString(),
      }];
      return {
        messages: updated.length > MAX_MESSAGES ? updated.slice(-MAX_MESSAGES) : updated,
      };
    });

    const context = attachedFile
      ? { file_name: attachedFile.name, file_base64: attachedFile.base64, file_path: attachedFile.file_path }
      : undefined;

    restPost(`${currentBaseUrl}/api/v1/command`, token, {
      command_id: commandId,
      session_id: resolvedSessionId || null,
      text,
      context,
    });
  },

  // ── Voice command ─────────────────────────────────────────────────────────

  sendVoiceCommand: (audioBase64: string, sessionId?: string) => {
    const { currentBaseUrl, token, currentSessionId } = get();
    if (!currentBaseUrl) { set({ error: 'Cannot send voice: Not connected' }); return; }

    const callId = uid();
    set((s) => ({
      messages: [...s.messages, {
        id: callId,
        text: '🎤 [Voice Command]',
        sender: 'user' as const,
        timestamp: new Date().toISOString(),
      }],
    }));

    restPost(`${currentBaseUrl}/api/v1/tool_call`, token, {
      call_id: callId,
      tool_name: 'voice_command',
      arguments: { audio_base64: audioBase64, session_id: sessionId || currentSessionId || null },
    });
  },

  // ── HITL response ─────────────────────────────────────────────────────────

  sendHitlResponse: (requestId: string, approved: boolean, input?: string) => {
    const { currentBaseUrl, token } = get();
    if (!currentBaseUrl) { set({ error: 'Cannot send: Not connected' }); return; }

    restPost(`${currentBaseUrl}/api/v1/tool_call`, token, {
      call_id: requestId,
      tool_name: 'hitl_response',
      arguments: { action_id: requestId, approved, reason: input || '', responded_by: 'mobile_user' },
    });
    set({ activeHitlRequest: null });
  },

  // ── Session management ────────────────────────────────────────────────────

  listSessions: async () => {
    const { currentBaseUrl, token } = get();
    if (!currentBaseUrl) return;
    try {
      const res = await restGet(`${currentBaseUrl}/api/v1/sessions`, token);
      if (res && res.sessions) {
        const mappedSessions = res.sessions.map((s: any) => ({
          id: s.id,
          title: s.title,
          lastActive: s.last_active_at || s.lastActive || new Date().toISOString(),
        }));
        set({ sessions: mappedSessions });
      }
    } catch (e) {
      console.warn('[REST] Failed to fetch sessions:', e);
    }
  },

  getSessionHistory: async (sessionId: string) => {
    const { currentBaseUrl, token } = get();
    if (!currentBaseUrl) return;
    try {
      const res = await restGet(`${currentBaseUrl}/api/v1/sessions/${sessionId}/history`, token);
      if (res && res.messages) {
        if (get().currentSessionId === sessionId) {
          const mappedMessages = res.messages.map((m: any, i: number) => ({
            id: m.id || `hist_${i}_${uid()}`,
            text: m.content || '',
            sender: m.role === 'user' ? 'user' : 'agent',
            agent_used: m.agent_used,
            timestamp: m.timestamp_ms ? new Date(m.timestamp_ms).toISOString() : new Date().toISOString(),
          }));
          set({ messages: mappedMessages.length > MAX_MESSAGES ? mappedMessages.slice(-MAX_MESSAGES) : mappedMessages });
        }
      }
    } catch (e) {
      console.warn('[REST] Failed to fetch session history:', e);
    }
  },

  deleteSession: (sessionId: string) => {
    const { currentBaseUrl, token, currentSessionId } = get();
    if (!currentBaseUrl) return;
    restPost(`${currentBaseUrl}/api/v1/command`, token, {
      command_id: `delete_session_${sessionId}`,
      session_id: sessionId,
      text: '__DELETE_SESSION__',
    });
    set((s) => ({
      sessions: s.sessions.filter((ses) => ses.id !== sessionId),
      ...(s.currentSessionId === sessionId ? { currentSessionId: null, messages: [] } : {}),
    }));
  },

  setCurrentSessionId: (sessionId: string | null) => {
    set({ currentSessionId: sessionId });
    if (sessionId) {
      SecureStore.setItemAsync('currentSessionId', sessionId).catch(e => console.warn('Failed to save session ID', e));
    } else {
      SecureStore.deleteItemAsync('currentSessionId').catch(e => console.warn('Failed to remove session ID', e));
      set({ messages: [] });
    }
  },

  listWorkspaces: async () => {
    const { currentBaseUrl, token } = get();
    if (!currentBaseUrl) return;
    try {
      const res = await fetch(`${currentBaseUrl}/api/v1/workspaces`, {
        headers: { 'Authorization': `Bearer ${token}` }
      });
      if (res.ok) {
        const workspaces = await res.json();
        set({ workspaces });
        if (!get().activeWorkspaceId && workspaces.length > 0) {
          set({ activeWorkspaceId: workspaces[0].id });
        }
      }
    } catch (e) {
      console.warn('[SSE] Failed to fetch workspaces:', e);
    }
  },

  setActiveWorkspaceId: (workspaceId: string | null) => {
    set({ activeWorkspaceId: workspaceId });
  },

  fetchWorkspaceFiles: async (workspaceId: string) => {
    const { currentBaseUrl, token } = get();
    if (!currentBaseUrl) return [];
    const res = await restGet(`${currentBaseUrl}/api/v1/workspaces/${workspaceId}/files`, token);
    return res || [];
  },

  clearError: () => set({ error: null }),
}));

// ── SSE event dispatcher ─────────────────────────────────────────────────────

function dispatchSseEvent(
  eventType: string,
  evt: SseEvent,
  set: (partial: Partial<SseState> | ((s: SseState) => Partial<SseState>)) => void,
  get: () => SseState,
): void {
  const { payload } = evt;
  const resolvedType = evt.event_type || eventType;

  switch (resolvedType) {
    case 'connected':
      // Server sends this immediately on connect to trigger XHR LOADING state.
      // This is our primary connection confirmation — do NOT rely solely on XHR 'open'.
      console.log('[SSE] Handshake received — connection confirmed:', payload.client_id);
      set({ isConnected: true, isConnecting: false, error: null });
      break;

    case 'result': {
      const isNewSession = get().currentSessionId !== payload.session_id;
      
      const newMsg: ChatMessage = {
        id: (evt.call_id ? evt.call_id + '_reply' : uid()),
        text: payload.content || '',
        sender: 'agent' as const,
        agent_used: payload.agent_used,
        timestamp: payload.timestamp || new Date().toISOString(),
        metadata: payload.metadata,
      };
      set((s) => {
        const updated = [...s.messages, newMsg];
        return {
          llmThought: null,
          currentSessionId: payload.session_id || s.currentSessionId,
          // Trim oldest messages to stay within MAX_MESSAGES
          messages: updated.length > MAX_MESSAGES ? updated.slice(-MAX_MESSAGES) : updated,
        };
      });

      if (isNewSession && payload.session_id) {
        get().getSessionHistory(payload.session_id);
      }
      break;
    }

    case 'progress':
      set({ llmThought: payload.thought || null });
      break;

    case 'session_list': {
      const mappedSessions = (payload.sessions || []).map((s: any) => ({
        id: s.id,
        title: s.title,
        lastActive: s.last_active_at || s.lastActive || new Date().toISOString(),
      }));
      set({ sessions: mappedSessions });
      break;
    }

    case 'session_history': {
      if (get().currentSessionId === payload.session_id) {
        const mappedMessages = (payload.messages || []).map((m: any) => ({
          id: m.id || uid(),
          text: m.content || '',
          sender: m.role === 'user' ? 'user' : 'agent',
          agent_used: m.agent_used,
          timestamp: m.timestamp_ms ? new Date(m.timestamp_ms).toISOString() : new Date().toISOString(),
        }));
        set({ messages: mappedMessages });
      }
      break;
    }

    case 'status': {
      set((s) => {
        const tasks = { ...s.activeTasks };
        const taskId = payload.workflow_name || payload.run_id;
        const statusLower: string = (payload.status || '').toLowerCase();
        if (['success', 'failed', 'aborted', 'completed'].includes(statusLower)) {
          delete tasks[taskId];
        } else {
          tasks[taskId] = {
            run_id: payload.run_id,
            workflow_id: payload.workflow_name,
            workflow_name: payload.workflow_name,
            step_name: payload.workflow_name,
            status: payload.status,
            message: payload.message,
            updated_at: payload.updated_at || new Date().toISOString(),
          };
        }
        return { activeTasks: tasks };
      });
      break;
    }

    case 'approval_request':
      set({
        activeHitlRequest: {
          action_id: payload.action_id,
          description: payload.description,
          risk_level: payload.risk_level,
          timeout_seconds: payload.timeout_seconds,
          payload: payload.payload,
        },
      });
      break;

    case 'error': {
      console.error('[SSE] Server error:', payload.message);
      const errMsg: ChatMessage = {
        id: uid(),
        text: `⚠️ Error: ${payload.message}`,
        sender: 'agent' as const,
        timestamp: new Date().toISOString(),
      };
      set((s) => {
        const updated = [...s.messages, errMsg];
        return {
          llmThought: null, // CLEAR THOUGHT ON ERROR
          messages: updated.length > MAX_MESSAGES ? updated.slice(-MAX_MESSAGES) : updated,
        };
      });
      break;
    }

    case 'log':
      break;

    default:
      break;
  }
}

// ── File upload helper ────────────────────────────────────────────────────────

export async function uploadFileToServer(
  baseUrl: string,
  token: string,
  file: { name: string; uri: string; type?: string },
): Promise<{ resource?: any; file_path?: string; error?: string }> {
  try {
    const formData = new FormData();
    formData.append('file', {
      uri: file.uri,
      name: file.name,
      type: file.type || 'application/octet-stream',
    } as any);

    const res = await fetch(`${baseUrl}/api/v1/files/upload`, {
      method: 'POST',
      headers: { 'Authorization': `Bearer ${token}` },
      body: formData,
    });

    return await res.json();
  } catch (e: any) {
    return { error: e.message };
  }
}
