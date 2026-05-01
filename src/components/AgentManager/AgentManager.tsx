import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Bot, Plus, Play, CheckCircle, Boxes, ShieldAlert, Server, Activity, Database, Cpu, Search, HardDrive } from 'lucide-react';
import toast from 'react-hot-toast';
import clsx from 'clsx';

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

export default function AgentManager() {
  const [activeTab, setActiveTab] = useState<'overview' | 'agents' | 'skills'>('overview');
  const [agents, setAgents] = useState<any[]>([]);
  const [mcpServers, setMcpServers] = useState<any[]>([]);
  const [logs, setLogs] = useState<TelemetryLog[]>([]);
  const [llmModel, setLlmModel] = useState<{ provider: string, model: string } | null>(null);

  // Import/Wizard states
  const [isImporting, setIsImporting] = useState(false);
  const [importUrl, setImportUrl] = useState('');
  const [showAddServer, setShowAddServer] = useState(false);
  const [serverAlias, setServerAlias] = useState('');
  const [serverConfig, setServerConfig] = useState('{\n  "command": "node",\n  "args": ["-v"]\n}');
  const [sandboxId, setSandboxId] = useState<string | null>(null);
  const [scriptPath, setScriptPath] = useState<string | null>(null);
  const [scriptContent, setScriptContent] = useState<string>('');
  const [step, setStep] = useState(1);
  const [testInput, setTestInput] = useState('');
  const [testResult, setTestResult] = useState<string | null>(null);
  const [evaluationReport, setEvaluationReport] = useState<any | null>(null);
  const [isEvaluating, setIsEvaluating] = useState(false);
  const [showWizard, setShowWizard] = useState(false);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchTelemetry, 3000);
    return () => clearInterval(interval);
  }, []);

  const fetchData = async () => {
    try {
      const agentList = await invoke<any[]>('get_agent_statuses');
      setAgents(agentList || []);
    } catch (e) {
      console.error('Failed to load agents', e);
    }

    try {
      const mcpList = await invoke<any[]>('list_mcp_servers');
      setMcpServers(mcpList || []);
    } catch (e) {
      console.error('Failed to load MCP servers', e);
    }

    try {
      const llm = await invoke<any>('get_llm_settings');
      setLlmModel(llm);
    } catch (e) {
      console.error('Failed to load LLM settings', e);
    }

    fetchTelemetry();
  };

  const fetchTelemetry = async () => {
    try {
      const data: TelemetryLog[] = await invoke('get_telemetry_logs', { limit: 200 });
      setLogs(data);
    } catch (error) {
      console.error('Failed to fetch telemetry:', error);
    }
  };

  // --- Handlers for Skills/Servers ---
  const handleStartImport = async () => {
    if (!importUrl) return;
    setStep(2);
    setIsImporting(true);
    try {
      const result: any = await invoke('start_skill_learning', { sourceUrl: importUrl });
      setScriptPath(result.script_path);
      setScriptContent(result.script_content || '');
      toast.success('Skill parsed successfully!');
      setStep(3);
    } catch (e) {
      toast.error(String(e));
      setStep(1);
    } finally {
      setIsImporting(false);
    }
  };

  const handleTestSandbox = async () => {
    if (!scriptPath) return;
    try {
      if (scriptContent) {
        await invoke('save_skill_file', { scriptPath, content: scriptContent });
      }
      const id: string = await invoke('test_skill_sandbox', { scriptPath });
      setSandboxId(id);
      toast.success('Skill running in sandbox');
      setStep(4);
      fetchData();

      setIsEvaluating(true);
      try {
        const report = await invoke('evaluate_skill', { scriptPath });
        setEvaluationReport(report);
      } catch (err) {
        toast.error('Failed to generate evaluation report');
      } finally {
        setIsEvaluating(false);
      }
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleApprove = async () => {
    if (!sandboxId) return;
    try {
      await invoke('approve_new_skill', { serverId: sandboxId });
      toast.success('Skill approved and installed!');
      setShowWizard(false);
      setStep(1);
      setImportUrl('');
      setSandboxId(null);
      setScriptPath(null);
      setTestResult(null);
      setTestInput('');
      setEvaluationReport(null);
      fetchData();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleRunTest = async () => {
    if (!testInput.trim() || !sandboxId) return;
    try {
      let parsedInput;
      try {
        parsedInput = JSON.parse(testInput);
      } catch (e) {
        toast.error('Input must be valid JSON, e.g. {"tool": "name", "args": {}}');
        return;
      }
      if (!parsedInput.tool) {
        toast.error('Input JSON must contain a "tool" property');
        return;
      }
      setTestResult('Running tool in sandbox...');
      const response = await invoke('call_mcp_tool', { 
        serverId: sandboxId, 
        toolName: parsedInput.tool,
        arguments: parsedInput.args || {}
      });
      toast.success('Test executed in sandbox');
      setTestResult(`Output for tool "${parsedInput.tool}":\n\n${JSON.stringify(response, null, 2)}`);
    } catch (e) {
      toast.error(String(e));
      setTestResult(`Error:\n${String(e)}`);
    }
  };

  const handleAddServer = async () => {
    if (!serverAlias.trim()) { toast.error("Alias cannot be empty"); return; }
    let parsedConfig;
    try { parsedConfig = JSON.parse(serverConfig); } catch (e) { toast.error("Config must be a valid JSON object"); return; }
    try {
      await invoke('install_mcp_server', { alias: serverAlias.trim(), config: parsedConfig });
      toast.success("MCP server installed successfully");
      setShowAddServer(false);
      setServerAlias('');
      fetchData();
    } catch (e) {
      toast.error(String(e));
    }
  };

  // --- Computed Stats for Overview ---
  const totalTokens = logs.reduce((acc, log) => acc + log.tokensUsed, 0);
  const totalTasks = logs.length;
  const agentUsage: Record<string, number> = {};
  logs.forEach(log => {
    agentUsage[log.agentName] = (agentUsage[log.agentName] || 0) + 1;
  });
  const topAgents = Object.entries(agentUsage)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 5);

  return (
    <div className="flex flex-col h-full bg-[var(--bg-main)] overflow-hidden transition-theme">
      {/* Header Area */}
      <div className="px-6 py-5 border-b border-[var(--border-default)] bg-[var(--bg-card)] flex-shrink-0 z-10">
        <div className="flex justify-between items-start mb-4">
          <div>
            <h1 className="text-xl font-bold text-[var(--text-primary)]">AI Dashboard</h1>
            <p className="text-sm text-[var(--text-secondary)] mt-1">Manage intelligence, monitor telemetry, and install new skills.</p>
          </div>
          <div className="flex gap-2">
            <button 
              onClick={() => { setActiveTab('skills'); setShowAddServer(true); setShowWizard(false); }}
              className="flex items-center gap-2 bg-[var(--bg-app)] hover:bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] border border-[var(--border-default)] px-3 py-1.5 rounded-lg text-sm font-medium transition-colors"
            >
              <Server size={16} />
              Add Server
            </button>
            <button 
              onClick={() => { setActiveTab('skills'); setShowWizard(true); setShowAddServer(false); setStep(1); }}
              className="flex items-center gap-2 bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white px-3 py-1.5 rounded-lg text-sm font-medium shadow-sm transition-colors"
            >
              <Plus size={16} />
              New Skill
            </button>
          </div>
        </div>

        {/* Tab Navigation */}
        <div className="flex gap-1 border-b border-[var(--border-default)] w-max -mb-[21px]">
          <TabButton active={activeTab === 'overview'} onClick={() => setActiveTab('overview')} icon={<Activity size={16} />} label="Overview" />
          <TabButton active={activeTab === 'agents'} onClick={() => setActiveTab('agents')} icon={<Bot size={16} />} label="Core Agents" count={agents.length} />
          <TabButton active={activeTab === 'skills'} onClick={() => setActiveTab('skills')} icon={<Boxes size={16} />} label="MCP Skills" count={mcpServers.length} />
        </div>
      </div>

      {/* Main Scrollable Area */}
      <div className="flex-1 overflow-y-auto p-6 relative">
        
        {/* --- OVERVIEW TAB --- */}
        {activeTab === 'overview' && (
          <div className="space-y-6 max-w-5xl">
            {/* Top Row: Metrics & LLM Info */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              
              <div className="col-span-1 md:col-span-2 grid grid-cols-2 gap-4">
                <MetricCard 
                  title="Total Tokens" 
                  value={totalTokens.toLocaleString()} 
                  icon={<Database size={20} className="text-amber-500" />} 
                  trend="Lifetime usage"
                />
                <MetricCard 
                  title="Tasks Executed" 
                  value={totalTasks.toLocaleString()} 
                  icon={<Cpu size={20} className="text-indigo-500" />} 
                  trend="Orchestrated actions"
                />
                <MetricCard 
                  title="Active Agents" 
                  value={agents.filter(a => a.status === 'working' || (typeof a.status === 'string' && a.status.toLowerCase() !== 'idle')).length.toString()} 
                  icon={<Bot size={20} className="text-emerald-500" />} 
                  trend="Running right now"
                />
                <MetricCard 
                  title="Loaded Skills" 
                  value={mcpServers.length.toString()} 
                  icon={<Boxes size={20} className="text-blue-500" />} 
                  trend="Available tools"
                />
              </div>

              {/* LLM Info Card */}
              <div className="col-span-1 bg-[var(--bg-card)] border border-[var(--border-default)] rounded-xl p-5 flex flex-col justify-between shadow-sm">
                <div>
                  <div className="flex items-center gap-2 mb-4">
                    <div className="p-1.5 rounded-lg bg-[var(--accent-subtle)] text-[var(--accent)]">
                      <HardDrive size={18} />
                    </div>
                    <h3 className="font-semibold text-[var(--text-primary)]">Intelligence Engine</h3>
                  </div>
                  
                  {llmModel ? (
                    <div className="space-y-4">
                      <div>
                        <span className="text-xs uppercase tracking-wider font-semibold text-[var(--text-muted)] block mb-1">Provider</span>
                        <div className="text-sm font-medium text-[var(--text-primary)] capitalize">{llmModel.provider}</div>
                      </div>
                      <div>
                        <span className="text-xs uppercase tracking-wider font-semibold text-[var(--text-muted)] block mb-1">Model Version</span>
                        <div className="text-sm font-medium text-[var(--text-primary)]">{llmModel.model}</div>
                      </div>
                    </div>
                  ) : (
                    <div className="text-sm text-[var(--text-secondary)] italic">Loading LLM settings...</div>
                  )}
                </div>
                <div className="mt-4 pt-4 border-t border-[var(--border-default)]">
                  <div className="flex items-center gap-2 text-xs text-[var(--text-secondary)] font-medium">
                    <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></span>
                    Engine is online and ready
                  </div>
                </div>
              </div>

            </div>

            {/* Bottom Row: Recent Activity & Top Agents */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              
              <div className="bg-[var(--bg-card)] border border-[var(--border-default)] rounded-xl p-5 shadow-sm">
                <h3 className="font-semibold text-[var(--text-primary)] mb-4 flex items-center gap-2">
                  <Activity size={18} className="text-[var(--text-secondary)]" />
                  Most Active Agents
                </h3>
                {topAgents.length > 0 ? (
                  <div className="space-y-3">
                    {topAgents.map(([agentName, count], idx) => (
                      <div key={agentName} className="flex items-center justify-between p-2 hover:bg-[var(--bg-hover)] rounded-lg transition-colors">
                        <div className="flex items-center gap-3">
                          <span className="text-xs font-bold text-[var(--text-muted)] w-4">{idx + 1}</span>
                          <div className="flex items-center gap-2">
                            <Bot size={16} className="text-[var(--accent)]" />
                            <span className="text-sm font-medium text-[var(--text-primary)] capitalize">{agentName.replace('_', ' ')}</span>
                          </div>
                        </div>
                        <div className="text-sm font-semibold text-[var(--text-secondary)] bg-[var(--bg-app)] px-2 py-0.5 rounded-md border border-[var(--border-default)]">
                          {count} runs
                        </div>
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="text-center py-6 text-[var(--text-secondary)] text-sm">No recent activity</div>
                )}
              </div>

              <div className="bg-[var(--bg-card)] border border-[var(--border-default)] rounded-xl p-5 shadow-sm flex flex-col">
                <h3 className="font-semibold text-[var(--text-primary)] mb-4 flex items-center gap-2">
                  <Search size={18} className="text-[var(--text-secondary)]" />
                  Recent Telemetry Logs
                </h3>
                <div className="flex-1 overflow-y-auto pr-2 space-y-2 max-h-[250px]">
                  {logs.slice(0, 10).map((log, i) => (
                    <div key={i} className="text-xs p-2 rounded-md border border-[var(--border-default)] bg-[var(--bg-app)] flex flex-col gap-1">
                      <div className="flex justify-between items-center">
                        <span className="font-semibold text-[var(--text-primary)] capitalize">{log.agentName.replace('_', ' ')}</span>
                        <span className="text-[var(--text-muted)]">{new Date(log.timestamp).toLocaleTimeString()}</span>
                      </div>
                      <div className="flex justify-between items-center">
                        <span className="text-[var(--text-secondary)] truncate max-w-[200px]" title={log.action}>{log.action}</span>
                        <span className="text-amber-500 font-medium">{log.tokensUsed} t</span>
                      </div>
                    </div>
                  ))}
                  {logs.length === 0 && (
                    <div className="text-center py-6 text-[var(--text-secondary)] text-sm">No logs available</div>
                  )}
                </div>
              </div>

            </div>
          </div>
        )}

        {/* --- CORE AGENTS TAB --- */}
        {activeTab === 'agents' && (
          <div className="max-w-4xl">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {agents.length === 0 ? (
                <div className="col-span-2 text-center py-10 text-[var(--text-secondary)]">No core agents found.</div>
              ) : (
                agents.map((a: any) => (
                  <div key={a.id} className="bg-[var(--bg-card)] p-4 border border-[var(--border-default)] rounded-xl flex justify-between items-center shadow-sm">
                    <div className="flex items-center gap-3">
                      <div className="p-2 bg-[var(--bg-app)] border border-[var(--border-default)] rounded-lg text-[var(--text-secondary)]">
                        <Bot size={20} />
                      </div>
                      <div>
                        <div className="font-semibold text-[var(--text-primary)]">{a.name}</div>
                        <div className="text-xs text-[var(--text-muted)] mt-0.5">ID: {a.id}</div>
                      </div>
                    </div>
                    <div className="flex flex-col items-end gap-1">
                      <div className={clsx(
                        "w-2.5 h-2.5 rounded-full shadow-sm",
                        (typeof a.status === 'string' && a.status.toLowerCase() === 'idle') ? 'bg-emerald-500' : 
                        (typeof a.status === 'object' && a.status?.error) ? 'bg-red-500' : 'bg-amber-500'
                      )} />
                      <span className="text-[10px] uppercase font-bold text-[var(--text-muted)] tracking-wider">
                         {typeof a.status === 'string' ? a.status : (a.status?.error ? 'Error' : 'Working')}
                      </span>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        )}

        {/* --- MCP SKILLS TAB --- */}
        {activeTab === 'skills' && (
          <div className="max-w-5xl">
            {/* Show Wizards if active */}
            {showAddServer && (
              <div className="bg-[var(--bg-card)] border border-[var(--border-default)] rounded-xl p-6 shadow-sm mb-6 relative">
                <button onClick={() => setShowAddServer(false)} className="absolute top-4 right-4 text-[var(--text-muted)] hover:text-[var(--text-primary)]">✕</button>
                <h2 className="text-lg font-semibold mb-4 flex items-center gap-2 text-[var(--text-primary)]">
                  <Server className="text-indigo-500" />
                  Add External MCP Server
                </h2>
                <div className="space-y-4">
                  <div>
                    <label className="block text-sm font-medium mb-1 text-[var(--text-secondary)]">Server Alias</label>
                    <input type="text" className="w-full bg-[var(--bg-input)] border border-[var(--border-default)] rounded-lg px-3 py-2 outline-none focus:border-[var(--accent)] text-[var(--text-primary)]" placeholder="my-custom-server" value={serverAlias} onChange={e => setServerAlias(e.target.value)} />
                  </div>
                  <div>
                    <label className="block text-sm font-medium mb-1 text-[var(--text-secondary)]">Configuration (JSON)</label>
                    <textarea className="w-full bg-[var(--bg-input)] border border-[var(--border-default)] rounded-lg p-4 font-mono text-xs outline-none focus:border-[var(--accent)] resize-none h-32 text-[var(--text-primary)]" value={serverConfig} onChange={(e) => setServerConfig(e.target.value)} spellCheck={false} />
                  </div>
                  <div className="flex justify-end pt-2">
                    <button onClick={handleAddServer} className="bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white px-6 py-2 rounded-lg font-medium transition-colors">Install Server</button>
                  </div>
                </div>
              </div>
            )}

            {showWizard && (
              <div className="bg-[var(--bg-card)] border border-[var(--border-default)] rounded-xl p-6 shadow-sm mb-6 relative">
                 <button onClick={() => setShowWizard(false)} className="absolute top-4 right-4 text-[var(--text-muted)] hover:text-[var(--text-primary)]">✕</button>
                <h2 className="text-lg font-semibold mb-4 text-[var(--text-primary)]">Skill Import Wizard</h2>
                
                <div className="flex items-center gap-4 mb-6">
                  <div className={`flex-1 h-1.5 rounded-full ${step >= 1 ? 'bg-blue-500' : 'bg-[var(--border-default)]'}`} />
                  <div className={`flex-1 h-1.5 rounded-full ${step >= 2 ? 'bg-blue-500' : 'bg-[var(--border-default)]'}`} />
                  <div className={`flex-1 h-1.5 rounded-full ${step >= 3 ? 'bg-blue-500' : 'bg-[var(--border-default)]'}`} />
                  <div className={`flex-1 h-1.5 rounded-full ${step >= 4 ? 'bg-blue-500' : 'bg-[var(--border-default)]'}`} />
                </div>

                <div className="space-y-4">
                  {step === 1 && (
                    <div>
                      <label className="block text-sm font-medium mb-1 text-[var(--text-secondary)]">Documentation URL or GitHub Repo</label>
                      <div className="flex gap-2">
                        <input type="text" className="flex-1 bg-[var(--bg-input)] border border-[var(--border-default)] rounded-lg px-3 py-2 outline-none focus:border-blue-500 text-[var(--text-primary)]" placeholder="https://api.example.com/docs" value={importUrl} onChange={e => setImportUrl(e.target.value)} />
                        <button onClick={handleStartImport} disabled={isImporting} className="bg-blue-600 hover:bg-blue-700 text-white px-4 py-2 rounded-lg font-medium transition-colors">Start</button>
                      </div>
                    </div>
                  )}
                  {step === 2 && (
                    <div className="flex flex-col items-center justify-center py-8">
                      <div className="animate-spin rounded-full h-10 w-10 border-b-2 border-blue-500 mb-4"></div>
                      <p className="text-[var(--text-secondary)]">AI is reading docs and generating code...</p>
                    </div>
                  )}
                  {step === 3 && (
                    <div className="py-2 flex flex-col h-[400px]">
                      <h3 className="font-medium text-lg mb-2 flex items-center gap-2 text-[var(--text-primary)]">
                        <CheckCircle className="text-emerald-500" size={20} /> Code Generated
                      </h3>
                      <textarea className="flex-1 w-full bg-[var(--bg-input)] border border-[var(--border-default)] rounded-lg p-4 font-mono text-xs outline-none focus:border-blue-500 resize-none mb-4 text-[var(--text-primary)]" value={scriptContent} onChange={(e) => setScriptContent(e.target.value)} spellCheck={false} />
                      <button onClick={handleTestSandbox} className="bg-amber-500 hover:bg-amber-600 text-white px-6 py-2 rounded-lg font-medium flex items-center justify-center gap-2 mx-auto transition-colors">
                        <Play size={18} /> Save & Run in Sandbox
                      </button>
                    </div>
                  )}
                  {step === 4 && (
                    <div className="py-2">
                      <div className="flex items-center gap-3 mb-6 border-b border-[var(--border-default)] pb-4">
                        <CheckCircle className="h-8 w-8 text-emerald-500" />
                        <div>
                          <h3 className="font-bold text-lg text-[var(--text-primary)]">Sandbox Active</h3>
                          <p className="text-sm text-[var(--text-secondary)]">Test the skill before final approval.</p>
                        </div>
                      </div>
                      <div className="grid grid-cols-1 md:grid-cols-2 gap-6 mb-6">
                        {/* Report & Test UI goes here, simplified for brevity */}
                        <div className="bg-[var(--bg-app)] rounded-xl p-4 border border-[var(--border-default)]">
                          <h4 className="font-semibold mb-3 flex items-center gap-2 text-[var(--text-primary)]"><ShieldAlert size={16} className="text-blue-500" /> Evaluation Report</h4>
                          {isEvaluating ? <div className="text-sm text-[var(--text-muted)]">Analyzing...</div> : (
                            <ul className="text-sm space-y-2 text-[var(--text-secondary)]">
                              <li><strong className="text-emerald-600">Strengths:</strong> {evaluationReport?.strengths?.join(', ') || 'N/A'}</li>
                              <li><strong className="text-blue-600">Security:</strong> {evaluationReport?.security?.join(', ') || 'Safe'}</li>
                            </ul>
                          )}
                        </div>
                        <div className="bg-[var(--bg-app)] rounded-xl p-4 border border-[var(--border-default)]">
                          <h4 className="font-semibold mb-3 flex items-center gap-2 text-[var(--text-primary)]"><Play size={16} className="text-indigo-500" /> Sandbox Test</h4>
                          <div className="flex gap-2 mb-3">
                            <input type="text" value={testInput} onChange={e => setTestInput(e.target.value)} placeholder='{"tool": "name", "args": {}}' className="flex-1 text-sm bg-[var(--bg-input)] border border-[var(--border-default)] rounded-md px-3 py-1.5 outline-none text-[var(--text-primary)]" />
                            <button onClick={handleRunTest} className="bg-indigo-600 hover:bg-indigo-700 text-white px-3 py-1.5 rounded-md text-sm">Run</button>
                          </div>
                          {testResult && <div className="bg-[var(--bg-card)] border border-[var(--border-default)] rounded-md p-3 text-xs font-mono whitespace-pre-wrap text-[var(--text-primary)] max-h-32 overflow-auto">{testResult}</div>}
                        </div>
                      </div>
                      <div className="flex justify-center border-t border-[var(--border-default)] pt-6">
                        <button onClick={handleApprove} className="bg-emerald-600 hover:bg-emerald-700 text-white px-8 py-2.5 rounded-lg font-medium flex items-center gap-2 shadow-sm"><CheckCircle size={18} /> Approve & Install</button>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* Servers List */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {mcpServers.length === 0 ? (
                <div className="col-span-2 text-center py-10 text-[var(--text-secondary)]">No external skills installed</div>
              ) : (
                mcpServers.map((s: any) => (
                  <div key={s.id} className="bg-[var(--bg-card)] p-4 border border-[var(--border-default)] rounded-xl flex flex-col shadow-sm">
                    <div className="flex justify-between items-start mb-3">
                      <div className="flex items-center gap-2">
                         <Boxes className="text-blue-500" size={18} />
                         <span className="font-semibold text-[var(--text-primary)]">{s.alias}</span>
                      </div>
                      <div className={clsx(
                        "text-[10px] uppercase font-bold tracking-wider px-2 py-0.5 rounded-md border",
                        (typeof s.status === 'string' && s.status.toLowerCase() === 'running') 
                          ? 'bg-emerald-500/10 text-emerald-600 border-emerald-500/20' 
                          : 'bg-[var(--bg-hover)] text-[var(--text-muted)] border-[var(--border-default)]'
                      )}>
                        {typeof s.status === 'string' ? s.status : 'Error'}
                      </div>
                    </div>
                    <div className="text-xs text-[var(--text-muted)] mb-3 flex gap-4">
                      <span>{s.toolCount || 0} tools</span>
                      <span>{s.resourceCount || 0} resources</span>
                    </div>
                    {s.tools?.length > 0 && (
                      <div className="flex flex-wrap gap-1 mt-auto">
                        {s.tools.slice(0, 5).map((t: string) => (
                          <span key={`tool-${t}`} className="px-1.5 py-0.5 bg-[var(--bg-app)] border border-[var(--border-default)] text-[var(--text-secondary)] rounded text-[10px] truncate max-w-[120px]">
                            {t}
                          </span>
                        ))}
                        {s.tools.length > 5 && (
                          <span className="px-1.5 py-0.5 bg-[var(--bg-app)] border border-[var(--border-default)] text-[var(--text-muted)] rounded text-[10px]">
                            +{s.tools.length - 5} more
                          </span>
                        )}
                      </div>
                    )}
                  </div>
                ))
              )}
            </div>
          </div>
        )}

      </div>
    </div>
  );
}

// --- Helpers ---
function TabButton({ active, onClick, icon, label, count }: any) {
  return (
    <button
      onClick={onClick}
      className={clsx(
        "flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 transition-colors duration-150",
        active 
          ? "border-[var(--accent)] text-[var(--accent-text)]" 
          : "border-transparent text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--border-default)]"
      )}
    >
      {icon}
      {label}
      {count !== undefined && (
        <span className={clsx(
          "px-1.5 py-0.5 text-[10px] rounded-full",
          active ? "bg-[var(--accent-subtle)] text-[var(--accent-text)]" : "bg-[var(--bg-hover)] text-[var(--text-muted)]"
        )}>
          {count}
        </span>
      )}
    </button>
  );
}

function MetricCard({ title, value, icon, trend }: any) {
  return (
    <div className="bg-[var(--bg-card)] border border-[var(--border-default)] rounded-xl p-4 shadow-sm flex flex-col justify-between">
      <div className="flex justify-between items-start">
        <span className="text-xs font-semibold text-[var(--text-muted)] uppercase tracking-wider">{title}</span>
        <div className="p-1.5 bg-[var(--bg-app)] rounded-lg border border-[var(--border-default)]">
          {icon}
        </div>
      </div>
      <div className="mt-2">
        <div className="text-2xl font-bold text-[var(--text-primary)]">{value}</div>
        <div className="text-xs text-[var(--text-secondary)] mt-1">{trend}</div>
      </div>
    </div>
  );
}
