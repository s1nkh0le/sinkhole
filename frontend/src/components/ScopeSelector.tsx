import type { Scope } from '../api/types';
import { SCOPE_LABELS } from '../lib/legality';
import './ScopeSelector.css';

const ALL_SCOPES: Scope[] = [
  'stream', 'http.path', 'http.body', 'http.header',
  'http.method', 'http.status', 'timing',
];

interface ScopeSelectorProps {
  value: Scope[];
  onChange: (scopes: Scope[]) => void;
  disabled?: boolean;
}

/** Multi-select chip group for choosing which scopes a port enables. */
export function ScopeSelector({ value, onChange, disabled }: ScopeSelectorProps) {
  const toggle = (scope: Scope) => {
    if (disabled) return;
    if (value.includes(scope)) {
      onChange(value.filter(s => s !== scope));
    } else {
      onChange([...value, scope]);
    }
  };

  return (
    <div className="scope-selector">
      {ALL_SCOPES.map(scope => (
        <label key={scope} className={`scope-selector__item ${value.includes(scope) ? 'scope-selector__item--active' : ''} ${disabled ? 'scope-selector__item--disabled' : ''}`}>
          <input
            type="checkbox"
            checked={value.includes(scope)}
            onChange={() => toggle(scope)}
            disabled={disabled}
            className="sr-only"
          />
          <span className="scope-selector__label">{SCOPE_LABELS[scope]}</span>
        </label>
      ))}
    </div>
  );
}
