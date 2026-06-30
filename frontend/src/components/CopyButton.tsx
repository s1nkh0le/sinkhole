import { useState } from 'react';
import { Copy, Check } from 'lucide-react';

/** Ghost icon button that copies `text` to the clipboard and flips to a check for 2s. */
export function CopyButton({ text, className = '' }: { text: string; className?: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = (e: React.MouseEvent) => {
    e.stopPropagation();
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <button 
      onClick={handleCopy} 
      className={`btn btn--ghost btn--icon ${className}`} 
      style={{ padding: '2px', height: 'auto', opacity: 0.5, transition: 'opacity 0.2s' }}
      title="Copy to clipboard"
      onMouseEnter={e => (e.currentTarget.style.opacity = '1')}
      onMouseLeave={e => (e.currentTarget.style.opacity = '0.5')}
    >
      {copied ? <Check size={14} className="text-success" /> : <Copy size={14} />}
    </button>
  );
}
