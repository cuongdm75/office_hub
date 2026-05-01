import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Save, RefreshCw, MonitorPlay, Power, AppWindow } from 'lucide-react';
import toast from 'react-hot-toast';

export interface SystemConfig {
  startupWithWindows: boolean;
  minimiseToTray: boolean;
  suppressSleepDuringTasks: boolean;
  agentsActiveOnLockscreen: boolean;
  websocketPort: number;
  useTailscale: boolean;
}

export default function SystemTab() {
  const [config, setConfig] = useState<SystemConfig | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [isInstallingAddin, setIsInstallingAddin] = useState(false);

  useEffect(() => {
    loadConfig();
  }, []);

  const loadConfig = async () => {
    try {
      setIsLoading(true);
      const data: SystemConfig = await invoke('get_system_config');
      setConfig(data);
    } catch (error) {
      console.error('Failed to load system config:', error);
      toast.error('Failed to load system configuration');
    } finally {
      setIsLoading(false);
    }
  };

  const handleSave = async () => {
    if (!config) return;
    try {
      setIsSaving(true);
      
      // Save config to backend
      await invoke('save_system_config', { config });
      
      // Toggle startup registration
      await invoke('toggle_startup', { enable: config.startupWithWindows });
      
      toast.success('System settings saved successfully');
    } catch (error) {
      console.error('Failed to save system config:', error);
      toast.error('Failed to save system settings');
    } finally {
      setIsSaving(false);
    }
  };

  const handleInstallAddin = async () => {
    try {
      setIsInstallingAddin(true);
      toast.loading('Đang cài đặt Office Add-in (Vui lòng xác nhận quyền Admin nếu có)...', { id: 'install_addin' });
      await invoke('install_office_addin');
      toast.success('Cài đặt Office Add-in thành công!', { id: 'install_addin' });
    } catch (error) {
      console.error('Failed to install addin:', error);
      toast.error(`Lỗi cài đặt: ${error}`, { id: 'install_addin', duration: 5000 });
    } finally {
      setIsInstallingAddin(false);
    }
  };

  if (isLoading || !config) {
    return (
      <div className="flex items-center justify-center h-full min-h-[400px]">
        <RefreshCw className="animate-spin text-slate-400" size={24} />
      </div>
    );
  }

  return (
    <div className="space-y-8 animate-in fade-in duration-300">
      <div className="flex justify-between items-center mb-6">
        <div>
          <h2 className="text-xl font-bold text-slate-800 dark:text-slate-100">System Preferences</h2>
          <p className="text-slate-500 dark:text-slate-400 mt-1">Manage application behavior and Windows integrations.</p>
        </div>
        <button
          onClick={handleSave}
          disabled={isSaving}
          className="flex items-center px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition-colors disabled:opacity-50 shadow-sm"
        >
          {isSaving ? <RefreshCw className="animate-spin mr-2" size={18} /> : <Save className="mr-2" size={18} />}
          Save System Changes
        </button>
      </div>

      <div className="bg-white dark:bg-slate-950 rounded-2xl border border-slate-200 dark:border-slate-800 shadow-sm overflow-hidden">
        <div className="px-6 py-4 border-b border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-900/50">
          <h2 className="text-lg font-semibold flex items-center text-slate-800 dark:text-slate-200">
            <MonitorPlay className="mr-2 text-slate-500" size={20} />
            Window & Startup
          </h2>
        </div>
        
        <div className="p-6 space-y-6">
          <label className="flex items-start space-x-3 cursor-pointer">
            <input 
              type="checkbox" 
              checked={config.startupWithWindows}
              onChange={(e) => setConfig({ ...config, startupWithWindows: e.target.checked })}
              className="mt-1 w-4 h-4 text-blue-600 bg-[var(--bg-input)] border-[var(--border-default)] rounded focus:ring-[var(--accent)]" 
            />
            <div>
              <p className="font-medium text-slate-800 dark:text-slate-200">Start with Windows</p>
              <p className="text-sm text-slate-500 dark:text-slate-400">Launch Office Hub automatically when you sign in.</p>
            </div>
          </label>

          <label className="flex items-start space-x-3 cursor-pointer">
            <input 
              type="checkbox" 
              checked={config.minimiseToTray}
              onChange={(e) => setConfig({ ...config, minimiseToTray: e.target.checked })}
              className="mt-1 w-4 h-4 text-blue-600 bg-[var(--bg-input)] border-[var(--border-default)] rounded focus:ring-[var(--accent)]" 
            />
            <div>
              <p className="font-medium text-slate-800 dark:text-slate-200">Minimise to Tray</p>
              <p className="text-sm text-slate-500 dark:text-slate-400">Closing the window hides the app to the system tray instead of exiting.</p>
            </div>
          </label>
        </div>
      </div>

      <div className="bg-white dark:bg-slate-950 rounded-2xl border border-slate-200 dark:border-slate-800 shadow-sm overflow-hidden">
        <div className="px-6 py-4 border-b border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-900/50">
          <h2 className="text-lg font-semibold flex items-center text-slate-800 dark:text-slate-200">
            <Power className="mr-2 text-slate-500" size={20} />
            Power Management
          </h2>
        </div>
        
        <div className="p-6 space-y-6">
          <label className="flex items-start space-x-3 cursor-pointer">
            <input 
              type="checkbox" 
              checked={config.suppressSleepDuringTasks}
              onChange={(e) => setConfig({ ...config, suppressSleepDuringTasks: e.target.checked })}
              className="mt-1 w-4 h-4 text-blue-600 bg-[var(--bg-input)] border-[var(--border-default)] rounded focus:ring-[var(--accent)]" 
            />
            <div>
              <p className="font-medium text-slate-800 dark:text-slate-200">Suppress Sleep During Tasks</p>
              <p className="text-sm text-slate-500 dark:text-slate-400">Prevent Windows from going to sleep while AI agents are running workflows.</p>
            </div>
          </label>

          <label className="flex items-start space-x-3 cursor-pointer">
            <input 
              type="checkbox" 
              checked={config.agentsActiveOnLockscreen}
              onChange={(e) => setConfig({ ...config, agentsActiveOnLockscreen: e.target.checked })}
              className="mt-1 w-4 h-4 text-blue-600 bg-[var(--bg-input)] border-[var(--border-default)] rounded focus:ring-[var(--accent)]" 
            />
            <div>
              <p className="font-medium text-slate-800 dark:text-slate-200">Active on Lockscreen</p>
              <p className="text-sm text-slate-500 dark:text-slate-400">Allow agents to continue working even when Windows is locked.</p>
            </div>
          </label>
        </div>
      </div>
      
      <div className="bg-white dark:bg-slate-950 rounded-2xl border border-slate-200 dark:border-slate-800 shadow-sm overflow-hidden">
        <div className="px-6 py-4 border-b border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-900/50">
          <h2 className="text-lg font-semibold flex items-center text-slate-800 dark:text-slate-200">
            <AppWindow className="mr-2 text-slate-500" size={20} />
            Office Add-in Integration
          </h2>
        </div>
        
        <div className="p-6">
          <p className="text-sm text-slate-500 dark:text-slate-400 mb-4">
            Cài đặt Office Hub Add-in vào Word, Excel và PowerPoint. Quá trình này sẽ đăng ký chứng chỉ bảo mật cho localhost và có thể yêu cầu quyền Administrator.
          </p>
          <button
            onClick={handleInstallAddin}
            disabled={isInstallingAddin}
            className="flex items-center px-4 py-2 bg-slate-800 dark:bg-slate-200 text-white dark:text-slate-900 hover:bg-slate-700 dark:hover:bg-white rounded-lg font-medium transition-colors disabled:opacity-50"
          >
            {isInstallingAddin ? <RefreshCw className="animate-spin mr-2" size={18} /> : <AppWindow className="mr-2" size={18} />}
            {isInstallingAddin ? 'Đang cài đặt...' : 'Cài đặt Office Add-in'}
          </button>
        </div>
      </div>

    </div>
  );
}
