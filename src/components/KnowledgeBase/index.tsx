import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import toast from 'react-hot-toast';
import FileList, { KnowledgeFile, KnowledgeCategory } from './FileList';
import MarkdownEditor from './MarkdownEditor';
import { BookOpen } from 'lucide-react';

export default function KnowledgeBase() {
  const [activeCategory, setActiveCategory] = useState<KnowledgeCategory>('knowledge');
  const [files, setFiles] = useState<KnowledgeFile[]>([]);
  const [activeFile, setActiveFile] = useState<string | null>(null);
  const [fileContent, setFileContent] = useState<string>('');
  const [isNew, setIsNew] = useState(false);
  const [isLoading, setIsLoading] = useState(true);

  // ── Fetch files for current category ────────────────────────────────────
  const fetchFiles = useCallback(async (cat: KnowledgeCategory) => {
    setIsLoading(true);
    try {
      const data = await invoke<KnowledgeFile[]>('list_knowledge', { category: cat });
      setFiles(data);
    } catch (error) {
      console.error('Failed to list knowledge files:', error);
      toast.error('Failed to load ' + cat);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchFiles(activeCategory);
    // Reset selection when switching category
    setActiveFile(null);
    setFileContent('');
    setIsNew(false);
  }, [activeCategory, fetchFiles]);

  // ── Category change ──────────────────────────────────────────────────────
  const handleCategoryChange = (cat: KnowledgeCategory) => {
    setActiveCategory(cat);
  };

  // ── File select ──────────────────────────────────────────────────────────
  const handleSelectFile = async (filename: string) => {
    try {
      const content = await invoke<string>('read_knowledge_file', {
        filename,
        category: activeCategory,
      });
      setFileContent(content);
      setActiveFile(filename);
      setIsNew(false);
    } catch (error) {
      console.error(`Failed to read ${filename}:`, error);
      toast.error(`Failed to open ${filename}`);
    }
  };

  const handleNewFile = () => {
    setActiveFile('');
    setFileContent(getDefaultContent(activeCategory));
    setIsNew(true);
  };

  const handleCancelNew = () => {
    setIsNew(false);
    if (files.length > 0 && files[0]) {
      handleSelectFile(files[0].filename);
    } else {
      setActiveFile(null);
    }
  };

  // ── Save ─────────────────────────────────────────────────────────────────
  const handleSave = async (filename: string, content: string) => {
    try {
      await invoke('save_knowledge_file', {
        filename,
        content,
        category: activeCategory,
      });
      toast.success('Saved successfully');
      await fetchFiles(activeCategory);

      const finalFilename = filename.endsWith('.md') ? filename : `${filename}.md`;
      setActiveFile(finalFilename);
      setIsNew(false);
    } catch (error) {
      console.error('Failed to save file:', error);
      toast.error('Failed to save file');
      throw error;
    }
  };

  // ── Delete ───────────────────────────────────────────────────────────────
  const handleDelete = async (filename: string) => {
    try {
      await invoke('delete_knowledge_file', { filename, category: activeCategory });
      toast.success('File deleted');
      await fetchFiles(activeCategory);

      if (activeFile === filename) {
        setIsNew(false);
        const remaining = files.filter((f) => f.filename !== filename);
        if (remaining.length > 0 && remaining[0]) {
          handleSelectFile(remaining[0].filename);
        } else {
          setActiveFile(null);
          setFileContent('');
        }
      }
    } catch (error) {
      console.error('Failed to delete file:', error);
      toast.error('Failed to delete file');
    }
  };

  return (
    <div className="flex h-full w-full bg-[var(--bg-main)] overflow-hidden transition-theme">
      <FileList
        files={files}
        activeFile={activeFile}
        activeCategory={activeCategory}
        onSelectFile={handleSelectFile}
        onNewFile={handleNewFile}
        onCategoryChange={handleCategoryChange}
      />

      <div className="flex-1 flex flex-col relative h-full">
        {isLoading ? (
          <div className="flex items-center justify-center h-full text-slate-500">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600" />
          </div>
        ) : activeFile !== null || isNew ? (
          <MarkdownEditor
            filename={activeFile || ''}
            initialContent={fileContent}
            isNew={isNew}
            onSave={handleSave}
            onDelete={handleDelete}
            onCancelNew={handleCancelNew}
          />
        ) : (
          <EmptyState category={activeCategory} onNewFile={handleNewFile} />
        )}
      </div>
    </div>
  );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function getDefaultContent(category: KnowledgeCategory): string {
  switch (category) {
    case 'policy':
      return `# Policy Name\n\n## Mục tiêu\n\n_Mô tả mục tiêu của policy này._\n\n## Quy tắc\n\n1. Quy tắc 1\n2. Quy tắc 2\n\n## Ngoại lệ\n\n_Liệt kê các ngoại lệ nếu có._\n`;
    case 'skills':
      return `---\nname: skill-name\ndescription: Mô tả ngắn gọn skill này làm gì\n---\n\n# Hướng dẫn thực hiện\n\n## Bước 1\n\n_Mô tả bước 1_\n\n## Bước 2\n\n_Mô tả bước 2_\n`;
    default:
      return `# Tiêu đề\n\n_Nội dung tri thức..._\n`;
  }
}

function EmptyState({ category, onNewFile }: { category: KnowledgeCategory; onNewFile: () => void }) {
  const labels: Record<KnowledgeCategory, { title: string; desc: string }> = {
    knowledge: {
      title: 'Knowledge Base',
      desc: 'Lưu trữ tài liệu, facts, thông tin nội bộ. LLM sẽ truy cập qua tool `read_knowledge`.',
    },
    policy: {
      title: 'Policy',
      desc: 'Định nghĩa quy tắc hành vi cho LLM — format, ngôn ngữ, giới hạn, ưu tiên. LLM đọc qua tool `query_policy`.',
    },
    skills: {
      title: 'Skills',
      desc: 'Kỹ năng và workflows cho Agent. Mỗi skill là một file SKILL.md với YAML frontmatter.',
    },
  };

  const { title, desc } = labels[category];

  return (
    <div className="flex flex-col items-center justify-center h-full text-slate-400 px-8">
      <BookOpen size={48} className="mb-4 opacity-20" />
      <h3 className="text-xl font-medium text-slate-600 dark:text-slate-300 mb-2">{title}</h3>
      <p className="text-slate-500 max-w-sm text-center text-sm leading-relaxed">{desc}</p>
      <button
        onClick={onNewFile}
        className="mt-6 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-sm font-medium transition-colors shadow-sm"
      >
        + Create New File
      </button>
    </div>
  );
}
