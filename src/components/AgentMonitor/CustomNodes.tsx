import { Handle, Position } from '@xyflow/react';
import { Bot, Wrench, Zap, Activity } from 'lucide-react';
import clsx from 'clsx';
import './AgentMonitor.css';

interface AgentNodeData {
  label: string;
  tokens: number;
  status: 'idle' | 'working' | 'error';
  task?: string;
  latency?: number;
  tools?: string[];
}

export const AgentNode = ({ data, isConnectable }: { data: AgentNodeData; isConnectable: boolean }) => {
  const isWorking = data.status === 'working';
  const isError = data.status === 'error';

  return (
    <div className={clsx(
      "agent-node-container",
      isWorking && "is-working",
      isError && "is-error"
    )}>
      <Handle
        type="target"
        position={Position.Top}
        isConnectable={isConnectable}
        style={{ background: 'var(--flow-handle)', border: '2px solid var(--flow-node-bg)' }}
      />

      {/* Header */}
      <div className="flex items-center justify-between pb-2 mb-2" style={{ borderBottom: '1px solid var(--flow-node-border)' }}>
        <div className="flex items-center gap-2">
          <div
            className="p-1.5 rounded-lg"
            style={{
              background: isWorking ? 'rgba(99,102,241,0.15)' : 'var(--bg-hover)',
              color: isWorking ? '#6366f1' : 'var(--node-muted)',
            }}
          >
            <Bot size={16} />
          </div>
          <span className="node-label">{data.label}</span>
        </div>
        <div
          className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full"
          style={{ background: 'var(--bg-app)', border: '1px solid var(--border-default)' }}
        >
          <Zap size={11} style={{ color: '#f59e0b' }} />
          <span style={{ color: '#f59e0b', fontWeight: 600, fontSize: '0.7rem' }}>
            {data.tokens?.toLocaleString() || 0}
          </span>
        </div>
      </div>

      {/* Status */}
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center justify-between text-xs">
          <span className="node-muted flex items-center gap-1">
            <Activity size={11} /> Status
          </span>
          <span
            className={clsx("font-semibold capitalize text-xs", isWorking && "animate-pulse")}
            style={{
              color: isWorking ? '#6366f1' : isError ? '#ef4444' : 'var(--text-muted)',
            }}
          >
            {data.status}
          </span>
        </div>

        {/* Current Task */}
        {data.task && (
          <div className="node-inner">
            <span className="node-sub block mb-1">Current Task</span>
            <span
              className="text-xs line-clamp-2"
              style={{ color: 'var(--flow-node-muted)' }}
              title={data.task}
            >
              {data.task}
            </span>
          </div>
        )}

        {/* Executed Tools */}
        {data.tools && data.tools.length > 0 && (
          <div className="mt-1 pt-2" style={{ borderTop: '1px solid var(--flow-node-border)' }}>
            <span className="node-sub block mb-1.5">Executed Tools</span>
            <div className="flex flex-wrap gap-1">
              {data.tools.map((tool, idx) => (
                <span
                  key={idx}
                  className="px-1.5 py-0.5 rounded text-[10px] truncate max-w-[120px]"
                  style={{
                    background: 'var(--bg-app)',
                    border: '1px solid var(--border-default)',
                    color: 'var(--flow-node-muted)',
                  }}
                  title={tool}
                >
                  {tool}
                </span>
              ))}
            </div>
          </div>
        )}
      </div>

      <Handle
        type="source"
        position={Position.Bottom}
        isConnectable={isConnectable}
        style={{ background: isWorking ? '#6366f1' : 'var(--flow-handle)', border: '2px solid var(--flow-node-bg)' }}
      />
    </div>
  );
};

interface ToolNodeData {
  label: string;
  status: 'idle' | 'running' | 'completed' | 'error';
  latency?: number;
}

export const ToolNode = ({ data, isConnectable }: { data: ToolNodeData; isConnectable: boolean }) => {
  const isRunning = data.status === 'running';

  return (
    <div className={clsx("tool-node-container", isRunning && "is-running")}>
      <Handle
        type="target"
        position={Position.Top}
        isConnectable={isConnectable}
        style={{ background: 'var(--flow-handle)', border: '2px solid var(--flow-node-bg)' }}
      />

      <div className="flex items-center gap-2">
        <div
          className="p-1.5 rounded-lg"
          style={{
            background: isRunning ? 'rgba(16,185,129,0.15)' : 'var(--bg-hover)',
            color: isRunning ? '#10b981' : 'var(--text-muted)',
          }}
        >
          <Wrench size={14} />
        </div>
        <div className="flex flex-col">
          <span className="node-label" style={{ fontSize: '0.8rem' }}>{data.label}</span>
          <span className="node-muted" style={{ fontSize: '0.65rem' }}>
            {isRunning ? 'Executing...' : 'Ready'}
            {data.latency ? ` (${data.latency}ms)` : ''}
          </span>
        </div>
      </div>

      <Handle
        type="source"
        position={Position.Bottom}
        isConnectable={isConnectable}
        style={{ background: 'var(--flow-handle)', border: '2px solid var(--flow-node-bg)' }}
      />
    </div>
  );
};
