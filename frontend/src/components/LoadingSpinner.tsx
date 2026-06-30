import './LoadingSpinner.css';

interface LoadingSpinnerProps {
  size?: 'sm' | 'md' | 'lg';
  label?: string;
}

/** Indeterminate loading spinner with an optional label. */
export function LoadingSpinner({ size = 'md', label }: LoadingSpinnerProps) {
  return (
    <div className={`loading-spinner loading-spinner--${size}`}>
      <div className="loading-spinner__ring" />
      {label && <span className="loading-spinner__label">{label}</span>}
    </div>
  );
}
