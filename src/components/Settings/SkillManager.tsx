import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Save, Plus, Trash2, Box, RefreshCcw } from 'lucide-react';
import clsx from 'clsx';
import * as yaml from 'js-yaml';

interface SkillMeta {
  id: string;
  name: string;
  description: string;
  version: string;
  author: string;
  permissions?: string[];
  path: string;
}

export default function SkillManager() {
  const [skills, setSkills] = useState<SkillMeta[]>([]);
  const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState<string>('');
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [isSaving, setIsSaving] = useState<boolean>(false);
  
  const selectedSkill = skills.find(s => s.id === selectedSkillId);

  const fetchSkills = async () => {
    setIsLoading(true);
    try {
      const data: any[] = await invoke('list_installed_skills');
      const formattedSkills: SkillMeta[] = data.map(item => ({
        id: item.id,
        name: item.name || item.id,
        description: item.description || '',
        version: item.version || '1.0.0',
        author: item.author || 'Unknown',
        permissions: Array.isArray(item.permissions) ? item.permissions : [],
        path: item.path,
      }));
      setSkills(formattedSkills);
    } catch (e) {
      console.error("Failed to load skills:", e);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    fetchSkills();
  }, []);

  useEffect(() => {
    if (selectedSkillId) {
      loadSkillContent(selectedSkillId);
    } else {
      setSkillContent('');
    }
  }, [selectedSkillId]);

  const loadSkillContent = async (id: string) => {
    try {
      const content: string = await invoke('read_skill_file', { skillId: id });
      setSkillContent(content);
    } catch (e) {
      console.error("Failed to load skill content:", e);
      setSkillContent('Error loading content.');
    }
  };

  const saveSkill = async () => {
    if (!selectedSkill) return;
    setIsSaving(true);
    
    // We update the frontmatter in the raw markdown
    try {
      // Very basic approach: generate new frontmatter and replace it
      const meta = {
        name: selectedSkill.name,
        description: selectedSkill.description,
        version: selectedSkill.version,
        author: selectedSkill.author,
        permissions: selectedSkill.permissions
      };
      
      const newFrontmatter = "---\n" + yaml.dump(meta) + "---";
      
      // Try to replace the existing frontmatter block
      let newContent = skillContent;
      if (skillContent.startsWith("---")) {
        const parts = skillContent.split("---");
        if (parts.length >= 3) {
          parts[1] = "\n" + yaml.dump(meta);
          newContent = parts.join("---");
        }
      } else {
        newContent = newFrontmatter + "\n\n" + skillContent;
      }
      
      await invoke('save_skill_file', { 
        scriptPath: selectedSkill.path,
        content: newContent
      });
      
      setSkillContent(newContent);
      // alert("Saved successfully!");
    } catch (e) {
      console.error("Failed to save:", e);
      alert("Failed to save skill.");
    } finally {
      setIsSaving(false);
    }
  };

  const deleteSkill = async (id: string) => {
    if (confirm("Are you sure you want to delete this skill folder? This cannot be undone.")) {
      try {
        await invoke('delete_skill_folder', { skillId: id });
        setSelectedSkillId(null);
        fetchSkills();
      } catch (e) {
        console.error("Failed to delete skill:", e);
        alert("Failed to delete skill.");
      }
    }
  };

  const updatePermission = (key: string, value: boolean) => {
    if (!selectedSkill) return;
    const newSkills = [...skills];
    const skillIndex = newSkills.findIndex(s => s.id === selectedSkill.id);
    if (skillIndex >= 0) {
      const currentPerms = selectedSkill.permissions || [];
      const newPerms = value 
        ? [...currentPerms, key] 
        : currentPerms.filter(p => p !== key);
        
      newSkills[skillIndex] = {
        ...selectedSkill,
        permissions: newPerms
      };
      setSkills(newSkills);
    }
  };

  const availablePermissions = [
    { id: 'network', label: 'Network' },
    { id: 'shell', label: 'Shell' },
    { id: 'db', label: 'Database' },
    { id: 'fs_read', label: 'File Read' },
    { id: 'fs_write', label: 'File Write' },
    { id: 'system', label: 'System Commands' }
  ];

  return (
    <div className="flex flex-col h-full bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-lg overflow-hidden">
      
      {/* Toolbar */}
      <div className="flex items-center justify-between p-4 bg-white dark:bg-slate-950 border-b border-slate-200 dark:border-slate-800">
        <div className="flex items-center space-x-3">
          <div className="p-2 bg-blue-100 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400 rounded-lg">
            <Box size={20} />
          </div>
          <div>
            <h2 className="text-lg font-semibold text-slate-800 dark:text-slate-100">Marketplace & Skills</h2>
            <p className="text-xs text-slate-500 dark:text-slate-400">Manage agent capabilities and sandbox permissions</p>
          </div>
        </div>
        
        <div className="flex items-center space-x-3">
          <button 
            onClick={fetchSkills}
            className="flex items-center px-3 py-2 text-sm font-medium text-slate-600 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700 rounded-md transition-colors"
          >
            <RefreshCcw size={14} className="mr-2" />
            Refresh
          </button>
          <button 
            className="flex items-center px-3 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors"
          >
            <Plus size={14} className="mr-2" />
            Create Skill
          </button>
        </div>
      </div>

      {/* Main Content Splitter */}
      <div className="flex flex-1 overflow-hidden">
        
        {/* Sidebar */}
        <div className="w-1/3 min-w-[250px] max-w-[350px] border-r border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-950 flex flex-col">
          <div className="p-3 border-b border-slate-100 dark:border-slate-800 bg-slate-50 dark:bg-slate-900/50">
            <input 
              type="text" 
              placeholder="Search skills..." 
              className="w-full px-3 py-1.5 text-sm bg-[var(--bg-input)] border border-[var(--border-default)] text-[var(--text-primary)] placeholder-[var(--text-muted)] rounded-md focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
            />
          </div>
          
          <div className="flex-1 overflow-y-auto p-2 space-y-1">
            {isLoading ? (
              <div className="flex justify-center p-4"><span className="text-sm text-slate-500">Loading...</span></div>
            ) : skills.length === 0 ? (
              <div className="flex justify-center p-4"><span className="text-sm text-slate-500">No skills found.</span></div>
            ) : (
              skills.map(skill => (
                <button
                  key={skill.id}
                  onClick={() => setSelectedSkillId(skill.id)}
                  className={clsx(
                    "w-full text-left p-3 rounded-lg transition-colors flex flex-col items-start border",
                    selectedSkillId === skill.id 
                      ? "bg-blue-50 dark:bg-blue-900/20 border-blue-200 dark:border-blue-800" 
                      : "bg-transparent border-transparent hover:bg-slate-100 dark:hover:bg-slate-800"
                  )}
                >
                  <span className="font-medium text-sm text-slate-800 dark:text-slate-200">{skill.name}</span>
                  <span className="text-xs text-slate-500 dark:text-slate-400 mt-1 line-clamp-1">{skill.description}</span>
                  <div className="flex space-x-2 mt-2">
                    <span className="text-[10px] px-1.5 py-0.5 rounded-sm bg-slate-200 dark:bg-slate-700 text-slate-600 dark:text-slate-300">v{skill.version}</span>
                    <span className="text-[10px] px-1.5 py-0.5 rounded-sm bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-400">installed</span>
                  </div>
                </button>
              ))
            )}
          </div>
        </div>

        {/* Editor Area */}
        <div className="flex-1 flex flex-col bg-slate-50 dark:bg-slate-900 overflow-hidden">
          {selectedSkill ? (
            <>
              {/* Info Header */}
              <div className="p-4 bg-white dark:bg-slate-950 border-b border-slate-200 dark:border-slate-800 flex-shrink-0">
                <div className="flex justify-between items-start">
                  <div>
                    <h3 className="text-xl font-bold text-slate-800 dark:text-slate-100">{selectedSkill.name}</h3>
                    <p className="text-sm text-slate-500 dark:text-slate-400 mt-1">Author: {selectedSkill.author} • Path: {selectedSkill.path}</p>
                  </div>
                  <div className="flex space-x-2">
                    <button 
                      onClick={() => deleteSkill(selectedSkill.id)}
                      className="p-2 text-red-500 hover:bg-red-50 dark:hover:bg-red-900/30 rounded-md transition-colors"
                      title="Delete Skill"
                    >
                      <Trash2 size={16} />
                    </button>
                    <button 
                      onClick={saveSkill}
                      disabled={isSaving}
                      className={clsx(
                        "flex items-center px-3 py-1.5 text-sm font-medium text-white rounded-md transition-colors",
                        isSaving ? "bg-blue-400" : "bg-blue-600 hover:bg-blue-700"
                      )}
                    >
                      <Save size={14} className="mr-2" />
                      {isSaving ? "Saving..." : "Save Config"}
                    </button>
                  </div>
                </div>

                {/* Sandbox Permissions Toggle */}
                <div className="mt-4 pt-4 border-t border-slate-100 dark:border-slate-800">
                  <h4 className="text-xs font-semibold text-slate-500 dark:text-slate-400 uppercase tracking-wider mb-3">Sandbox Permissions</h4>
                  <div className="flex flex-wrap gap-4">
                    {availablePermissions.map((perm) => (
                      <label key={perm.id} className="flex items-center space-x-2 cursor-pointer">
                        <input 
                          type="checkbox"
                          checked={(selectedSkill.permissions || []).includes(perm.id)}
                          onChange={(e) => updatePermission(perm.id, e.target.checked)}
                          className="w-4 h-4 text-blue-600 rounded border-slate-300 focus:ring-blue-500"
                        />
                        <span className="text-sm font-medium text-slate-700 dark:text-slate-300">{perm.label}</span>
                      </label>
                    ))}
                  </div>
                </div>
              </div>

              {/* Markdown Editor */}
              <div className="flex-1 overflow-hidden flex flex-col p-4">
                <div className="flex items-center justify-between mb-2">
                  <h4 className="text-xs font-semibold text-slate-500 dark:text-slate-400 uppercase tracking-wider">Instructions (SKILL.md)</h4>
                </div>
                <textarea 
                  value={skillContent}
                  onChange={(e) => setSkillContent(e.target.value)}
                  className="flex-1 w-full p-4 font-mono text-sm bg-[var(--bg-input)] border border-[var(--border-default)] text-[var(--text-primary)] rounded-lg focus:outline-none focus:ring-2 focus:ring-[var(--accent)] resize-none"
                  spellCheck="false"
                />
              </div>
            </>
          ) : (
            <div className="flex-1 flex flex-col items-center justify-center text-slate-400 dark:text-slate-500">
              <Box size={48} className="mb-4 opacity-50" />
              <p className="text-lg font-medium">Select a skill to configure</p>
              <p className="text-sm mt-1">Manage sandbox permissions and instructions.</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
