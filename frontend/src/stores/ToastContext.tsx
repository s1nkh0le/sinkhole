/* eslint-disable react-refresh/only-export-components */
import React, { createContext, useContext, useState, useCallback, useRef } from 'react';

export interface Toast {
  id: string;
  message: string;
  variant: 'success' | 'error' | 'info' | 'warning';
}

interface ToastContextValue {
  toasts: Toast[];
  showToast: (message: string, variant?: Toast['variant']) => void;
  dismissToast: (id: string) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

/**
 * Provides transient toast notifications and renders them. Call
 * {@link useToast}'s `showToast(message, variant)` from anywhere under the
 * provider; toasts auto-dismiss after 25s or when closed.
 */
export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const counterRef = useRef(0);

  const showToast = useCallback((message: string, variant: Toast['variant'] = 'success') => {
    const id = `toast_${++counterRef.current}`;
    setToasts(prev => [...prev, { id, message, variant }]);
    // Auto-dismiss after 25s
    setTimeout(() => {
      setToasts(prev => prev.filter(t => t.id !== id));
    }, 25000);
  }, []);

  const dismissToast = useCallback((id: string) => {
    setToasts(prev => prev.filter(t => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ toasts, showToast, dismissToast }}>
      {children}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </ToastContext.Provider>
  );
}

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error('useToast must be used within ToastProvider');
  return ctx;
}

/* ---- Renderer ---- */

function ToastContainer({ toasts, onDismiss }: { toasts: Toast[]; onDismiss: (id: string) => void }) {
  if (toasts.length === 0) return null;

  const getIcon = (variant: Toast['variant']) => {
    switch (variant) {
      case 'success': return '✓';
      case 'error': return '✕';
      case 'warning': return '⚠';
      case 'info': return 'ℹ';
    }
  };

  return (
    <div className="toast-container">
      {toasts.map(toast => (
        <div
          key={toast.id}
          className={`toast toast--${toast.variant}`}
          role="alert"
        >
          <span className="toast__icon">
            {getIcon(toast.variant)}
          </span>
          <span className="toast__message">{toast.message}</span>
          <button className="toast__dismiss" onClick={() => onDismiss(toast.id)}>×</button>
        </div>
      ))}
    </div>
  );
}
