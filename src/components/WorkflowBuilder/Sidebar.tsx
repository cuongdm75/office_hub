import React from 'react';
import { Settings, LogIn, LogOut, Filter, Activity } from 'lucide-react';

export let draggedNodePayload: any = null;

interface SidebarProps {
  onNodeAdd?: (nodeType: string, actionName: string) => void;
}

export default function Sidebar({ onNodeAdd }: SidebarProps) {
  const onDragStart = (event: React.DragEvent, nodeType: string, actionName: string) => {
    const payload = { type: nodeType, action: actionName };
    draggedNodePayload = payload;
    
    const str = JSON.stringify(payload);
    event.dataTransfer.setData('application/reactflow', str);
    event.dataTransfer.setData('text/plain', str);
    event.dataTransfer.effectAllowed = 'move';
  };

  return (
    <div className="w-64 border-r border-[var(--border-default)] bg-[var(--bg-sidebar)] p-4 flex flex-col transition-theme">
      <h3 className="font-semibold text-[var(--text-primary)] mb-4 flex items-center gap-2">
        <Settings size={18} />
        Node Palette
      </h3>
      
      <p className="text-xs text-[var(--text-secondary)] mb-4">
        Drag and drop (or click) nodes to the canvas to build your workflow.
      </p>

      <div className="space-y-3">
        <div 
          className="flex items-center gap-3 p-3 border border-[var(--border-default)] rounded-lg cursor-grab hover:bg-[var(--bg-hover)] transition-colors"
          onDragStart={(event) => onDragStart(event, 'step', 'Trigger / Input')}
          onClick={() => onNodeAdd?.('step', 'Trigger / Input')}
          draggable
        >
          <div className="w-8 h-8 rounded-full bg-blue-100 text-blue-600 flex items-center justify-center">
            <LogIn size={16} />
          </div>
          <div className="text-sm font-medium text-[var(--text-primary)]">Input Node</div>
        </div>

        <div 
          className="flex items-center gap-3 p-3 border border-[var(--border-default)] rounded-lg cursor-grab hover:bg-[var(--bg-hover)] transition-colors"
          onDragStart={(event) => onDragStart(event, 'step', 'Condition / Filter')}
          onClick={() => onNodeAdd?.('step', 'Condition / Filter')}
          draggable
        >
          <div className="w-8 h-8 rounded-full bg-orange-100 text-orange-600 flex items-center justify-center">
            <Filter size={16} />
          </div>
          <div className="text-sm font-medium text-[var(--text-primary)]">Filter Node</div>
        </div>

        <div 
          className="flex items-center gap-3 p-3 border border-[var(--border-default)] rounded-lg cursor-grab hover:bg-[var(--bg-hover)] transition-colors"
          onDragStart={(event) => onDragStart(event, 'step', 'Execute Action')}
          onClick={() => onNodeAdd?.('step', 'Execute Action')}
          draggable
        >
          <div className="w-8 h-8 rounded-full bg-purple-100 text-purple-600 flex items-center justify-center">
            <Activity size={16} />
          </div>
          <div className="text-sm font-medium text-[var(--text-primary)]">Action Node</div>
        </div>

        <div 
          className="flex items-center gap-3 p-3 border border-[var(--border-default)] rounded-lg cursor-grab hover:bg-[var(--bg-hover)] transition-colors"
          onDragStart={(event) => onDragStart(event, 'step', 'Output Result')}
          onClick={() => onNodeAdd?.('step', 'Output Result')}
          draggable
        >
          <div className="w-8 h-8 rounded-full bg-green-100 text-green-600 flex items-center justify-center">
            <LogOut size={16} />
          </div>
          <div className="text-sm font-medium text-[var(--text-primary)]">Output Node</div>
        </div>
      </div>
    </div>
  );
}
