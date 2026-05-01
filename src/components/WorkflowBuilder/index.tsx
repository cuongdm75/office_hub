import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  ReactFlow,
  ReactFlowProvider,
  MiniMap,
  Controls,
  Background,
  useNodesState,
  useEdgesState,
  useReactFlow,
  Node,
  Edge,
  MarkerType,
  Handle,
  Position,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { Play, RotateCcw, AlertCircle, CheckCircle2, Clock, Loader2, Save } from 'lucide-react';
import clsx from 'clsx';
import Sidebar, { draggedNodePayload } from './Sidebar';
import WorkflowGeneratorChat from './WorkflowGeneratorChat';

let id = 0;
const getId = () => `dndnode_${id++}`;

// Type definitions
interface WorkflowDefinition {
  id: string;
  name: string;
  description: string;
  steps: WorkflowStep[];
}

interface WorkflowStep {
  id: string;
  name: string;
  agent: string;
  action: string;
  next_step?: string;
  condition?: string;
}

interface WorkflowProgressUpdate {
  type: 'run' | 'step';
  run_id: string;
  workflow_id: string;
  step_id?: string;
  step_name?: string;
  status: string; // 'pending' | 'running' | 'success' | 'failed' | 'aborted'
  message?: string;
  updated_at: string;
}

// Custom Node Component
function StepNode({ data }: any) {
  const statusColors: Record<string, string> = {
    pending: 'bg-slate-100 border-slate-300 text-slate-500',
    running: 'bg-blue-50 border-blue-400 text-blue-600',
    success: 'bg-green-50 border-green-400 text-green-600',
    failed: 'bg-red-50 border-red-400 text-red-600',
    aborted: 'bg-orange-50 border-orange-400 text-orange-600',
  };

  const statusIcons: Record<string, React.ReactNode> = {
    pending: <Clock size={16} />,
    running: <Loader2 size={16} className="animate-spin" />,
    success: <CheckCircle2 size={16} />,
    failed: <AlertCircle size={16} />,
    aborted: <AlertCircle size={16} />,
  };

  const status = data.status || 'pending';

  return (
    <div className={clsx("px-4 py-3 shadow-md rounded-xl border-2 min-w-[200px] transition-colors relative", statusColors[status])}>
      <Handle type="target" position={Position.Top} className="w-2 h-2" />
      <div className="flex items-center justify-between mb-2">
        <div className="font-bold text-sm truncate">{data.label}</div>
        <div>{statusIcons[status]}</div>
      </div>
      <div className="text-xs text-slate-500 flex flex-col gap-1">
        <span className="font-mono bg-white/50 px-1 py-0.5 rounded">Agent: {data.agent}</span>
        <span className="font-mono bg-white/50 px-1 py-0.5 rounded truncate">Action: {data.action}</span>
      </div>
      {data.message && (
        <div className="mt-2 text-xs italic text-slate-600 bg-white/50 px-2 py-1 rounded truncate">
          {data.message}
        </div>
      )}
      <Handle type="source" position={Position.Bottom} className="w-2 h-2" />
    </div>
  );
}

const nodeTypes = {
  step: StepNode,
};

function NodePropertiesPanel({ node, updateNodeData, onDeleteNode, agents }: { node: Node, updateNodeData: (id: string, data: any) => void, onDeleteNode?: (id: string) => void, agents: any[] }) {
  if (!node) return null;

  const selectedAgent = agents.find(a => a.id === node.data.agent);
  const actions = selectedAgent?.capabilities || [];

  return (
    <div className="w-80 border-l border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-950 p-4 flex flex-col overflow-y-auto z-10 shadow-xl">
      <h3 className="font-semibold text-slate-800 dark:text-slate-200 mb-4">Node Properties</h3>
      
      <div className="space-y-4">
        <div>
          <label className="block text-xs font-medium text-slate-500 mb-1">Label / Name</label>
          <input 
            type="text" 
            value={node.data.label as string || ''} 
            onChange={(e) => updateNodeData(node.id, { label: e.target.value })}
            className="w-full px-3 py-2 bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-lg text-sm"
          />
        </div>
        
        <div>
          <label className="block text-xs font-medium text-slate-500 mb-1">Agent</label>
          <select 
            value={node.data.agent as string || 'unassigned'} 
            onChange={(e) => {
              const newAgent = e.target.value;
              const newAgentObj = agents.find((a: any) => a.id === newAgent);
              const firstAction = newAgentObj?.capabilities?.[0] || '';
              updateNodeData(node.id, { agent: newAgent, action: firstAction });
            }}
            className="w-full px-3 py-2 bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-lg text-sm font-mono"
          >
            <option value="unassigned">unassigned</option>
            {agents.map((a: any) => (
              <option key={a.id} value={a.id}>{a.name}</option>
            ))}
          </select>
        </div>

        <div>
          <label className="block text-xs font-medium text-slate-500 mb-1">Action</label>
          {actions.length > 0 ? (
            <select 
              value={node.data.action as string || ''} 
              onChange={(e) => updateNodeData(node.id, { action: e.target.value })}
              className="w-full px-3 py-2 bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-lg text-sm font-mono"
            >
              <option value="">Select an action...</option>
              {actions.map((act: string) => (
                <option key={act} value={act}>{act}</option>
              ))}
            </select>
          ) : (
            <input 
              type="text" 
              value={node.data.action as string || ''} 
              onChange={(e) => updateNodeData(node.id, { action: e.target.value })}
              className="w-full px-3 py-2 bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-lg text-sm font-mono"
              placeholder="Manual action input..."
            />
          )}
        </div>

        {onDeleteNode && (
          <div className="pt-4 border-t border-slate-200 dark:border-slate-800 mt-4">
            <button
              onClick={() => onDeleteNode(node.id)}
              className="w-full flex items-center justify-center gap-2 px-3 py-2 bg-red-50 hover:bg-red-100 dark:bg-red-900/30 dark:hover:bg-red-900/50 text-red-600 dark:text-red-400 border border-red-200 dark:border-red-900/50 rounded-lg text-sm font-medium transition-colors"
            >
              <AlertCircle size={16} />
              <span>Delete Node</span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function WorkflowBuilderInner() {
  const [workflows, setWorkflows] = useState<any[]>([]);
  const [agents, setAgents] = useState<any[]>([]);
  const [selectedWorkflowId, setSelectedWorkflowId] = useState<string>('');
  const [workflowDef, setWorkflowDef] = useState<WorkflowDefinition | null>(null);
  
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);
  
  const [isTriggering, setIsTriggering] = useState(false);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const { screenToFlowPosition, getViewport } = useReactFlow();
  
  const reactFlowWrapper = React.useRef<HTMLDivElement>(null);

  const updateNodeData = (id: string, newData: any) => {
    setNodes((nds) =>
      nds.map((n) => {
        if (n.id === id) {
          return { ...n, data: { ...n.data, ...newData } };
        }
        return n;
      })
    );
  };

  const handleDeleteNode = (id: string) => {
    setNodes((nds) => nds.filter((n) => n.id !== id));
    setEdges((eds) => eds.filter((e) => e.source !== id && e.target !== id));
  };

  // Load available workflows and agents on mount
  useEffect(() => {
    async function loadInitialData() {
      try {
        const list = await invoke<any[]>('list_workflows');
        setWorkflows(list);
        if (list.length > 0) {
          setSelectedWorkflowId(list[0].id);
        }

        const agentList = await invoke<any[]>('get_agent_statuses');
        setAgents(agentList);
      } catch (e) {
        console.error('Failed to list workflows or agents', e);
      }
    }
    loadInitialData();
  }, []);

  // Fetch workflow definition when selected
  useEffect(() => {
    async function fetchDef() {
      if (!selectedWorkflowId) return;
      try {
        const def = await invoke<WorkflowDefinition | null>('get_workflow_definition', { workflowId: selectedWorkflowId });
        if (def) {
          setWorkflowDef(def);
          buildGraph(def);
        }
      } catch (e) {
        console.error('Failed to fetch workflow definition', e);
      }
    }
    fetchDef();
  }, [selectedWorkflowId]);

  // Build React Flow graph from WorkflowDefinition
  const buildGraph = (def: WorkflowDefinition) => {
    const newNodes: Node[] = [];
    const newEdges: Edge[] = [];
    
    let yOffset = 50;
    
    def.steps.forEach((step, index) => {
      newNodes.push({
        id: step.id,
        type: 'step',
        position: { x: 250, y: yOffset },
        data: { 
          label: step.name, 
          agent: step.agent, 
          action: step.action,
          status: 'pending'
        },
      });
      
      // Determine next step
      let targetId = step.next_step;
      if (!targetId && index < def.steps.length - 1) {
        targetId = def.steps[index + 1]?.id;
      }
      
      if (targetId) {
        newEdges.push({
          id: `e-${step.id}-${targetId}`,
          source: step.id,
          target: targetId,
          markerEnd: {
            type: MarkerType.ArrowClosed,
          },
          label: step.condition ? 'Condition' : undefined,
          animated: false,
        });
      }
      
      yOffset += 150;
    });

    setNodes(newNodes);
    setEdges(newEdges);
  };

  // Listen to Tauri events for live progress
  useEffect(() => {
    const unlistenPromise = listen<WorkflowProgressUpdate>('workflow_progress', (event) => {
      const update = event.payload;
      if (!update) return;
      
      // Ignore updates for other workflows if we only want to track the selected one
      if (update.workflow_id !== selectedWorkflowId) return;
      
      if (update.type === 'run') {
        setActiveRunId(update.status === 'Running' || update.status === 'Pending' ? update.run_id : null);
        // Reset all nodes if a new run starts
        if (update.status === 'Running') {
          setNodes((nds) => nds.map(n => ({
            ...n,
            data: { ...n.data, status: 'pending', message: undefined }
          })));
          setEdges((eds) => eds.map(e => ({ ...e, animated: false })));
        }
      } else if (update.type === 'step' && update.step_id) {
        // Update node status
        setNodes((nds) => nds.map((n) => {
          if (n.id === update.step_id) {
            return {
              ...n,
              data: {
                ...n.data,
                status: update.status.toLowerCase(),
                message: update.message
              }
            };
          }
          return n;
        }));
        
        // Update edge animations
        if (update.status === 'Running') {
          setEdges((eds) => eds.map(e => ({
            ...e,
            animated: e.target === update.step_id || e.source === update.step_id
          })));
        } else {
          setEdges((eds) => eds.map(e => {
            if (e.source === update.step_id) {
               return { ...e, animated: false };
            }
            return e;
          }));
        }
      }
    });

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [selectedWorkflowId, setNodes, setEdges]);

  const handleTrigger = async () => {
    if (!selectedWorkflowId) return;
    setIsTriggering(true);
    try {
      await invoke('trigger_workflow', { workflowId: selectedWorkflowId, payload: null });
    } catch (e) {
      console.error('Trigger failed:', e);
    } finally {
      setIsTriggering(false);
    }
  };

  const handleReset = () => {
    if (workflowDef) {
      buildGraph(workflowDef);
    }
  };

  const onDragOver = React.useCallback((event: React.DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  }, []);

  const onDrop = React.useCallback(
    (event: React.DragEvent) => {
      event.preventDefault();

      if (!reactFlowWrapper.current) return;

      let nodeDataStr = event.dataTransfer.getData('application/reactflow');
      if (!nodeDataStr) {
        nodeDataStr = event.dataTransfer.getData('text/plain');
      }

      let nodeData;
      if (nodeDataStr) {
        try {
          nodeData = JSON.parse(nodeDataStr);
        } catch (e) {
          console.error("Failed to parse node data", e);
        }
      }

      if (!nodeData && draggedNodePayload) {
        nodeData = draggedNodePayload;
      }

      if (!nodeData) return;
      const position = screenToFlowPosition({
        x: event.clientX,
        y: event.clientY,
      });

      const newNode: Node = {
        id: getId(),
        type: nodeData.type,
        position,
        data: { label: `New ${nodeData.action}`, action: nodeData.action, agent: 'unassigned', status: 'pending' },
      };

      setNodes((nds) => nds.concat(newNode));
    },
    [screenToFlowPosition, setNodes],
  );

  const handleNodeClickAdd = (nodeType: string, actionName: string) => {
    // Determine center of current viewport
    const { x, y, zoom } = getViewport();
    const viewportWidth = window.innerWidth / 2; // approximation
    const viewportHeight = window.innerHeight / 2;
    
    // Reverse the viewport transform to get the center point in flow coordinates
    const centerX = (viewportWidth - x) / zoom;
    const centerY = (viewportHeight - y) / zoom;

    const newNode: Node = {
      id: getId(),
      type: nodeType,
      position: { x: centerX, y: centerY },
      data: { label: `New ${actionName}`, action: actionName, agent: 'unassigned', status: 'pending' },
    };

    setNodes((nds) => nds.concat(newNode));
  };

  const handleSave = async () => {
    // PRE-SAVE VALIDATION
    if (nodes.length === 0) {
      alert("Cannot save an empty workflow.");
      return;
    }
    
    // Check for unconfigured nodes
    const unconfigured = nodes.find(n => n.data.agent === 'unassigned' || !n.data.action || String(n.data.action).trim() === '');
    if (unconfigured) {
      alert(`Validation Error: Node "${unconfigured.data.label}" is missing an assigned Agent or Action.`);
      return;
    }
    
    // Check if there are completely isolated nodes
    if (nodes.length > 1) {
      const isolated = nodes.find(n => {
        const hasEdges = edges.some(e => e.source === n.id || e.target === n.id);
        return !hasEdges;
      });
      if (isolated) {
        alert(`Validation Error: Node "${isolated.data.label}" is disconnected from the workflow.`);
        return;
      }
    }

    try {
      const steps: WorkflowStep[] = nodes.map(node => {
        const outgoingEdge = edges.find(e => e.source === node.id);
        return {
          id: node.id,
          name: node.data.label as string,
          agent: node.data.agent as string,
          action: node.data.action as string,
          next_step: outgoingEdge ? outgoingEdge.target : undefined,
          condition: outgoingEdge && outgoingEdge.label === 'Condition' ? 'true' : undefined,
        };
      });

      const newDef = workflowDef ? {
        ...workflowDef,
        steps
      } : {
        id: `workflow-${Date.now()}`,
        name: 'New Workflow',
        description: 'Created from Visual Editor',
        trigger: { type: 'manual', config: {} },
        steps
      };

      await invoke('save_workflow_definition', { workflow: newDef });
      
      setWorkflowDef(newDef as WorkflowDefinition);
      
      const list = await invoke<any[]>('list_workflows');
      setWorkflows(list);
      if (!selectedWorkflowId) setSelectedWorkflowId(newDef.id);
      
      alert("Workflow saved successfully!");
    } catch (e) {
      console.error("Failed to save workflow:", e);
      alert(`Failed to save workflow: ${e}`);
    }
  };

  const handleWorkflowGenerated = (def: any) => {
    setWorkflowDef(def);
    setSelectedWorkflowId(def.id || 'new-workflow');
    buildGraph(def);
  };

  return (
    <div className="flex flex-col h-full bg-[var(--bg-main)] border-l border-[var(--border-default)] transition-theme">
      {/* Header */}
      <div className="px-6 py-4 border-b border-[var(--border-default)] bg-[var(--bg-card)] flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-[var(--text-primary)]">Visual Workflow Editor</h2>
          <p className="text-sm text-slate-500 dark:text-slate-400">View and track automated tasks</p>
        </div>
        
        <div className="flex items-center space-x-4">
          <select 
            value={selectedWorkflowId || ''}
            onChange={(e) => setSelectedWorkflowId(e.target.value)}
            className="px-3 py-2 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg text-sm outline-none focus:border-[var(--accent)] transition-colors"
          >
            {workflows.length === 0 && <option value="" disabled>No workflows available</option>}
            {workflows.map(w => (
              <option key={w.id} value={w.id}>{w.name}</option>
            ))}
          </select>
          
          <button 
            onClick={handleReset}
            className="p-2 text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] rounded-lg transition-colors"
            title="Reset Graph"
          >
            <RotateCcw size={18} />
          </button>
          
          <button 
            onClick={handleSave}
            className="flex items-center space-x-2 px-3 py-2 text-[var(--text-primary)] bg-[var(--bg-input)] hover:bg-[var(--bg-hover)] rounded-lg transition-colors text-sm font-medium border border-[var(--border-default)]"
            title="Save Workflow"
          >
            <Save size={16} />
            <span>Save</span>
          </button>
          
          <button
            onClick={handleTrigger}
            disabled={isTriggering || activeRunId !== null}
            className="flex items-center space-x-2 px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-slate-300 dark:disabled:bg-slate-700 text-white rounded-lg transition-colors text-sm font-medium"
          >
            {isTriggering || activeRunId !== null ? <Loader2 size={16} className="animate-spin" /> : <Play size={16} />}
            <span>{activeRunId ? 'Running...' : 'Trigger Manually'}</span>
          </button>
        </div>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 flex h-full relative">
        <Sidebar onNodeAdd={handleNodeClickAdd} />
        <div className="flex-1 w-full h-full relative" ref={reactFlowWrapper}>
          <ReactFlow
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onDragOver={onDragOver}
            onDrop={onDrop}
            nodeTypes={nodeTypes}
            fitView
            attributionPosition="bottom-right"
          >
            <Background color="#ccc" gap={16} />
            <Controls />
            <MiniMap zoomable pannable nodeClassName={() => `bg-blue-200`} />
          </ReactFlow>
        </div>
        
        {nodes.find(n => n.selected) && (
          <NodePropertiesPanel 
            node={nodes.find(n => n.selected)!} 
            updateNodeData={updateNodeData} 
            onDeleteNode={handleDeleteNode}
            agents={agents}
          />
        )}

        <WorkflowGeneratorChat onWorkflowGenerated={handleWorkflowGenerated} />
      </div>
    </div>
  );
}

export default function WorkflowBuilder() {
  return (
    <ReactFlowProvider>
      <WorkflowBuilderInner />
    </ReactFlowProvider>
  );
}
