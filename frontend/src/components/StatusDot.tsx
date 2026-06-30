import './StatusDot.css';

interface StatusDotProps {
  status: 'healthy' | 'degraded' | 'offline';
  size?: 'sm' | 'md';
  label?: string;
}

/** Colored health indicator dot (healthy / degraded / offline) with optional label. */
export function StatusDot({ status, size = 'sm', label }: StatusDotProps) {
  return (
    <span className={`status-dot status-dot--${status} status-dot--${size}`} title={label ?? status}>
      <span className="status-dot__inner" />
      {label && <span className="status-dot__label">{label}</span>}
    </span>
  );
}
