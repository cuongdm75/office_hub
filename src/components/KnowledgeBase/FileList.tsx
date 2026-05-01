import { FileText, Plus, FileQuestion, BookOpen, Shield, Zap, Star } from 'lucide-react';
import clsx from 'clsx';

export interface KnowledgeFile {
  filename: string;
  path: string;
  modified_at: number;
  category: string;
}

export type KnowledgeCategory = 'knowledge' | 'policy' | 'skills';

interface FileListProps {
  files: KnowledgeFile[];
  activeFile: string | null;
  activeCategory: KnowledgeCategory;
  onSelectFile: (filename: string) => void;
  onNewFile: () => void;
  onCategoryChange: (cat: KnowledgeCategory) => void;
}

const CATEGORIES: { id: KnowledgeCategory; label: string; icon: React.FC<any>; description: string }[] = [
  {
    id: 'knowledge',
    label: 'Knowledge',
    icon: BookOpen,
    description: 'Tài liệu tri thức, facts nội bộ',
  },
  {
    id: 'policy',
    label: 'Policy',
    icon: Shield,
    description: 'Quy tắc, quy định hành vi LLM',
  },
  {
    id: 'skills',
    label: 'Skills',
    icon: Zap,
    description: 'Kỹ năng, workflows cho Agent',
  },
];

export default function FileList({
  files,
  activeFile,
  activeCategory,
  onSelectFile,
  onNewFile,
  onCategoryChange,
}: FileListProps) {
  return (
    <div className="w-64 border-r border-[var(--border-default)] bg-[var(--bg-sidebar)] flex flex-col h-full transition-theme">
      {/* Header */}
      <div className="p-4 border-b border-[var(--border-default)]">
        <h2 className="font-semibold text-[var(--text-primary)] mb-3">Knowledge Manager</h2>

        {/* Category Tabs */}
        <div className="flex flex-col gap-1">
          {CATEGORIES.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              onClick={() => onCategoryChange(id)}
              className={clsx(
                'flex items-center gap-2 px-3 py-2 rounded-lg text-sm font-medium transition-all text-left',
                activeCategory === id
                  ? 'bg-[var(--accent)] text-white shadow-sm'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
              )}
            >
              <Icon size={15} />
              {label}
              {activeCategory === id && (
                <span className="ml-auto text-xs bg-white/20 px-1.5 py-0.5 rounded-full text-white">
                  {files.length}
                </span>
              )}
            </button>
          ))}
        </div>
      </div>

      {/* File list header */}
      <div className="px-4 py-2 border-b border-[var(--border-default)] flex items-center justify-between">
        <span className="text-xs font-semibold text-[var(--text-muted)] uppercase tracking-wider">
          Files
        </span>
        <button
          onClick={onNewFile}
          className="p-1 hover:bg-[var(--bg-hover)] rounded-md text-[var(--text-secondary)] transition-colors"
          title="New File"
        >
          <Plus size={16} />
        </button>
      </div>

      {/* Files */}
      <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
        {files.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-[var(--text-muted)] text-sm">
            <FileQuestion size={24} className="mb-2 opacity-50" />
            <p>No files yet.</p>
            <button
              onClick={onNewFile}
              className="mt-2 text-xs text-[var(--accent)] hover:text-[var(--accent-hover)] font-medium"
            >
              Create one →
            </button>
          </div>
        ) : (
          files.map((file) => {
            const isActive = activeFile === file.filename;
            const isIndex = file.filename === 'index.md';
            const isSkillFolder = file.filename.includes('/');
            const displayName = isSkillFolder
              ? file.filename.split('/')[0]
              : file.filename.replace('.md', '');

            return (
              <button
                key={file.filename}
                onClick={() => onSelectFile(file.filename)}
                className={clsx(
                  'w-full text-left px-3 py-2 flex items-center gap-2.5 rounded-lg transition-all text-sm',
                  isActive
                    ? 'bg-[var(--accent-subtle)] text-[var(--accent)] font-medium'
                    : 'hover:bg-[var(--bg-hover)] text-[var(--text-primary)]'
                )}
              >
                <FileText
                  size={14}
                  className={clsx(
                    'shrink-0',
                    isActive ? 'text-[var(--accent)]' : 'text-[var(--text-muted)]'
                  )}
                />
                <div className="flex-1 truncate">{displayName}</div>
                {isIndex && (
                  <Star size={12} className="shrink-0 text-amber-400" />
                )}
              </button>
            );
          })
        )}
      </div>

      {/* LLM Access note */}
      <div className="p-3 border-t border-[var(--border-default)]">
        <p className="text-[10px] text-[var(--text-muted)] leading-relaxed">
          LLM truy cập tất cả files trong category này qua MCP khi xử lý yêu cầu.
        </p>
      </div>
    </div>
  );
}
