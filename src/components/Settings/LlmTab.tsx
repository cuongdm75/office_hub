import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Save, CheckCircle2, XCircle, RefreshCw, Server, Key, BrainCircuit } from 'lucide-react';
import clsx from 'clsx';
import toast from 'react-hot-toast';

export interface ProviderCredentials {
  gemini_api_key?: string | null;
  openai_api_key?: string | null;
  anthropic_api_key?: string | null;
  zai_api_key?: string | null;
  ollama_endpoint?: string | null;
  lmstudio_endpoint?: string | null;
}

export interface LlmConfig {
  fast_provider: string;
  fast_model: string;
  default_provider: string;
  default_model: string;
  reasoning_provider: string;
  reasoning_model: string;
  credentials: ProviderCredentials;
  context_window_limit: number;
  token_cache_enabled: boolean;
  auto_handoff_enabled: boolean;
}

const PROVIDERS = [
  { id: 'gemini', name: 'Google Gemini', type: 'cloud' },
  { id: 'openai', name: 'OpenAI', type: 'cloud' },
  { id: 'anthropic', name: 'Anthropic Claude', type: 'cloud' },
  { id: 'z.ai', name: 'Z.AI', type: 'cloud' },
  { id: 'ollama', name: 'Ollama (Local)', type: 'local' },
  { id: 'lmstudio', name: 'LM Studio (Local)', type: 'local' },
];

export default function LlmTab() {
  const [settings, setSettings] = useState<LlmConfig>({
    fast_provider: 'ollama',
    fast_model: '',
    default_provider: 'gemini',
    default_model: '',
    reasoning_provider: 'anthropic',
    reasoning_model: '',
    credentials: {},
    context_window_limit: 32000,
    token_cache_enabled: true,
    auto_handoff_enabled: false,
  });
  
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [isTesting, setIsTesting] = useState(false);
  const [testResult, setTestResult] = useState<boolean | null>(null);

  // We cache models by provider name to avoid refetching
  const [modelsCache, setModelsCache] = useState<Record<string, string[]>>({});
  const [fetchingProvider, setFetchingProvider] = useState<string | null>(null);

  useEffect(() => {
    loadSettings();
  }, []);

  const loadSettings = async () => {
    try {
      setIsLoading(true);
      const data: LlmConfig = await invoke('get_llm_settings');
      setSettings(prev => ({
        ...prev,
        ...data,
        credentials: data.credentials || prev.credentials || {},
      }));
      // Kick off fetching models for the 3 current providers
      fetchModelsForProvider(data.fast_provider, data);
      fetchModelsForProvider(data.default_provider, data);
      fetchModelsForProvider(data.reasoning_provider, data);
    } catch (error) {
      console.error('Failed to load settings:', error);
      toast.error('Failed to load settings');
    } finally {
      setIsLoading(false);
    }
  };

  const fetchModelsForProvider = async (provider: string, currentSettings: LlmConfig = settings) => {
    if (!provider) return;
    try {
      setFetchingProvider(provider);
      let endpoint = null;
      if (provider === 'ollama') endpoint = currentSettings.credentials.ollama_endpoint;
      if (provider === 'lmstudio') endpoint = currentSettings.credentials.lmstudio_endpoint;

      let api_key = null;
      if (provider === 'openai') api_key = currentSettings.credentials.openai_api_key;
      if (provider === 'gemini') api_key = currentSettings.credentials.gemini_api_key;
      if (provider === 'anthropic') api_key = currentSettings.credentials.anthropic_api_key;

      const models: string[] = await invoke('get_available_models', {
        provider: provider,
        endpoint,
        apiKey: api_key
      });
      
      setModelsCache(prev => ({ ...prev, [provider]: models }));
    } catch (error) {
      console.error(`Failed to fetch models for ${provider}:`, error);
    } finally {
      setFetchingProvider(null);
    }
  };

  const handleSave = async () => {
    try {
      setIsSaving(true);
      await invoke('update_llm_settings', { settings });
      toast.success('Settings saved successfully');
      setTestResult(null);
    } catch (error) {
      console.error('Failed to save settings:', error);
      toast.error('Failed to save settings');
    } finally {
      setIsSaving(false);
    }
  };

  const handleTestConnection = async () => {
    try {
      setIsTesting(true);
      setTestResult(null);
      // We test the default provider for overall connectivity health
      const isHealthy: boolean = await invoke('test_llm_connection');
      setTestResult(isHealthy);
      if (isHealthy) {
        toast.success('Connection test successful!');
      } else {
        toast.error('Connection test failed. Check credentials.');
      }
    } catch (error) {
      console.error('Test connection failed:', error);
      setTestResult(false);
      toast.error('Connection test failed.');
    } finally {
      setIsTesting(false);
    }
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <RefreshCw className="animate-spin text-slate-400" size={24} />
      </div>
    );
  }

  const updateCredentials = (key: keyof ProviderCredentials, value: string) => {
    setSettings(prev => ({
      ...prev,
      credentials: { ...prev.credentials, [key]: value || null }
    }));
  };

  const renderTierSetup = (
    tierName: string, 
    desc: string,
    providerKey: 'fast_provider' | 'default_provider' | 'reasoning_provider',
    modelKey: 'fast_model' | 'default_model' | 'reasoning_model'
  ) => {
    const activeProvider = settings[providerKey];
    const activeModel = settings[modelKey];
    const availableModels = modelsCache[activeProvider] || [];
    const isFetching = fetchingProvider === activeProvider;

    return (
      <div className="p-5 border border-slate-200 dark:border-slate-800 rounded-xl bg-slate-50/30 dark:bg-slate-900/30">
        <div className="mb-4">
          <h3 className="text-sm font-bold text-slate-800 dark:text-slate-200 uppercase tracking-wider">{tierName} TIER</h3>
          <p className="text-xs text-slate-500 mt-1">{desc}</p>
        </div>
        <div className="space-y-4">
          <div>
            <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1.5">Provider</label>
            <select
              value={activeProvider}
              onChange={(e) => {
                const newProvider = e.target.value;
                setSettings({ ...settings, [providerKey]: newProvider });
                fetchModelsForProvider(newProvider, settings);
              }}
              className="w-full px-3 py-2 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg text-sm focus:ring-2 focus:ring-[var(--accent)] outline-none"
            >
              {PROVIDERS.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
            </select>
          </div>
          <div>
            <label className="flex justify-between items-center text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1.5">
              <span>Model</span>
              <button onClick={() => fetchModelsForProvider(activeProvider)} disabled={isFetching} className="text-blue-500 hover:text-blue-600">
                <RefreshCw size={12} className={clsx(isFetching && "animate-spin")} />
              </button>
            </label>
            <input
              type="text"
              value={activeModel || ''}
              onChange={(e) => setSettings({ ...settings, [modelKey]: e.target.value })}
              list={`models-${tierName}`}
              placeholder="e.g. gpt-4o"
              className="w-full px-3 py-2 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg text-sm focus:ring-2 focus:ring-[var(--accent)] outline-none"
            />
            <datalist id={`models-${tierName}`}>
              {availableModels.map(m => <option key={m} value={m} />)}
            </datalist>
          </div>
        </div>
      </div>
    );
  };

  // Extract all unique selected providers to show credential fields
  const activeProviders = Array.from(new Set([settings.fast_provider, settings.default_provider, settings.reasoning_provider]));

  return (
    <div className="space-y-8 animate-in fade-in duration-300 pb-12">
      <div className="flex justify-between items-center mb-6">
        <div>
          <h2 className="text-xl font-bold text-slate-800 dark:text-slate-100">LLM Provider Configuration</h2>
          <p className="text-slate-500 dark:text-slate-400 mt-1">Configure your multi-tier routing logic.</p>
        </div>
        <div className="flex space-x-3">
          <button
            onClick={handleTestConnection}
            disabled={isTesting || isSaving}
            className="flex items-center px-4 py-2 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 rounded-lg font-medium transition-colors disabled:opacity-50"
          >
            {isTesting ? <RefreshCw className="animate-spin mr-2" size={18} /> : <BrainCircuit className="mr-2" size={18} />}
            Test Connection
          </button>
          <button
            onClick={handleSave}
            disabled={isSaving || isTesting}
            className="flex items-center px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition-colors disabled:opacity-50 shadow-sm"
          >
            {isSaving ? <RefreshCw className="animate-spin mr-2" size={18} /> : <Save className="mr-2" size={18} />}
            Save Changes
          </button>
        </div>
      </div>

      {testResult !== null && (
        <div className={clsx(
          "p-4 rounded-xl border flex items-start space-x-3",
          testResult 
            ? "bg-emerald-50 border-emerald-200 text-emerald-800 dark:bg-emerald-900/20 dark:border-emerald-800/50 dark:text-emerald-400" 
            : "bg-red-50 border-red-200 text-red-800 dark:bg-red-900/20 dark:border-red-800/50 dark:text-red-400"
        )}>
          {testResult ? <CheckCircle2 size={24} className="mt-0.5" /> : <XCircle size={24} className="mt-0.5" />}
          <div>
            <h3 className="font-semibold">{testResult ? 'Connection Successful' : 'Connection Failed'}</h3>
            <p className="text-sm mt-1 opacity-90">
              {testResult 
                ? 'Office Hub successfully communicated with the default LLM provider.' 
                : 'Office Hub could not reach the LLM provider. Please check your credentials or endpoints.'}
            </p>
          </div>
        </div>
      )}

      {/* Routing Matrix Configuration */}
      <div className="bg-white dark:bg-slate-950 rounded-2xl border border-slate-200 dark:border-slate-800 shadow-sm overflow-hidden">
        <div className="px-6 py-4 border-b border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-900/50">
          <h2 className="text-lg font-semibold flex items-center text-slate-800 dark:text-slate-200">
            <Server className="mr-2 text-blue-500" size={20} />
            Complexity Routing Matrix
          </h2>
          <p className="text-sm text-slate-500 mt-1">Assign models based on the required task complexity.</p>
        </div>
        
        <div className="p-6 grid grid-cols-1 md:grid-cols-3 gap-6">
          {renderTierSetup("Fast", "Simple parsing, summarization, file ops.", "fast_provider", "fast_model")}
          {renderTierSetup("Balanced", "General chat, drafting, orchestration.", "default_provider", "default_model")}
          {renderTierSetup("Reasoning", "Complex coding, architecture, deep logic.", "reasoning_provider", "reasoning_model")}
        </div>
      </div>

      {/* Credentials */}
      <div className="bg-white dark:bg-slate-950 rounded-2xl border border-slate-200 dark:border-slate-800 shadow-sm overflow-hidden">
        <div className="px-6 py-4 border-b border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-900/50">
          <h2 className="text-lg font-semibold flex items-center text-slate-800 dark:text-slate-200">
            <Key className="mr-2 text-slate-500" size={20} />
            Provider Credentials
          </h2>
          <p className="text-sm text-slate-500 mt-1">Configure endpoints and keys for the providers used in your routing matrix.</p>
        </div>
        
        <div className="p-6 space-y-6">
          {activeProviders.includes('gemini') && (
            <div className="space-y-2 max-w-xl">
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300">Google Gemini API Key</label>
              <input type="password" value={settings.credentials.gemini_api_key || ''} onChange={e => updateCredentials('gemini_api_key', e.target.value)}
                placeholder="AIzaSy..."
                className="w-full px-4 py-2 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg focus:ring-2 focus:ring-[var(--accent)] outline-none text-sm font-mono" />
            </div>
          )}

          {activeProviders.includes('openai') && (
            <div className="space-y-2 max-w-xl">
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300">OpenAI API Key</label>
              <input type="password" value={settings.credentials.openai_api_key || ''} onChange={e => updateCredentials('openai_api_key', e.target.value)}
                placeholder="sk-..."
                className="w-full px-4 py-2 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg focus:ring-2 focus:ring-[var(--accent)] outline-none text-sm font-mono" />
            </div>
          )}

          {activeProviders.includes('anthropic') && (
            <div className="space-y-2 max-w-xl">
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300">Anthropic API Key</label>
              <input type="password" value={settings.credentials.anthropic_api_key || ''} onChange={e => updateCredentials('anthropic_api_key', e.target.value)}
                placeholder="sk-ant-..."
                className="w-full px-4 py-2 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg focus:ring-2 focus:ring-[var(--accent)] outline-none text-sm font-mono" />
            </div>
          )}

          {activeProviders.includes('z.ai') && (
            <div className="space-y-2 max-w-xl">
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300">Z.AI API Key</label>
              <input type="password" value={settings.credentials.zai_api_key || ''} onChange={e => updateCredentials('zai_api_key', e.target.value)}
                className="w-full px-4 py-2 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg focus:ring-2 focus:ring-[var(--accent)] outline-none text-sm font-mono" />
            </div>
          )}

          {activeProviders.includes('ollama') && (
            <div className="space-y-2 max-w-xl">
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300">Ollama Endpoint</label>
              <input type="text" value={settings.credentials.ollama_endpoint || ''} onChange={e => updateCredentials('ollama_endpoint', e.target.value)}
                placeholder="http://localhost:11434/v1"
                className="w-full px-4 py-2 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg focus:ring-2 focus:ring-[var(--accent)] outline-none text-sm font-mono" />
            </div>
          )}

          {activeProviders.includes('lmstudio') && (
            <div className="space-y-2 max-w-xl">
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300">LM Studio Endpoint</label>
              <input type="text" value={settings.credentials.lmstudio_endpoint || ''} onChange={e => updateCredentials('lmstudio_endpoint', e.target.value)}
                placeholder="http://localhost:1234/v1"
                className="w-full px-4 py-2 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg focus:ring-2 focus:ring-[var(--accent)] outline-none text-sm font-mono" />
            </div>
          )}
        </div>
      </div>

      {/* Advanced Settings */}
      <div className="bg-white dark:bg-slate-950 rounded-2xl border border-slate-200 dark:border-slate-800 shadow-sm overflow-hidden">
        <div className="px-6 py-4 border-b border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-900/50">
          <h2 className="text-lg font-semibold flex items-center text-slate-800 dark:text-slate-200">
            Settings
          </h2>
        </div>
        
        <div className="p-6">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div className="space-y-2">
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300">Context Window Limit</label>
              <div className="flex space-x-2">
                <input
                  type="number"
                  value={settings.context_window_limit}
                  onChange={(e) => setSettings({ ...settings, context_window_limit: Number(e.target.value) })}
                  className="w-full px-4 py-2 bg-[var(--bg-input)] text-[var(--text-primary)] border border-[var(--border-default)] rounded-lg focus:ring-2 focus:ring-[var(--accent)] outline-none"
                />
                <button
                  onClick={async () => {
                    const toastId = toast.loading('Detecting limit...');
                    try {
                      const limit: number = await invoke('detect_llm_limit', { model: settings.default_model });
                      setSettings({ ...settings, context_window_limit: limit });
                      toast.success(`Limit detected: ${limit} tokens`, { id: toastId });
                    } catch (e) {
                      toast.error('Failed to detect limit', { id: toastId });
                    }
                  }}
                  className="px-4 py-2 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 rounded-lg text-sm font-medium whitespace-nowrap"
                >
                  Auto Detect
                </button>
              </div>
            </div>
            
            <div className="space-y-2 flex flex-col justify-center">
              <label className="flex items-center space-x-3 cursor-pointer mt-1 md:mt-6">
                <input
                  type="checkbox"
                  checked={settings.auto_handoff_enabled}
                  onChange={(e) => setSettings({ ...settings, auto_handoff_enabled: e.target.checked })}
                  className="w-5 h-5 text-blue-600 rounded focus:ring-blue-500"
                />
                <div>
                  <span className="block text-sm font-medium text-slate-700 dark:text-slate-300">Auto Context Handoff</span>
                  <span className="block text-xs text-slate-500 mt-0.5">Split sessions when context reaches 80% limit</span>
                </div>
              </label>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
