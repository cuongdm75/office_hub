import React, { useEffect, useState, useMemo, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  ReactFlow,
  Controls,
  Background,
  BackgroundVariant,
  useNodesState,
  useEdgesState,
  MarkerType,
  Node,
  Edge,
  useReactFlow,
  ReactFlowProvider,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { AgentNode, ToolNode } from './CustomNodes';
import './AgentMonitor.css';
import { Activity, RefreshCw, Pause, Play } from 'lucide-react';

interface TelemetryLog {
  id: number;
  sessionId: string;
  agentName: string;
  action: string;
  latencyMs: number;
  tokensUsed: number;
  status: string;
  timestamp: string;
}

const nodeTypes = {
  agent: AgentNode,
  tool: ToolNode,
};

const POLLING_INTERVAL = 2000;
const ACTIVE_THRESHOLD_MS = 15000;

const SYSTEM_AGENTS = [
  { id: 'orchestrator', label: 'Orchestrator' },
  { id: 'analyst', label: 'Analyst' },
  { id: 'folder_scanner', label: 'Folder Scanner' },
  { id: 'office_master', label: 'Office Master' },
  { id: 'converter', label: 'Converter' },
  { id: 'outlook', label: 'Outlook' },
  { id: 'web_researcher', label: 'Web Researcher' },
];

// Inner component that has access to ReactFlow context
const MonitorFlow: React.FC<{
  nodes: Node[];
  edges: Edge[];
  onNodesChange: any;
  onEdgesChange: any;
}> = ({ nodes, edges, onNodesChange, onEdgesChange }) => {
  const { fitView } = useReactFlow();

  useEffect(() => {
    if (nodes.length > 0) {
      setTimeout(() => fitView({ padding: 0.15, duration: 400 }), 50);
    }
  }, []); // only on mount

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      nodeTypes={nodeTypes}
      fitView
      panOnDrag={true}
      nodesDraggable={true}
      zoomOnScroll={true}
      zoomOnPinch={true}
      zoomOnDoubleClick={true}
      minZoom={0.3}
      maxZoom={2}
      style={{ background: 'var(--flow-bg)' }}
      proOptions={{ hideAttribution: true }}
    >
      <Background
        variant={BackgroundVariant.Dots}
        color="var(--flow-dot)"
        gap={24}
        size={1}
      />
      <Controls
        style={{
          background: 'var(--bg-card)',
          border: '1px solid var(--border-default)',
          borderRadius: '10px',
          overflow: 'hidden',
        }}
      />
    </ReactFlow>
  );
};

const AgentMonitorInner: React.FC = () => {
  const [logs, setLogs] = useState<TelemetryLog[]>([]);
  const [loading, setLoading] = useState(true);
  const [isPaused, setIsPaused] = useState(false);

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  const fetchTelemetry = useCallback(async () => {
    if (isPaused) return;
    try {
      const data: TelemetryLog[] = await invoke('get_telemetry_logs', { limit: 100 });
      setLogs(data);
    } catch (error) {
      console.error('Failed to fetch telemetry:', error);
    } finally {
      setLoading(false);
    }
  }, [isPaused]);

  useEffect(() => {
    fetchTelemetry();
    const interval = setInterval(fetchTelemetry, POLLING_INTERVAL);
    return () => clearInterval(interval);
  }, [fetchTelemetry]);

  useEffect(() => {
    const now = new Date().getTime();

    const agentData: Record<string, {
      totalTokens: number;
      latestLog: TelemetryLog | null;
      isActive: boolean;
      executedTools: Set<string>;
    }> = {};

    SYSTEM_AGENTS.forEach(agent => {
      agentData[agent.id] = { totalTokens: 0, latestLog: null, isActive: false, executedTools: new Set() };
    });

    [...logs]
      .sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime())
      .forEach(log => {
        if (!agentData[log.agentName]) {
          agentData[log.agentName] = { totalTokens: 0, latestLog: null, isActive: false, executedTools: new Set() };
        }
        const data = agentData[log.agentName]!;
        data.totalTokens += log.tokensUsed;
        data.latestLog = log;
        if (log.action && log.action !== 'plan_and_route' && log.action !== 'final_response') {
          data.executedTools.add(log.action);
        }
        const logTime = new Date(log.timestamp).getTime();
        data.isActive = (now - logTime) < ACTIVE_THRESHOLD_MS;
      });

    const newNodes: Node[] = [];
    const newEdges: Edge[] = [];

    const CENTER_X = 500;
    const CENTER_Y = 300;
    const RADIUS = 260;

    const agentNames = Object.keys(agentData);
    const satelliteAgents = agentNames.filter(n => n !== 'orchestrator');
    const totalSatellites = satelliteAgents.length;

    Object.entries(agentData).forEach(([agentName, data]) => {
      let xPos = 0, yPos = 0;
      let label = agentName;

      const sysAgent = SYSTEM_AGENTS.find(a => a.id === agentName);
      if (sysAgent) label = sysAgent.label;

      if (agentName === 'orchestrator') {
        xPos = CENTER_X;
        yPos = CENTER_Y;
      } else {
        const index = satelliteAgents.indexOf(agentName);
        const angle = -Math.PI / 2 + (index * 2 * Math.PI) / Math.max(1, totalSatellites);
        xPos = CENTER_X + RADIUS * Math.cos(angle);
        yPos = CENTER_Y + RADIUS * Math.sin(angle);
      }

      const agentId = `agent-${agentName}`;
      newNodes.push({
        id: agentId,
        type: 'agent',
        position: { x: xPos - 110, y: yPos - 60 },
        data: {
          label,
          tokens: data.totalTokens,
          status: data.isActive ? 'working' : 'idle',
          task: data.latestLog ? data.latestLog.action : 'Waiting for tasks...',
          tools: Array.from(data.executedTools),
        },
      });

      if (agentName !== 'orchestrator') {
        const activeColor = '#6366f1';
        newEdges.push({
          id: `sys-edge-orchestrator-${agentName}`,
          source: 'agent-orchestrator',
          target: agentId,
          animated: data.isActive,
          style: {
            stroke: data.isActive ? activeColor : 'var(--flow-edge-idle)',
            strokeWidth: data.isActive ? 2 : 1,
            strokeDasharray: data.isActive ? 'none' : '4 4',
          },
          markerEnd: data.isActive
            ? { type: MarkerType.ArrowClosed, color: activeColor }
            : undefined,
        });
      }
    });

    setNodes(newNodes);
    setEdges(newEdges);
  }, [logs, setNodes, setEdges]);

  const totalTokens = useMemo(() => logs.reduce((acc, l) => acc + l.tokensUsed, 0), [logs]);
  const totalTasks = logs.length;
  const activeAgents = nodes.filter(n => n.type === 'agent' && n.data.status === 'working').length;

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center" style={{ color: 'var(--text-secondary)' }}>
        Initializing Monitor...
      </div>
    );
  }

  return (
    <div className="agent-monitor-layout">
      <header className="monitor-header">
        {/* Left: Title */}
        <div className="flex items-center gap-2.5 flex-shrink-0">
          <div className="p-1.5 rounded-lg" style={{ background: 'rgba(99,102,241,0.12)', color: '#6366f1' }}>
            <Activity size={18} />
          </div>
          <div>
            <h2 className="text-sm font-bold leading-tight" style={{ color: 'var(--flow-node-text)' }}>
              Visual Agent Monitor
            </h2>
            <div className="flex items-center gap-1.5 mt-0.5">
              <span className="live-dot" />
              <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
                Live multi-agent orchestration flow
              </p>
            </div>
          </div>
        </div>

        {/* Center: Stats */}
        <div className="monitor-stats">
          <div className="stat-card">
            <span className="stat-label">Active Agents</span>
            <span className="stat-value" style={{ color: '#6366f1' }}>{activeAgents}</span>
          </div>
          <div className="stat-card">
            <span className="stat-label">Total Tasks</span>
            <span className="stat-value">{totalTasks}</span>
          </div>
          <div className="stat-card">
            <span className="stat-label">Total Tokens</span>
            <span className="stat-value" style={{ color: '#f59e0b' }}>{totalTokens.toLocaleString()}</span>
          </div>
        </div>

        {/* Right: Toolbar */}
        <div className="monitor-toolbar">
          <button
            className={`monitor-btn ${!isPaused ? 'active' : ''}`}
            onClick={() => setIsPaused(p => !p)}
            title={isPaused ? 'Resume live updates' : 'Pause live updates'}
          >
            {isPaused ? <Play size={13} /> : <Pause size={13} />}
            {isPaused ? 'Paused' : 'Live'}
          </button>
          <button
            className="monitor-btn"
            onClick={() => fetchTelemetry()}
            title="Refresh now"
          >
            <RefreshCw size={13} />
            Refresh
          </button>
        </div>
      </header>

      <div className="flex-1 w-full relative" style={{ background: 'var(--flow-bg)' }}>
        <MonitorFlow
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
        />
      </div>
    </div>
  );
};

const AgentMonitor: React.FC = () => (
  <ReactFlowProvider>
    <AgentMonitorInner />
  </ReactFlowProvider>
);

export default AgentMonitor;
