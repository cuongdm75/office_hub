
import { FileIcon, FileImage, FileText, ExternalLink } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { convertFileSrc } from '@tauri-apps/api/core';

interface FileEntry {
  name: string;
  path: string;
  isDir: boolean;
  sizeBytes?: number;
  modifiedAt?: string;
  extension?: string;
}

interface FilePreviewProps {
  file: FileEntry | null;
}

export default function FilePreview({ file }: FilePreviewProps) {
  if (!file) {
    return (
      <div className="h-full w-full flex flex-col items-center justify-center text-[var(--text-muted)] bg-[var(--bg-main)] border-b border-[var(--border-default)] transition-theme">
        <FileIcon size={48} className="mb-4 opacity-50" />
        <p>Select a file to preview</p>
      </div>
    );
  }

  const handleOpenNative = () => {
    invoke('open_file', { path: file.path });
  };

  // Safe check for extension
  const ext = typeof file.name === 'string' && file.name.includes('.') 
    ? file.name.split('.').pop()?.toLowerCase() 
    : undefined;
    
  const isImage = ext && ['jpg', 'jpeg', 'png', 'gif', 'webp'].includes(ext);
  const isText = ext && ['txt', 'md', 'json', 'yaml', 'yml', 'csv', 'ts', 'tsx', 'rs'].includes(ext);

  return (
    <div className="h-full w-full flex flex-col bg-[var(--bg-card)] relative transition-theme">
      <div className="flex items-center justify-between p-3 border-b border-[var(--border-default)] shrink-0 bg-[var(--bg-main)]">
        <div className="flex items-center space-x-2 truncate">
          {isImage ? <FileImage size={18} className="text-blue-500" /> : isText ? <FileText size={18} className="text-emerald-500" /> : <FileIcon size={18} className="text-slate-500" />}
          <span className="font-medium text-sm text-[var(--text-primary)] truncate" title={file.name}>{file.name}</span>
        </div>
        <button 
          onClick={handleOpenNative}
          className="flex items-center space-x-1 px-2 py-1 bg-[var(--bg-input)] hover:bg-[var(--bg-hover)] border border-[var(--border-default)] text-[var(--text-primary)] rounded text-xs transition-colors shadow-sm"
        >
          <ExternalLink size={12} />
          <span>Open Externally</span>
        </button>
      </div>
      
      <div className="flex-1 overflow-auto p-4 flex items-center justify-center bg-[var(--bg-app)]">
        {isImage ? (
          <img 
            src={convertFileSrc(file.path)} 
            alt={file.name} 
            className="max-w-full max-h-full object-contain rounded shadow-sm border border-slate-200 dark:border-slate-800"
          />
        ) : (
          <div className="text-center text-[var(--text-muted)] flex flex-col items-center">
            <FileIcon size={64} className="mb-4 text-slate-300 dark:text-slate-700" />
            <h3 className="text-lg font-medium text-[var(--text-primary)] mb-1">{file.name}</h3>
            <p className="text-sm">{(file.sizeBytes ? (file.sizeBytes / 1024).toFixed(1) + ' KB' : 'Unknown size')}</p>
            {file.modifiedAt && <p className="text-xs text-slate-400 mt-1">Modified: {new Date(file.modifiedAt).toLocaleString()}</p>}
            
            <button 
              onClick={handleOpenNative}
              className="mt-6 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-sm transition-colors shadow-sm font-medium"
            >
              Open in Default App
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
