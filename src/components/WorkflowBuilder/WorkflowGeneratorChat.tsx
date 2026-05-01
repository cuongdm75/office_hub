import React, { useState, useRef, useEffect } from 'react';
import { Bot, Send, Loader2, Sparkles, ChevronDown, ChevronUp } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import clsx from 'clsx';
import ReactMarkdown from 'react-markdown';

interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
}

interface WorkflowGeneratorChatProps {
  onWorkflowGenerated: (workflowDef: any) => void;
}

export default function WorkflowGeneratorChat({ onWorkflowGenerated }: WorkflowGeneratorChatProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [isMinimized, setIsMinimized] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isMinimized) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, isMinimized, isLoading]);

  const handleSend = async () => {
    if (!input.trim() || isLoading) return;

    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: input.trim(),
    };

    setMessages(prev => [...prev, userMessage]);
    setInput('');
    setIsLoading(true);

    try {
      // Create an orchestrator request to generate a workflow.
      // We instruct the LLM to return JSON containing the workflow definition.
      const prompt = `You are a Workflow Generation AI. Generate a workflow definition in JSON based on this request: "${userMessage.content}". 
The JSON must follow this exact structure:
{
  "id": "new-workflow",
  "name": "Generated Workflow",
  "description": "...",
  "trigger": { "type": "manual", "config": {} },
  "steps": [
    { "id": "step1", "name": "...", "agent": "web_researcher", "action": "navigate_to_url", "next_step": "step2" }
  ]
}
Return ONLY the raw JSON without markdown code blocks.`;

      // We bypass full orchestrator context and just make a raw LLM request
      // This prevents the Orchestrator's JSON schema from conflicting with the prompt
      const content: string = await invoke('raw_llm_request', { prompt });
      
      // Try to parse JSON from content
      let jsonStr = content;
      if (content.includes('```json')) {
        jsonStr = content.split('```json')[1]?.split('```')[0]?.trim() || content;
      } else if (content.includes('```')) {
        jsonStr = content.split('```')[1]?.split('```')[0]?.trim() || content;
      }

      const workflowDef = JSON.parse(jsonStr);

      const assistantMessage: ChatMessage = {
        id: crypto.randomUUID(),
        role: 'assistant',
        content: `I have generated the workflow "${workflowDef.name}". It is now loaded onto the canvas.`,
      };
      
      setMessages(prev => [...prev, assistantMessage]);
      onWorkflowGenerated(workflowDef);

    } catch (e) {
      console.error('Workflow generation failed', e);
      setMessages(prev => [...prev, {
        id: crypto.randomUUID(),
        role: 'system',
        content: `Error generating workflow: ${e instanceof Error ? e.message : String(e)}`,
      }]);
    } finally {
      setIsLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className={clsx(
      "absolute bottom-6 right-6 w-80 bg-[var(--bg-card)] rounded-2xl shadow-2xl border border-[var(--border-default)] flex flex-col overflow-hidden transition-all duration-300 z-50",
      isMinimized ? "h-14" : "h-[450px]"
    )}>
      {/* Header */}
      <div 
        className="px-4 py-3 bg-gradient-to-r from-blue-600 to-indigo-600 text-white flex items-center justify-between cursor-pointer select-none shrink-0"
        onClick={() => setIsMinimized(!isMinimized)}
      >
        <div className="flex items-center space-x-2">
          <Sparkles size={18} />
          <span className="font-medium text-sm">AI Flow Generator</span>
        </div>
        <button className="text-white/80 hover:text-white transition-colors">
          {isMinimized ? <ChevronUp size={18} /> : <ChevronDown size={18} />}
        </button>
      </div>

      {/* Body */}
      {!isMinimized && (
        <>
          <div className="flex-1 overflow-y-auto p-4 space-y-4 bg-[var(--bg-main)]">
            {messages.length === 0 ? (
              <div className="text-center text-[var(--text-secondary)] text-sm mt-4">
                <Bot size={32} className="mx-auto mb-2 opacity-50" />
                <p>Describe what you want to automate, and I will generate the workflow for you.</p>
                <div className="mt-4 space-y-2">
                  <button onClick={() => setInput('Tạo quy trình trích xuất giá vàng và lưu vào báo cáo')} className="w-full text-left px-3 py-2 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded text-xs hover:border-[var(--accent)] transition-colors">
                    "Tạo quy trình trích xuất giá vàng và lưu vào báo cáo"
                  </button>
                </div>
              </div>
            ) : (
              messages.map(msg => (
                <div key={msg.id} className={clsx("flex flex-col max-w-[85%]", msg.role === 'user' ? "ml-auto items-end" : "mr-auto items-start")}>
                  <div className={clsx(
                    "px-3 py-2 rounded-xl text-sm shadow-sm",
                    msg.role === 'user' ? "bg-[var(--accent)] text-white rounded-tr-none" :
                    msg.role === 'system' ? "bg-red-500/10 text-red-500 border border-red-500/30" :
                    "bg-[var(--bg-input)] text-[var(--text-primary)] rounded-tl-none border border-[var(--border-default)]"
                  )}>
                    {msg.role === 'assistant' ? (
                      <div className="prose prose-sm dark:prose-invert">
                        <ReactMarkdown>{msg.content}</ReactMarkdown>
                      </div>
                    ) : (
                      <p className="whitespace-pre-wrap">{msg.content}</p>
                    )}
                  </div>
                </div>
              ))
            )}
            {isLoading && (
              <div className="flex justify-start">
                <div className="px-3 py-2 bg-[var(--bg-input)] rounded-xl rounded-tl-none border border-[var(--border-default)] shadow-sm flex items-center space-x-2">
                  <Loader2 size={14} className="animate-spin text-[var(--accent)]" />
                  <span className="text-xs text-[var(--text-secondary)]">Generating flow...</span>
                </div>
              </div>
            )}
            <div ref={messagesEndRef} />
          </div>

          {/* Input */}
          <div className="p-3 bg-[var(--bg-card)] border-t border-[var(--border-default)] shrink-0">
            <div className="relative flex items-center">
              <textarea
                value={input}
                onChange={e => setInput(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder="Ask AI to build a workflow..."
                className="w-full bg-[var(--bg-input)] border border-[var(--border-default)] rounded-lg py-2 pl-3 pr-10 text-sm focus:ring-1 focus:ring-[var(--accent)] resize-none h-[40px] leading-tight text-[var(--text-primary)] placeholder-[var(--text-muted)]"
                rows={1}
              />
              <button
                onClick={handleSend}
                disabled={!input.trim() || isLoading}
                className="absolute right-1 p-1.5 text-[var(--accent)] hover:bg-[var(--bg-hover)] rounded-md disabled:opacity-50 transition-colors"
              >
                <Send size={16} />
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
