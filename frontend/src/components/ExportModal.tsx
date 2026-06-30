import { useState } from 'react';
import { Modal } from './Modal';
import { Download, Terminal, Copy, Check } from 'lucide-react';
import { decodeBase64, bytesToText } from '../lib/bytes';
import type { RecordGroup } from '../api/types';

/**
 * Export a captured connection (a RecordGroup) as a ready-to-run pwntools
 * Python script that replays its inbound traffic against the target, optionally
 * annotated with per-message comments. Handy for turning observed traffic into a
 * reproducible exploit/test.
 */
export function ExportModal({
  group,
  open,
  onClose
}: {
  group: RecordGroup | null;
  open: boolean;
  onClose: () => void;
}) {
  const [copiedScript, setCopiedScript] = useState<string | null>(null);
  const [annotate, setAnnotate] = useState(false);

  if (!group) return null;

  // Generate raw content
  const handleDownloadRaw = () => {
    let combined: Uint8Array;
    
    if (annotate) {
      const parts: Uint8Array[] = [];
      const encoder = new TextEncoder();
      
      for (const r of group.records) {
        if (!r.raw_b64) continue;
        const prefix = encoder.encode(r.inbound ? "> " : "< ");
        parts.push(prefix);
        parts.push(decodeBase64(r.raw_b64));
      }
      
      const totalLength = parts.reduce((acc, b) => acc + b.length, 0);
      combined = new Uint8Array(totalLength);
      let offset = 0;
      for (const p of parts) {
        combined.set(p, offset);
        offset += p.length;
      }
    } else {
      const buffers = group.records
        .filter(r => r.raw_b64)
        .map(r => decodeBase64(r.raw_b64!));
      
      const totalLength = buffers.reduce((acc, b) => acc + b.length, 0);
      combined = new Uint8Array(totalLength);
      let offset = 0;
      for (const b of buffers) {
        combined.set(b, offset);
        offset += b.length;
      }
    }

    const blob = new Blob([combined as unknown as BlobPart], { type: 'application/octet-stream' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = annotate ? `connection_${group.conn_id}_annotated.bin` : `connection_${group.conn_id}_raw.bin`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  /**
   * Generates a ready-to-run Python script using the 'pwntools' library.
   * This script allows the user to replay the traffic connection locally.
   * 
   * It maps inbound messages to `c.send(...)` commands and generates commented-out
   * `c.recvuntil(...)` assertions for outbound messages, allowing the user to
   * precisely synchronize the script with expected server responses.
   */
  const generatePwntoolsScript = () => {
    let script = `#!/usr/bin/env python3\nfrom pwn import *\n\ncontext.terminal = ['tmux', 'splitw', '-h']\n\n# Target config (update as needed)\nHOST = "127.0.0.1"\nPORT = ${group.port}\n\n# Set ssl=True if the challenge uses TLS\nc = remote(HOST, PORT, ssl=False)\n\n`;
    
    group.records.forEach(rec => {
      if (!rec.raw_b64) return;
      
      const text = bytesToText(decodeBase64(rec.raw_b64));
      // Escape for python literal
      const escaped = text
        .replace(/\\/g, '\\\\')
        .replace(/"/g, '\\"')
        .replace(/\n/g, '\\n')
        .replace(/\r/g, '\\r')
        .replace(/\t/g, '\\t');
      
      if (rec.inbound) {
        script += `# Message ${rec.message_seq} (INBOUND)\nc.send(b"${escaped}")\n\n`;
      } else {
        script += `# Message ${rec.message_seq} (OUTBOUND)\n# Expected output:\n# c.recvuntil(b"${escaped}")\n\n`;
      }
    });
    
    script += `c.interactive()\nc.close()\n`;
    return script;
  };

  const handleCopyScript = (scriptStr: string, id: string) => {
    navigator.clipboard.writeText(scriptStr);
    setCopiedScript(id);
    setTimeout(() => setCopiedScript(null), 2000);
  };

  return (
    <Modal open={open} onClose={onClose} title={`Export Connection #${group.conn_id}`} size="lg">
      <div style={{ display: 'grid', gap: 'var(--space-4)', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))' }}>
        
        {/* Raw Binary Download */}
        <div className="card" style={{ padding: 'var(--space-4)', display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)' }}>
            <Download size={20} className="text-info" />
            <h3 style={{ margin: 0, fontSize: 'var(--text-md)' }}>Raw Bin</h3>
          </div>
          <p className="text-muted" style={{ fontSize: 'var(--text-sm)', margin: 0, flex: 1 }}>
            Download the raw bidirectional bytes of this connection as a single binary file.
          </p>
          <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)', marginBottom: 'var(--space-2)' }}>
            <input 
              type="checkbox" 
              id="annotate-bin" 
              checked={annotate} 
              onChange={e => setAnnotate(e.target.checked)} 
              style={{ cursor: 'pointer' }}
            />
            <label htmlFor="annotate-bin" style={{ fontSize: 'var(--text-sm)', cursor: 'pointer', userSelect: 'none' }}>
              Annotate with `&gt; ` and `&lt; `
            </label>
          </div>
          <button className="btn btn--secondary w-full" onClick={handleDownloadRaw}>
            Download .bin
          </button>
        </div>

        {/* Pwntools Script */}
        <div className="card" style={{ padding: 'var(--space-4)', display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)' }}>
            <Terminal size={20} className="text-warning" />
            <h3 style={{ margin: 0, fontSize: 'var(--text-md)' }}>Pwntools Script</h3>
          </div>
          <p className="text-muted" style={{ fontSize: 'var(--text-sm)', margin: 0, flex: 1 }}>
            Copy a Python script using pwntools that replays all the inbound traffic from this connection.
          </p>
          <button 
            className="btn btn--secondary w-full" 
            onClick={() => handleCopyScript(generatePwntoolsScript(), 'pwntools')}
          >
            {copiedScript === 'pwntools' ? <><Check size={14}/> Copied!</> : <><Copy size={14}/> Copy Script</>}
          </button>
        </div>

      </div>
    </Modal>
  );
}
