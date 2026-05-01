import { useState, useEffect } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Save, Trash2, Edit3, Eye, FileText } from 'lucide-react';
import clsx from 'clsx';

interface MarkdownEditorProps {
  filename: string;
  initialContent: string;
  isNew: boolean;
  onSave: (filename: string, content: string) => Promise<void>;
  onDelete: (filename: string) => Promise<void>;
  onCancelNew?: () => void;
}

export default function MarkdownEditor({ 
  filename, 
  initialContent, 
  isNew, 
  onSave, 
  onDelete,
  onCancelNew 
}: MarkdownEditorProps) {
  const [content, setContent] = useState(initialContent);
  const [editFilename, setEditFilename] = useState(filename);
  const [isPreview, setIsPreview] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  // Sync state when props change
  useEffect(() => {
    setContent(initialContent);
    setEditFilename(filename);
    setIsPreview(!isNew); // Default to preview mode for existing files
  }, [filename, initialContent, isNew]);

  const handleSave = async () => {
    if (!editFilename.trim()) return;
    setIsSaving(true);
    try {
      await onSave(editFilename, content);
      setIsPreview(true);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="flex-1 flex flex-col bg-[var(--bg-main)] h-full overflow-hidden transition-theme">
      {/* Toolbar */}
      <div className="h-14 border-b border-[var(--border-default)] flex items-center justify-between px-4 bg-[var(--bg-sidebar)]">
        <div className="flex items-center gap-3 flex-1">
          <FileText className="text-[var(--text-muted)]" size={20} />
          {isNew ? (
            <div className="flex items-center gap-2 flex-1 max-w-sm">
              <input
                type="text"
                value={editFilename}
                onChange={(e) => setEditFilename(e.target.value)}
                placeholder="filename.md"
                className="w-full bg-[var(--bg-input)] border border-[var(--border-default)] rounded px-2 py-1 text-sm focus:outline-none focus:border-[var(--accent)] text-[var(--text-primary)]"
                autoFocus
              />
            </div>
          ) : (
            <h2 className="font-medium text-[var(--text-primary)]">{filename}</h2>
          )}
        </div>

        <div className="flex items-center gap-2">
          <div className="flex bg-[var(--bg-input)] border border-[var(--border-default)] p-0.5 rounded-lg mr-2">
            <button
              onClick={() => setIsPreview(false)}
              className={clsx(
                "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm transition-colors",
                !isPreview ? "bg-[var(--bg-card)] shadow-sm text-[var(--text-primary)]" : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
              )}
            >
              <Edit3 size={14} /> Edit
            </button>
            <button
              onClick={() => setIsPreview(true)}
              className={clsx(
                "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm transition-colors",
                isPreview ? "bg-[var(--bg-card)] shadow-sm text-[var(--text-primary)]" : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
              )}
            >
              <Eye size={14} /> Preview
            </button>
          </div>

          <button
            onClick={handleSave}
            disabled={isSaving || !editFilename.trim()}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
          >
            <Save size={16} />
            {isSaving ? 'Saving...' : 'Save'}
          </button>

          {isNew ? (
            <button
              onClick={onCancelNew}
              className="px-3 py-1.5 text-[var(--text-secondary)] hover:text-[var(--text-primary)] text-sm font-medium transition-colors"
            >
              Cancel
            </button>
          ) : (
            <button
              onClick={() => {
                if (window.confirm(`Are you sure you want to delete ${filename}?`)) {
                  onDelete(filename);
                }
              }}
              className="p-1.5 text-[var(--text-muted)] hover:text-red-500 hover:bg-red-500/10 rounded-lg transition-colors"
              title="Delete File"
            >
              <Trash2 size={18} />
            </button>
          )}
        </div>
      </div>

      {/* Editor / Preview Area */}
      <div className="flex-1 overflow-hidden relative">
        {isPreview ? (
          <div className="absolute inset-0 overflow-y-auto p-8 bg-[var(--bg-main)]">
            <div className="max-w-3xl mx-auto prose dark:prose-invert" style={{ color: 'var(--text-primary)' }}>
              <ReactMarkdown remarkPlugins={[remarkGfm]}>
                {content || '*No content*'}
              </ReactMarkdown>
            </div>
          </div>
        ) : (
          <textarea
            value={content}
            onChange={(e) => setContent(e.target.value)}
            className="w-full h-full p-6 bg-[var(--bg-app)] text-[var(--text-primary)] border-none outline-none focus:ring-0 resize-none font-mono text-sm leading-relaxed"
            placeholder="Write your markdown content here..."
            spellCheck="false"
          />
        )}
      </div>
    </div>
  );
}
