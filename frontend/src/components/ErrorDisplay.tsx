import { AlertTriangle, XCircle } from 'lucide-react';
import './ErrorDisplay.css';

interface ErrorEnvelope {
  status: string;
  code: number;
  title: string;
  detail?: React.ReactNode | string | null;
  error?: string | null;
  instance?: string | null;
}

interface ErrorDisplayProps {
  error: ErrorEnvelope | Error | string;
  onRetry?: () => void;
  variant?: 'error' | 'warning';
}

/**
 * Renders an error for the user from any of the shapes the app produces: a
 * controller error envelope (status/code/title/detail), a thrown `Error`, or a
 * plain string. Optionally shows a retry button.
 */
export function ErrorDisplay({ error, onRetry, variant = 'error' }: ErrorDisplayProps) {
  let title: string;
  let detail: React.ReactNode | string | null = null;
  let code: number | null = null;
  let errorCode: string | null = null;

  if (typeof error === 'string') {
    title = error;
  } else if (error instanceof Error) {
    title = error.message;
  } else {
    title = error.title;
    detail = error.detail ?? null;
    code = error.code;
    errorCode = error.error ?? null;
  }

  const modifierClass = variant === 'warning' ? 'error-display--warning' : '';
  const iconNode = variant === 'warning' ? <AlertTriangle size={24} /> : <XCircle size={24} />;

  return (
    <div className={`error-display ${modifierClass}`}>
      <div className="error-display__icon">
        {iconNode}
      </div>
      <div className="error-display__body">
        <div className="error-display__header">
          <h3 className="error-display__title">{title}</h3>
          {!!code && <span className="error-display__code">{code}</span>}
        </div>
        {detail && <p className="error-display__detail">{detail}</p>}
        {errorCode && <p className="error-display__error-code">Error: <code>{errorCode}</code></p>}
      </div>
      {onRetry && (
        <button className="error-display__retry" onClick={onRetry}>
          Retry
        </button>
      )}
    </div>
  );
}
