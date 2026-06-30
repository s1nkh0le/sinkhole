import React, { useState, useEffect } from 'react';
import { Activity, CheckCircle, XCircle } from 'lucide-react';

export interface ConnectionConfigFormProps {
  initialUrl: string;
  initialToken?: string;
  onApply: (url: string, token: string) => void;
  onCancel?: () => void;
  applyLabel?: string;
  loading?: boolean;
  preventSameUrl?: boolean;
}

/**
 * Controller URL + token form with live verification. As the user types
 * (debounced), it calls `/auth/ping` and reports whether the endpoint is a
 * reachable controller, a filter (wrong node type), or unauthorized. `onApply`
 * is only enabled once verification succeeds. Used by both the login page and
 * the settings page; `preventSameUrl` blocks re-entering the current controller.
 */
export function ConnectionConfigForm({
  initialUrl,
  initialToken = '',
  onApply,
  onCancel,
  applyLabel = 'Apply',
  loading = false,
  preventSameUrl = false,
}: ConnectionConfigFormProps) {
  const [url, setUrl] = useState(initialUrl);
  const [token, setToken] = useState(initialToken);
  const [status, setStatus] = useState<'idle' | 'checking' | 'ok' | 'error'>('idle');
  const [statusMsg, setStatusMsg] = useState('');
  const [controllerName, setControllerName] = useState('');

  useEffect(() => {
    let active = true;
    
    const checkAuth = async (isInterval = false) => {
      const trimmedUrl = url.trim().replace(/\/+$/, '');
      const trimmedToken = token.trim();
      
      if (!trimmedUrl || !trimmedToken) {
        if (active) {
          setStatus('idle');
          setStatusMsg('');
          setControllerName('');
        }
        return;
      }

      if (preventSameUrl && trimmedUrl === initialUrl.trim().replace(/\/+$/, '')) {
        if (active) {
          setStatus('error');
          setStatusMsg('This is already your current controller URL.');
          setControllerName('');
        }
        return;
      }
      
      if (active && !isInterval) {
        setStatus('checking');
      }
      
      try {
        const res = await fetch(`${trimmedUrl}/auth/ping`, {
          method: 'GET',
          headers: { 'Authorization': `Bearer ${trimmedToken}` }
        });
        
        if (!active) return;
        
        if (!res.ok) {
          setStatus('error');
          if (res.status === 401 || res.status === 403) {
            setStatusMsg('Invalid API Token.');
          } else {
            setStatusMsg(`Server returned HTTP ${res.status}`);
          }
          setControllerName('');
          return;
        }
        
        const data = await res.json();
        
        if (data.data?.instance === 'filter') {
          setStatus('error');
          setStatusMsg('This is a filter node. Please connect to a controller.');
          setControllerName('');
        } else if (data.data?.instance === 'controller') {
          setStatus('ok');
          setStatusMsg('Verified controller');
          setControllerName(data.data?.name || '');
        } else {
          setStatus('error');
          setStatusMsg('Outdated or unknown instance type. Please upgrade the node.');
          setControllerName('');
        }
        
      } catch (err) {
        if (!active) return;
        setStatus('error');
        const errString = String(err);
        if (err instanceof TypeError || errString.includes('NetworkError') || errString.includes('Failed to fetch')) {
          setStatusMsg('Could not reach controller. Check URL and connectivity.');
        } else {
          setStatusMsg(`Error: ${err instanceof Error ? err.message : 'Unknown'}`);
        }
        setControllerName('');
      }
    };

    const timer = setTimeout(() => {
      checkAuth(false);
    }, 500);

    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [url, token, initialUrl, preventSameUrl]); // Re-run effect immediately when url or token changes

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (status === 'ok') {
      onApply(url.trim().replace(/\/+$/, ''), token.trim());
    }
  };

  return (
    <form className="form-stack" onSubmit={handleSubmit}>
      <div className="form-field">
        <label className="form-label">Controller URL</label>
        <input
          type="url"
          value={url}
          onChange={e => setUrl(e.target.value)}
          placeholder="https://controller.example.com"
          autoFocus
          disabled={loading}
          style={{ width: '100%', padding: 'var(--space-2)' }}
        />
      </div>

      <div className="form-field">
        <label className="form-label">API Token</label>
        <input
          type="password"
          value={token}
          onChange={e => setToken(e.target.value)}
          placeholder="Enter API token"
          disabled={loading}
          style={{ width: '100%', padding: 'var(--space-2)' }}
        />
      </div>

      <div style={{ 
        display: 'flex', 
        alignItems: 'center', 
        gap: 'var(--space-2)',
        padding: 'var(--space-2)',
        borderRadius: 'var(--radius-sm)',
        background: status === 'ok' ? 'var(--color-success-transparent, rgba(34, 197, 94, 0.1))' : 
                    status === 'error' ? 'var(--color-danger-transparent, rgba(239, 68, 68, 0.1))' : 
                    status === 'checking' ? 'var(--color-primary-transparent, rgba(59, 130, 246, 0.1))' : 'transparent',
        border: `1px solid ${
          status === 'ok' ? 'var(--color-success, #22c55e)' : 
          status === 'error' ? 'var(--color-danger, #ef4444)' : 
          status === 'checking' ? 'var(--color-primary, #3b82f6)' : 'transparent'
        }`
      }}>
        {status === 'idle' && <span style={{ fontSize: 'var(--text-sm)', color: 'var(--color-text-tertiary)' }}>Enter URL and Token to verify</span>}
        {status === 'checking' && <><Activity size={16} style={{ color: 'var(--color-primary)' }} /><span style={{ fontSize: 'var(--text-sm)', color: 'var(--color-primary)' }}>Checking connection...</span></>}
        {status === 'ok' && <><CheckCircle size={16} style={{ color: 'var(--color-success)' }} /><span style={{ fontSize: 'var(--text-sm)', color: 'var(--color-success)' }}>{statusMsg} {controllerName && `(${controllerName})`}</span></>}
        {status === 'error' && <><XCircle size={16} style={{ color: 'var(--color-danger)' }} /><span style={{ fontSize: 'var(--text-sm)', color: 'var(--color-danger)' }}>{statusMsg}</span></>}
      </div>

      <div className="form-actions" style={{ marginTop: 'var(--space-4)' }}>
        {onCancel && (
          <button type="button" className="btn btn--ghost" onClick={onCancel} disabled={loading}>
            Cancel
          </button>
        )}
        <button 
          type="submit" 
          className="btn btn--primary" 
          disabled={loading || status !== 'ok'}
        >
          {loading ? 'Processing…' : applyLabel}
        </button>
      </div>
    </form>
  );
}
