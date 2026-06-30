import { useState } from 'react';
import { ChevronDown, ChevronRight, CheckCircle2, XCircle, AlertCircle, Trash2 } from 'lucide-react';
import type { EvaluationDetail } from '../api/types';
import './RuleDetailCard.css';

interface RuleDetailCardProps {
  detail: EvaluationDetail;
  isRemoved: boolean;
}

/**
 * Expandable card for a single rule's contribution to one evaluation: its match
 * status (satisfied / unmatched / skipped / error), whether it drove the drop,
 * and the matcher details on expand. `isRemoved` flags a rule no longer in the
 * live ruleset (it appears in a historical record but not the current rules).
 */
export function RuleDetailCard({ detail, isRemoved }: RuleDetailCardProps) {
  const [expanded, setExpanded] = useState(false);

  // Determine status classification
  const status = detail.match_output.status;
  const isMatched = status === 'satisfied';
  const isError = status === 'error';
  const isSkipped = status === 'skipped';
  
  // Rule kind
  const isFilter = detail.kind === 'filter';
  
  // Card border/bg color classes
  let modifierClass = 'rule-card--unmatched';
  if (isMatched) modifierClass = 'rule-card--matched';
  if (isError) modifierClass = 'rule-card--error';
  if (isSkipped) modifierClass = 'rule-card--skipped';
  
  // Specific drop styling for filters
  if (isFilter && detail.decision_effect?.includes('drop')) {
    modifierClass = 'rule-card--dropped';
  }

  return (
    <div className={`rule-card ${modifierClass} ${expanded ? 'rule-card--expanded' : ''}`}>
      <div className="rule-card__header" onClick={() => setExpanded(!expanded)}>
        <div className="rule-card__icon">
          {isMatched ? <CheckCircle2 size={16} className="text-success" /> : 
           (isError || (isFilter && detail.decision_effect?.includes('drop'))) ? <XCircle size={16} className="text-error" /> : 
           <AlertCircle size={16} className="text-muted" />}
        </div>
        
        <div className="rule-card__summary">
          <span className="rule-card__kind">{detail.kind.toUpperCase()}</span>
          <span className="rule-card__id">
            {detail.id}
            {isRemoved && <span className="removed-indicator" title="Rule has been removed" style={{ display: 'inline-flex', verticalAlign: 'text-bottom', marginLeft: 'var(--space-1)' }}><Trash2 size={14} className="text-muted" /></span>}
          </span>
          {detail.name && <span className="rule-card__name">({detail.name})</span>}
        </div>

        <div className="rule-card__status-text">
          {detail.gated === false ? (
            <span className="text-warning">Gated (Skipped)</span>
          ) : (
            <span className={
              isMatched ? 'text-success' : 
              isError ? 'text-error' : 
              isSkipped ? 'text-warning' : 'text-muted'
            }>
              {status === 'satisfied' ? 'Matched' : 
               status === 'notsatisfied' ? 'Unmatched' : 
               status === 'error' ? 'Error' : 'Skipped'}
            </span>
          )}
          
          {isFilter && detail.decision_effect && (
            <span className={`rule-card__effect ${detail.decision_effect.includes('drop') ? 'text-error' : 'text-success'}`}>
              -&gt; {detail.decision_effect.toUpperCase()}
            </span>
          )}
        </div>

        <button className="rule-card__toggle btn btn--ghost btn--icon">
          {expanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
        </button>
      </div>

      {expanded && (
        <div className="rule-card__body">
          <div className="rule-card__grid">
            <div className="rule-card__field">
              <span className="rule-card__label">Scope</span>
              <span className="rule-card__value mono">{detail.match_output.scope}</span>
            </div>
            <div className="rule-card__field">
              <span className="rule-card__label">Condition</span>
              <span className="rule-card__value mono">{detail.match_output.condition}</span>
            </div>
            
            <div className="rule-card__field">
              <span className="rule-card__label">Input</span>
              <span className="rule-card__value">{detail.match_output.input || '(empty)'}</span>
            </div>
            <div className="rule-card__field">
              <span className="rule-card__label">Comparator</span>
              <span className="rule-card__value">{detail.match_output.comparator}</span>
            </div>

            {detail.match_output.matched && (
              <div className="rule-card__field rule-card__field--wide">
                <span className="rule-card__label">Matched Extract</span>
                <span className="rule-card__value mono" style={{ color: 'var(--color-accent)' }}>"{detail.match_output.matched}"</span>
              </div>
            )}
            
            {detail.match_output.flags && detail.match_output.flags.length > 0 && (
              <div className="rule-card__field">
                <span className="rule-card__label">Flags</span>
                <span className="rule-card__value">{detail.match_output.flags.join(', ')}</span>
              </div>
            )}
            
            {detail.match_output.numeric != null && (
              <div className="rule-card__field">
                <span className="rule-card__label">Numeric</span>
                <span className="rule-card__value">{detail.match_output.numeric}</span>
              </div>
            )}

            {detail.mode && (
              <div className="rule-card__field">
                <span className="rule-card__label">Mode</span>
                <span className="rule-card__value">{detail.mode}</span>
              </div>
            )}
            
            {detail.match_output.comment && (
              <div className="rule-card__field rule-card__field--wide">
                <span className="rule-card__label">Comment</span>
                <span className="rule-card__value">{detail.match_output.comment}</span>
              </div>
            )}
            
            {detail.match_output.error && (
              <div className="rule-card__field rule-card__field--wide">
                <span className="rule-card__label">Error</span>
                <span className="rule-card__value text-error">{detail.match_output.error}</span>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
