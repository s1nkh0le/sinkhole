import { DIRECTION_LABELS } from '../lib/legality';
import type { RuleDirection } from '../api/types';
import './DirectionControl.css';

interface DirectionControlProps {
  value: RuleDirection;
  onChange: (dir: RuleDirection) => void;
  legalDirections?: RuleDirection[];
  disabled?: boolean;
}

const ALL_DIRECTIONS: RuleDirection[] = ['inbound', 'outbound', 'both'];

/**
 * Segmented control for picking a rule direction (inbound / outbound / both).
 * Directions outside `legalDirections` are shown disabled rather than hidden, so
 * the user can see why a choice is unavailable for the current scope.
 */
export function DirectionControl({ value, onChange, legalDirections, disabled }: DirectionControlProps) {
  const legal = legalDirections ?? ALL_DIRECTIONS;

  return (
    <div className="direction-control">
      {ALL_DIRECTIONS.map(dir => {
        const isLegal = legal.includes(dir);
        const isActive = value === dir;
        return (
          <button
            key={dir}
            className={`direction-control__btn ${isActive ? 'direction-control__btn--active' : ''} ${!isLegal ? 'direction-control__btn--illegal' : ''}`}
            onClick={() => isLegal && !disabled && onChange(dir)}
            disabled={disabled || !isLegal}
            title={!isLegal ? `${DIRECTION_LABELS[dir]} is not valid for this scope` : DIRECTION_LABELS[dir]}
          >
            {dir === 'inbound' && '↓ '}
            {dir === 'outbound' && '↑ '}
            {dir === 'both' && '↕ '}
            {DIRECTION_LABELS[dir]}
          </button>
        );
      })}
    </div>
  );
}
