import { useState, useEffect, useRef, useCallback } from 'react'
import './App.css'

/* global Office, Word, Excel */

type HostType = 'Word' | 'Excel' | 'PowerPoint' | 'Outlook' | 'Unknown'
type Message = { role: 'user' | 'assistant' | 'status'; content: string }

const WS_URL = 'ws://127.0.0.1:9001'
const AUTH_TOKEN = import.meta.env.VITE_AUTH_TOKEN || '87ecb66c080a4de29eb20555c397181f'
const MAX_RECONNECT_DELAY_MS = 30_000

interface WorkflowStatus {
  run_id: string;
  workflow_id: string;
  workflow_name?: string;
  step_name?: string;
  status: string;
  message?: string;
  updated_at: string;
}

/** Detect which Office host we're running in – safe (Office may not be ready yet) */
function detectHost(): HostType {
  try {
    const host = Office?.context?.host
    if (!host) return 'Unknown'
    if (host === Office.HostType.Word) return 'Word'
    if (host === Office.HostType.Excel) return 'Excel'
    if (host === Office.HostType.PowerPoint) return 'PowerPoint'
    if (host === Office.HostType.Outlook) return 'Outlook'
    return 'Unknown'
  } catch {
    return 'Unknown'
  }
}

/** Get file/item context info */
function getContextInfo(host: HostType): string {
  try {
    if (host === 'Outlook') {
      const item = Office.context.mailbox?.item
      if (!item) return 'Outlook (no item selected)'
      const subject = (item as Office.MessageRead).subject ?? '(no subject)'
      return `Outlook | Email: "${subject}"`
    }
    const url = Office.context.document?.url
    return url ? `${host} | ${url.split('\\').pop() ?? url}` : `${host} | Unsaved Document`
  } catch {
    return host
  }
}

function App() {
  const [messages, setMessages] = useState<Message[]>([])
  const [input, setInput] = useState('')
  const [status, setStatus] = useState<'connecting' | 'connected' | 'reconnecting' | 'error'>('connecting')
  const [host, setHost] = useState<HostType>('Unknown')
  const [contextInfo, setContextInfo] = useState('')
  const [emailContext, setEmailContext] = useState<string>('')
  const [activeTasks, setActiveTasks] = useState<Record<string, WorkflowStatus>>({})
  const wsRef = useRef<WebSocket | null>(null)
  const messagesEndRef = useRef<HTMLDivElement>(null)
  const reconnectDelayRef = useRef<number>(1000) // start with 1s
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const unmountedRef = useRef(false)

  // Auto-scroll to bottom
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages])

  // ── Auto-reconnect logic ──────────────────────────────────────────────────
  const connectWs = useCallback((currentHost: HostType) => {
    if (unmountedRef.current) return

    setStatus('connecting')
    const ws = new WebSocket(WS_URL)
    wsRef.current = ws

    ws.onopen = () => {
      if (unmountedRef.current) { ws.close(); return }
      setStatus('connected')
      reconnectDelayRef.current = 1000 // reset backoff on successful connect

      // 1. Auth
      ws.send(JSON.stringify({ type: 'auth', token: AUTH_TOKEN }))

      // 2. Notify backend of context
      const payload: Record<string, unknown> = {
        type: 'office_addin_event',
        event: 'DocumentOpened',
        app_type: currentHost,
        file_path: '',
      }
      if (currentHost === 'Outlook') {
        try {
          const item = Office.context.mailbox?.item as Office.MessageRead
          payload.subject = item?.subject ?? ''
          payload.sender = item?.from?.emailAddress ?? ''
        } catch { /* ignore */ }
      } else {
        try { payload.file_path = Office.context.document?.url ?? '' } catch { /* ignore */ }
      }
      ws.send(JSON.stringify(payload))

      // 3. Selection listeners
      if (currentHost === 'Word') {
        try {
          Office.context.document.addHandlerAsync(Office.EventType.DocumentSelectionChanged, () => {
            Word.run(async (ctx) => {
              const range = ctx.document.getSelection()
              range.load('text')
              await ctx.sync()
              if (range.text) {
                setContextInfo(`Word | Selection: "${range.text.slice(0, 30)}..."`)
                if (wsRef.current?.readyState === WebSocket.OPEN)
                  wsRef.current.send(JSON.stringify({ type: 'office_addin_event', event: 'SelectionChanged', content: range.text, app_type: 'Word' }))
              }
            }).catch(console.error)
          })
        } catch (e) { console.warn('Could not add Word selection handler:', e) }
      } else if (currentHost === 'Excel') {
        Excel.run(async (ctx) => {
          ctx.workbook.onSelectionChanged.add(() => {
            Excel.run(async (c) => {
              const range = c.workbook.getSelectedRange()
              range.load('address')
              await c.sync()
              setContextInfo(`Excel | Selected Cell: ${range.address}`)
              if (wsRef.current?.readyState === WebSocket.OPEN)
                wsRef.current.send(JSON.stringify({ type: 'office_addin_event', event: 'SelectionChanged', content: range.address, app_type: 'Excel' }))
            }).catch(console.error)
            return null as any
          })
          await ctx.sync()
        }).catch(console.error)
      }
    }

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        if (data.type === 'auth_success') {
          // auth acknowledged – nothing to do
        } else if (data.type === 'chat_response' || data.type === 'response') {
          setMessages(prev => [...prev, { role: 'assistant', content: data.content ?? data.text ?? '' }])
        } else if (data.type === 'chat_reply') {
          setMessages(prev => [...prev, { role: 'assistant', content: data.content ?? '' }])
        } else if (data.type === 'context_analysis') {
          setMessages(prev => [...prev, { role: 'assistant', content: `📄 ${data.summary}` }])
        } else if (data.type === 'workflow_status') {
          setActiveTasks(prev => {
            const newTasks = { ...prev }
            const taskId = data.step_name || data.workflow_name || data.run_id
            if (['success', 'failed', 'aborted', 'completed'].includes(data.status.toLowerCase())) {
              delete newTasks[taskId]
            } else {
              newTasks[taskId] = data
            }
            return newTasks
          })
        } else if (data.type === 'addin_command') {
          if (data.command === 'insert_text') insertIntoDocument(data.payload || '')
          else if (data.command === 'replace_document') replaceDocumentContent(data.payload || '')
          else if (data.command === 'save_document')
            (Office.context.document as any).saveAsync?.().catch(console.error)
          else if (data.command === 'extract_file' && wsRef.current)
            extractFileAndSend(wsRef.current, data.payload || 'extracted_file.docx')
        }
      } catch (e) {
        console.error('WS parse error:', e)
      }
    }

    const scheduleReconnect = () => {
      if (unmountedRef.current) return
      setStatus('reconnecting')
      const delay = reconnectDelayRef.current
      reconnectDelayRef.current = Math.min(delay * 2, MAX_RECONNECT_DELAY_MS)
      console.warn(`WebSocket disconnected – reconnecting in ${delay}ms`)
      reconnectTimerRef.current = setTimeout(() => connectWs(currentHost), delay)
    }

    ws.onerror = () => { /* onclose will fire next */ }
    ws.onclose = scheduleReconnect
  }, [])

  // ── Initialise once on mount ──────────────────────────────────────────────
  useEffect(() => {
    unmountedRef.current = false

    const detectedHost = detectHost()
    setHost(detectedHost)
    setContextInfo(getContextInfo(detectedHost))

    // For Outlook, extract email body
    if (detectedHost === 'Outlook') {
      try {
        const item = Office.context.mailbox?.item as Office.MessageRead
        if (item?.body) {
          item.body.getAsync(Office.CoercionType.Text, (result) => {
            if (result.status === Office.AsyncResultStatus.Succeeded)
              setEmailContext(result.value.slice(0, 500))
          })
        }
      } catch (e) {
        console.warn('Could not read email body:', e)
      }
    }

    connectWs(detectedHost)

    return () => {
      unmountedRef.current = true
      if (reconnectTimerRef.current) clearTimeout(reconnectTimerRef.current)
      wsRef.current?.close()
    }
  }, [connectWs])

  /** Insert AI response into the active document */
  const insertIntoDocument = async (text: string) => {
    try {
      if (host === 'Word') {
        await Word.run(async (context) => {
          const range = context.document.getSelection()
          range.insertText(text, Word.InsertLocation.after)
          await context.sync()
        })
      } else if (host === 'Outlook') {
        // Insert into compose window body
        const item = Office.context.mailbox?.item as Office.MessageCompose
        item?.body?.setAsync(text, { coercionType: Office.CoercionType.Text })
      } else {
        // Excel / PowerPoint fallback
        Office.context.document.setSelectedDataAsync(text, { coercionType: Office.CoercionType.Text })
      }
    } catch (e) {
      console.error('Insert failed:', e)
    }
  }

  const replaceDocumentContent = async (text: string) => {
    try {
      if (host === 'Word') {
        await Word.run(async (context) => {
          context.document.body.clear();
          context.document.body.insertText(text, Word.InsertLocation.end);
          await context.sync();
        });
      } else {
        console.warn('Replace document fully supported only in Word for now');
      }
    } catch (e) {
      console.error('Replace failed:', e);
    }
  }

  const extractFileAndSend = (ws: WebSocket, fileName: string) => {
    Office.context.document.getFileAsync(Office.FileType.Compressed, { sliceSize: 65536 }, (result) => {
      if (result.status === Office.AsyncResultStatus.Succeeded) {
        const file = result.value;
        const slices: any[] = [];
        const sliceCount = file.sliceCount;
        let slicesReceived = 0;

        const getSlice = (sliceIndex: number) => {
          file.getSliceAsync(sliceIndex, (sliceResult) => {
            if (sliceResult.status === Office.AsyncResultStatus.Succeeded) {
              slices[sliceIndex] = sliceResult.value.data;
              slicesReceived++;
              if (slicesReceived === sliceCount) {
                file.closeAsync();
                let totalLength = 0;
                for (let i = 0; i < slices.length; i++) {
                  totalLength += slices[i].byteLength;
                }
                const combined = new Uint8Array(totalLength);
                let offset = 0;
                for (let i = 0; i < slices.length; i++) {
                  combined.set(new Uint8Array(slices[i]), offset);
                  offset += slices[i].byteLength;
                }
                let binary = '';
                for (let i = 0; i < combined.byteLength; i++) {
                  binary += String.fromCharCode(combined[i]);
                }
                const base64Data = window.btoa(binary);

                ws.send(JSON.stringify({
                  type: 'document_extracted',
                  file_name: fileName,
                  base64_data: base64Data
                }));
              } else {
                getSlice(sliceIndex + 1);
              }
            } else {
              file.closeAsync();
              console.error("Error getting slice: ", sliceResult.error.message);
            }
          });
        };
        getSlice(0);
      } else {
        console.error("Error getting file: ", result.error.message);
      }
    });
  }

  const extractDocumentContent = async (hostType: string): Promise<string> => {
    let extractedText = '';
    try {
      if (hostType === 'Word') {
        await Word.run(async (context) => {
          // Priority 1: Try to get the selected text
          const selection = context.document.getSelection();
          selection.load('text');
          await context.sync();
          
          if (selection.text && selection.text.trim().length > 0) {
            extractedText = selection.text;
          } else {
            // Priority 2: Fallback to document body but limit to 5000 chars
            const body = context.document.body;
            body.load('text');
            await context.sync();
            extractedText = body.text.substring(0, 5000);
          }
        });
      } else if (hostType === 'Excel') {
        await Excel.run(async (context) => {
          // Priority 1: Try to get the active selection
          const range = context.workbook.getSelectedRange();
          range.load('values');
          await context.sync();
          
          let hasValues = false;
          if (range.values && range.values.length > 0) {
             // Check if it's more than just an empty cell
             hasValues = range.values.some(row => row.some(cell => cell !== '' && cell !== null));
          }

          if (hasValues) {
            extractedText = JSON.stringify(range.values);
          } else {
            // Priority 2: Fallback to used range if no specific selection
            const usedRange = context.workbook.worksheets.getActiveWorksheet().getUsedRange();
            usedRange.load('values');
            await context.sync();
            extractedText = JSON.stringify(usedRange.values).substring(0, 5000);
          }
        });
      } else if (hostType === 'PowerPoint') {
        // Fallback for PowerPoint: just get selected text for now
        return new Promise((resolve) => {
          Office.context.document.getSelectedDataAsync(Office.CoercionType.Text, (result) => {
            if (result.status === Office.AsyncResultStatus.Succeeded) {
              resolve(result.value as string);
            } else {
              resolve('Không thể đọc nội dung PowerPoint. Vui lòng bôi đen (chọn) đoạn văn bản cần xử lý trước khi bấm nút.');
            }
          });
        });
      }
    } catch (e) {
      console.error('Extraction error:', e);
    }
    // Limit maximum token payload size to keep LLM snappy
    return extractedText.substring(0, 50000);
  };

  const handleSend = async () => {
    const text = input.trim()
    if (!text) return
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
      setMessages(prev => [...prev, { role: 'status', content: '⚠️ Chưa kết nối backend. Đang thử kết nối lại...' }])
      return
    }

    setMessages(prev => [...prev, { role: 'user', content: text }])
    setActiveTasks({})

    const payload: Record<string, unknown> = {
      type: 'chat_request',
      content: text,
      app_type: host,
    }

    const url = Office.context.document?.url ?? ''
    payload.file_context = url

    if (host === 'Outlook' && emailContext) {
      payload.email_context = emailContext
    } else {
      // Always extract document content for all Office apps (local + online)
      const docText = await extractDocumentContent(host);
      if (docText && docText.trim().length > 0) {
        payload.document_content = docText;
        // Mask SharePoint/OneDrive URLs to prevent unauthorized external fetches
        if (url.startsWith('http') || url.startsWith('https')) {
          payload.file_context = 'Local_Document.docx';
        }
      }
    }

    wsRef.current.send(JSON.stringify(payload))
    setInput('')
  }

  const handleAutoFormat = async () => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
      setMessages(prev => [...prev, { role: 'status', content: '⚠️ Chưa kết nối backend.' }])
      return
    }
    setMessages(prev => [...prev, { role: 'user', content: 'Tư vấn trình bày / Format lại tài liệu này.' }]);
    
    const content = await extractDocumentContent(host);

    wsRef.current.send(JSON.stringify({
      type: 'chat_request',
      content: 'Tư vấn trình bày / Format lại tài liệu này.',
      document_content: content,
      app_type: host,
      file_context: 'Local_Document.docx'
    }));
  };

  const handleCreateForm = async () => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
      setMessages(prev => [...prev, { role: 'status', content: '⚠️ Chưa kết nối backend.' }])
      return
    }
    setMessages(prev => [...prev, { role: 'user', content: 'Tạo Form từ tài liệu này.' }]);
    
    const content = await extractDocumentContent(host);

    wsRef.current.send(JSON.stringify({
      type: 'chat_request',
      content: 'Tạo Form từ tài liệu này.',
      document_content: content,
      app_type: host,
      file_context: 'Local_Document.docx'
    }));
  };

  const statusText =
    status === 'connected' ? '● Connected' :
    status === 'connecting' ? '◌ Connecting...' :
    status === 'reconnecting' ? '↺ Reconnecting...' :
    '✕ Disconnected'

  const hostIcon: Record<HostType, string> = {
    Word: '📝', Excel: '📊', PowerPoint: '📊', Outlook: '✉️', Unknown: '🤖'
  }

  return (
    <div style={{
      height: '100vh', display: 'flex', flexDirection: 'column',
      fontFamily: '"Segoe UI", system-ui, sans-serif', fontSize: '13px',
      background: '#fafafa', color: '#1b1b1b'
    }}>
      {/* Header */}
      <div style={{
        padding: '10px 14px', background: '#0078d4', color: 'white',
        display: 'flex', alignItems: 'center', gap: '8px', flexShrink: 0
      }}>
        <span style={{ fontSize: '18px' }}>{hostIcon[host]}</span>
        <div style={{ flex: 1 }}>
          <div style={{ fontWeight: 600, fontSize: '14px' }}>Office Hub AI</div>
          <div style={{ fontSize: '11px', opacity: 0.85, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
            {contextInfo}
          </div>
        </div>
        <span style={{ fontSize: '11px', background: 'rgba(255,255,255,0.2)', padding: '2px 8px', borderRadius: '99px' }}>
          {statusText}
        </span>
      </div>

      {/* Email context banner (Outlook only) */}
      {host === 'Outlook' && emailContext && (
        <div style={{
          padding: '8px 12px', background: '#fffbe6', borderBottom: '1px solid #ffd700',
          fontSize: '11px', color: '#5a4500'
        }}>
          <strong>Email Context (preview):</strong> {emailContext.slice(0, 120)}…
        </div>
      )}

      {/* Messages */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '12px', display: 'flex', flexDirection: 'column', gap: '10px' }}>
        {messages.length === 0 && (
          <div style={{ textAlign: 'center', color: '#888', marginTop: '40px', lineHeight: 1.8 }}>
            <div style={{ fontSize: '28px', marginBottom: '8px' }}>{hostIcon[host]}</div>
            <div><strong>Office Hub AI</strong> sẵn sàng</div>
            <div style={{ fontSize: '12px' }}>Hỏi AI để chỉnh sửa tài liệu, soạn email, phân tích dữ liệu...</div>
          </div>
        )}

        {messages.map((msg, i) => (
          <div key={i} style={{
            display: 'flex', flexDirection: 'column',
            alignSelf: msg.role === 'user' ? 'flex-end' : 'flex-start',
            maxWidth: '88%'
          }}>
            <div style={{
              padding: '8px 12px', borderRadius: msg.role === 'user' ? '16px 16px 4px 16px' : '16px 16px 16px 4px',
              background: msg.role === 'user' ? '#0078d4' : 'white',
              color: msg.role === 'user' ? 'white' : '#1b1b1b',
              boxShadow: '0 1px 3px rgba(0,0,0,0.1)',
              whiteSpace: 'pre-wrap', lineHeight: 1.5
            }}>
              {msg.content}
            </div>
            {msg.role === 'assistant' && (
              <button
                onClick={() => insertIntoDocument(msg.content)}
                style={{
                  marginTop: '4px', alignSelf: 'flex-start',
                  background: 'none', border: '1px solid #0078d4',
                  color: '#0078d4', borderRadius: '4px', padding: '2px 10px',
                  cursor: 'pointer', fontSize: '11px',
                  transition: 'all 0.15s'
                }}
                onMouseOver={e => { (e.target as HTMLElement).style.background = '#0078d4'; (e.target as HTMLElement).style.color = 'white' }}
                onMouseOut={e => { (e.target as HTMLElement).style.background = 'none'; (e.target as HTMLElement).style.color = '#0078d4' }}
              >
                {host === 'Outlook' ? '✉ Insert into Reply' : '📄 Insert to Document'}
              </button>
            )}
          </div>
        ))}
        {Object.values(activeTasks).length > 0 && (
          <div style={{
            alignSelf: 'flex-start',
            maxWidth: '88%',
            display: 'flex', flexDirection: 'column', gap: '4px',
            padding: '8px 12px', borderRadius: '16px 16px 16px 4px',
            background: '#f1f5f9', color: '#475569',
            fontSize: '12px'
          }}>
            {Object.values(activeTasks).map(task => (
              <div key={task.run_id + task.step_name} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                <span className="spinner">⏳</span>
                <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                  {task.message || `Running ${task.step_name || task.workflow_name}...`}
                </span>
              </div>
            ))}
          </div>
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <div style={{
        padding: '10px 12px', borderTop: '1px solid #e0e0e0',
        display: 'flex', gap: '8px', background: 'white', flexShrink: 0
      }}>
        <textarea
          style={{
            flex: 1, padding: '8px 10px', borderRadius: '6px',
            border: '1px solid #ccc', resize: 'none', height: '60px',
            fontFamily: 'inherit', fontSize: '13px', lineHeight: 1.4,
            outline: 'none'
          }}
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend() } }}
          placeholder={
            host === 'Outlook'
              ? 'Soạn email, tóm tắt nội dung...'
              : 'Hỏi AI về tài liệu này...'
          }
        />
        <button
          onClick={handleSend}
          disabled={status !== 'connected'}
          style={{
            background: status === 'connected' ? '#0078d4' : status === 'reconnecting' ? '#f59e0b' : '#ccc',
            color: 'white', border: 'none', borderRadius: '6px',
            padding: '0 16px', cursor: status === 'connected' ? 'pointer' : 'not-allowed',
            fontWeight: 600, fontSize: '13px', transition: 'background 0.15s'
          }}
        >
          {status === 'reconnecting' ? '↺' : 'Gửi'}
        </button>
      </div>
      
      {/* Quick Actions */}
      {status === 'connected' && (
        <div style={{ padding: '8px 12px', display: 'flex', gap: '8px', background: '#f8fafc', borderTop: '1px solid #e2e8f0' }}>
          <button
            onClick={handleAutoFormat}
            style={{
              flex: 1, padding: '6px', fontSize: '11px', background: '#fff', border: '1px solid #cbd5e1', 
              borderRadius: '4px', cursor: 'pointer', color: '#334155'
            }}
          >
            ✨ Tư vấn Format
          </button>
          <button
            onClick={handleCreateForm}
            style={{
              flex: 1, padding: '6px', fontSize: '11px', background: '#fff', border: '1px solid #cbd5e1', 
              borderRadius: '4px', cursor: 'pointer', color: '#334155'
            }}
          >
            📋 Tạo Form
          </button>
        </div>
      )}
    </div>
  )
}

export default App
