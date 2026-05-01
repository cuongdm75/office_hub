import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Activity, RefreshCw, BarChart2, Zap, AlertCircle } from 'lucide-react';
import clsx from 'clsx';

interface GatewayMetrics {
  total_requests: number;
  successful_requests: number;
  failed_requests: number;
  cache_hits: number;
  cloud_requests: number;
  local_requests: number;
  total_prompt_tokens: number;
  total_completion_tokens: number;
  total_latency_ms: number;
  fallbacks_triggered: number;
  tokens_per_agent: Record<string, number>;
  tokens_per_model: Record<string, number>;
  success_per_model: Record<string, number>;
  failure_per_model: Record<string, number>;
}

export default function AIDashboard() {
  const [metrics, setMetrics] = useState<GatewayMetrics | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    fetchMetrics();
    // Optional: set up polling if we want real-time updates
    const interval = setInterval(fetchMetrics, 5000);
    return () => clearInterval(interval);
  }, []);

  const fetchMetrics = async () => {
    try {
      const data: GatewayMetrics = await invoke('get_llm_metrics');
      setMetrics(data);
    } catch (err) {
      console.error('Failed to fetch LLM metrics:', err);
    } finally {
      setIsLoading(false);
    }
  };

  if (isLoading && !metrics) {
    return (
      <div className="flex items-center justify-center h-full">
        <RefreshCw className="animate-spin text-slate-400" size={24} />
      </div>
    );
  }

  if (!metrics) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-slate-500">
        <AlertCircle size={48} className="mb-4 text-slate-400" />
        <p>Could not load AI metrics.</p>
      </div>
    );
  }

  const formatNumber = (num: number) => new Intl.NumberFormat().format(num);
  const totalTokens = metrics.total_prompt_tokens + metrics.total_completion_tokens;

  // Sorting agents by usage
  const topAgents = Object.entries(metrics.tokens_per_agent)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 5);

  const topModels = Object.entries(metrics.tokens_per_model)
    .sort((a, b) => b[1] - a[1]);

  return (
    <div className="space-y-6 animate-in fade-in duration-300 pb-12">
      <div className="flex justify-between items-center mb-4">
        <div>
          <h2 className="text-xl font-bold text-[var(--text-primary)]">AI Monitor</h2>
          <p className="text-[var(--text-secondary)] mt-1">Real-time resource utilization and token consumption.</p>
        </div>
        <button
          onClick={fetchMetrics}
          className="p-2 bg-[var(--bg-input)] hover:bg-[var(--bg-hover)] rounded-lg transition-colors border border-[var(--border-default)]"
        >
          <RefreshCw size={18} className="text-[var(--text-primary)]" />
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div className="bg-[var(--bg-card)] p-5 rounded-2xl border border-[var(--border-default)] shadow-sm">
          <div className="flex items-center space-x-2 text-[var(--text-secondary)] mb-2">
            <Activity size={16} />
            <h3 className="text-sm font-medium">Total Requests</h3>
          </div>
          <p className="text-3xl font-bold text-[var(--text-primary)]">{formatNumber(metrics.total_requests)}</p>
          <p className="text-xs text-[var(--text-muted)] mt-2">
            <span className="text-emerald-500">{metrics.successful_requests} ok</span>
            <span className="mx-2">•</span>
            <span className="text-rose-500">{metrics.failed_requests} failed</span>
          </p>
        </div>

        <div className="bg-[var(--bg-card)] p-5 rounded-2xl border border-[var(--border-default)] shadow-sm">
          <div className="flex items-center space-x-2 text-[var(--text-secondary)] mb-2">
            <BarChart2 size={16} />
            <h3 className="text-sm font-medium">Total Tokens</h3>
          </div>
          <p className="text-3xl font-bold text-[var(--text-primary)]">{formatNumber(totalTokens)}</p>
          <p className="text-xs text-[var(--text-muted)] mt-2">
            <span>{formatNumber(metrics.total_prompt_tokens)} in</span>
            <span className="mx-2">•</span>
            <span>{formatNumber(metrics.total_completion_tokens)} out</span>
          </p>
        </div>

        <div className="bg-[var(--bg-card)] p-5 rounded-2xl border border-[var(--border-default)] shadow-sm">
          <div className="flex items-center space-x-2 text-[var(--text-secondary)] mb-2">
            <Zap size={16} />
            <h3 className="text-sm font-medium">Cache Hits</h3>
          </div>
          <p className="text-3xl font-bold text-[var(--text-primary)]">{formatNumber(metrics.cache_hits)}</p>
          <p className="text-xs text-emerald-500 mt-2">Saved tokens & time</p>
        </div>

        <div className="bg-[var(--bg-card)] p-5 rounded-2xl border border-[var(--border-default)] shadow-sm">
          <div className="flex items-center space-x-2 text-[var(--text-secondary)] mb-2">
            <AlertCircle size={16} />
            <h3 className="text-sm font-medium">Fallbacks</h3>
          </div>
          <p className="text-3xl font-bold text-[var(--text-primary)]">{metrics.fallbacks_triggered}</p>
          <p className="text-xs text-[var(--text-muted)] mt-2">Dynamic routes activated</p>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-[var(--bg-card)] rounded-2xl border border-[var(--border-default)] shadow-sm overflow-hidden">
          <div className="px-6 py-4 border-b border-[var(--border-default)] bg-[var(--bg-app)]">
            <h3 className="font-semibold text-[var(--text-primary)]">Token Usage by Agent</h3>
          </div>
          <div className="p-0">
            {topAgents.length === 0 ? (
              <div className="p-6 text-center text-[var(--text-muted)]">No agent usage data available yet.</div>
            ) : (
              <table className="w-full text-sm text-left text-[var(--text-secondary)]">
                <thead className="text-xs text-[var(--text-muted)] uppercase bg-[var(--bg-app)]">
                  <tr>
                    <th scope="col" className="px-6 py-3">Agent</th>
                    <th scope="col" className="px-6 py-3 text-right">Tokens Consumed</th>
                  </tr>
                </thead>
                <tbody>
                  {topAgents.map(([agent, tokens]) => (
                    <tr key={agent} className="border-b border-[var(--border-default)] last:border-0 hover:bg-[var(--bg-hover)]">
                      <td className="px-6 py-4 font-medium text-[var(--text-primary)] capitalize flex items-center space-x-2">
                        <div className="w-2 h-2 rounded-full bg-blue-500"></div>
                        <span>{agent}</span>
                      </td>
                      <td className="px-6 py-4 text-right">{formatNumber(tokens)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </div>

        <div className="bg-[var(--bg-card)] rounded-2xl border border-[var(--border-default)] shadow-sm overflow-hidden">
          <div className="px-6 py-4 border-b border-[var(--border-default)] bg-[var(--bg-app)] flex items-center justify-between">
            <h3 className="font-semibold text-[var(--text-primary)]">Usage & Reliability by Model</h3>
          </div>
          <div className="p-0">
            {topModels.length === 0 ? (
              <div className="p-6 text-center text-[var(--text-muted)]">No model usage data available yet.</div>
            ) : (
              <table className="w-full text-sm text-left text-[var(--text-secondary)]">
                <thead className="text-xs text-[var(--text-muted)] uppercase bg-[var(--bg-app)]">
                  <tr>
                    <th scope="col" className="px-6 py-3">Provider / Model</th>
                    <th scope="col" className="px-4 py-3 text-center">Status</th>
                    <th scope="col" className="px-6 py-3 text-right">Tokens Consumed</th>
                  </tr>
                </thead>
                <tbody>
                  {topModels.map(([model, tokens]) => {
                    const success = metrics.success_per_model[model] || 0;
                    const fail = metrics.failure_per_model[model] || 0;
                    const totalRequests = success + fail;
                    const rate = totalRequests > 0 ? Math.round((success / totalRequests) * 100) : 0;
                    
                    return (
                      <tr key={model} className="border-b border-[var(--border-default)] last:border-0 hover:bg-[var(--bg-hover)]">
                        <td className="px-6 py-4 font-medium text-[var(--text-primary)]">{model}</td>
                        <td className="px-4 py-4 text-center">
                          {totalRequests > 0 ? (
                            <div className="flex flex-col items-center">
                              <span className={clsx("font-medium", rate >= 90 ? "text-emerald-500" : rate >= 70 ? "text-amber-500" : "text-rose-500")}>
                                {rate}%
                              </span>
                              <span className="text-[10px] text-[var(--text-muted)]">{success} / {totalRequests}</span>
                            </div>
                          ) : (
                            <span className="text-[var(--text-muted)]">-</span>
                          )}
                        </td>
                        <td className="px-6 py-4 text-right">{formatNumber(tokens)}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
          </div>
        </div>
      </div>
      
      {/* Optimization Tips */}
      {topAgents.length > 0 && topAgents[0] && topAgents[0][1] > 10000 && (
        <div className="bg-indigo-500/10 border border-indigo-500/20 rounded-xl p-4 flex items-start space-x-3">
          <Zap size={20} className="text-indigo-500 shrink-0 mt-0.5" />
          <div>
            <h4 className="font-medium text-indigo-500">Optimization Tip</h4>
            <p className="text-sm text-[var(--text-secondary)] mt-1">
              Agent <span className="font-semibold capitalize text-[var(--text-primary)]">{topAgents[0][0]}</span> has consumed {formatNumber(topAgents[0][1])} tokens. Consider assigning a "Fast Model" (like Llama 3 or Gemini Flash) for this agent if it mainly does simple text processing to reduce cost and latency.
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
