import { useState, useEffect, useCallback, useRef } from 'react';
import { getApi } from '../api/client';
import { useToast } from '../stores/ToastContext';
import { Badge } from '../components/Badge';
import { DirectionControl } from '../components/DirectionControl';
import { Modal } from '../components/Modal';
import { CopyButton } from '../components/CopyButton';
import { ErrorDisplay } from '../components/ErrorDisplay';
import { LoadingSpinner } from '../components/LoadingSpinner';
import {
  legalMatchersForScope, legalDirectionsForScope,
  SCOPE_LABELS, DIRECTION_LABELS, MODE_LABELS, MATCHER_TYPE_LABELS, ALL_SCOPES,
} from '../lib/legality';
import { RULE_LIBRARY, instantiateLibraryEntry } from '../lib/ruleLibrary';
import type { RuleSetRaw, MarkerDef, FilterDef, Scope, MatcherDef, RuleDirection, CmpOp, Mode, CtrlReplyBodyServiceState } from '../api/types';
import { HistoryModal } from '../components/HistoryModal';
import { Plus, Code, Library, Globe, Trash2, ArrowUp, ArrowDown, CopyPlus, FileJson } from 'lucide-react';
import './RulesetEditor.css';

interface RulesetEditorProps {
  serviceId: string;
  port: number;
  enabledScopes: Scope[];
  view: 'markers' | 'filters';
  onDirtyChange?: (dirty: boolean) => void;
  onCountsChange?: (markers: number, filters: number) => void;
  isDegraded?: boolean;
}

function createDefaultMatcher(scope: Scope): MatcherDef {
  const legal = legalMatchersForScope(scope);
  const firstType = legal[0];
  switch (firstType) {
    case 'stream_regex': return { type: 'stream_regex', pattern: '', case_sensitive: false };
    case 'path_regex': return { type: 'path_regex', pattern: '', case_sensitive: false };
    case 'body_regex': return { type: 'body_regex', pattern: '', case_sensitive: false };
    case 'header_regex': return { type: 'header_regex', header_name: '', pattern: '', case_sensitive: false };
    case 'method_regex': return { type: 'method_regex', pattern: '', case_sensitive: false };
    case 'status_regex': return { type: 'status_regex', pattern: '', case_sensitive: false };
    case 'size': return { type: 'size', op: '>', value: 0 };
    case 'session.lifetime': return { type: 'session.lifetime', op: '>', value: 0 };
    case 'session.idle': return { type: 'session.idle', op: '>', value: 0 };
    case 'stream_bytes': return { type: 'stream_bytes', pattern: '' };
    case 'body_bytes': return { type: 'body_bytes', pattern: '' };
    case 'bytes_hex': return { type: 'bytes_hex', value: '' };
    default: return { type: 'always', value: true };
  }
}

function createDefaultDirection(scope: Scope): RuleDirection {
  const legal = legalDirectionsForScope(scope);
  return legal.includes('both') ? 'both' : legal[0];
}

function newMarker(): MarkerDef {
  return { id: `marker_${Date.now().toString(36)}`, name: '', enabled: true, scope: 'stream', direction: 'both', matcher: { type: 'stream_regex', pattern: '', case_sensitive: false } };
}

function newFilter(): FilterDef {
  return { id: `filter_${Date.now().toString(36)}`, name: '', enabled: true, scope: 'stream', direction: 'both', matcher: { type: 'stream_regex', pattern: '', case_sensitive: false }, mode: 'blacklist' };
}

/**
 * Editor for one port's ruleset: the markers and filters that the engine
 * compiles and runs. Loads the current ruleset, edits markers/filters in local
 * draft state (per-row stable keys keep React reconciliation steady across
 * reorders), validates legality client-side, and saves a new version. The `view`
 * prop selects the markers or filters tab; dirty state and rule counts are
 * reported upward so the host page can guard navigation and show counts.
 */
export function RulesetEditor({ serviceId, port, enabledScopes, view, onDirtyChange, onCountsChange, isDegraded }: RulesetEditorProps) {
  const { showToast } = useToast();
  // Stable key counter via ref to avoid cross-instance pollution
  const stableKeyRef = useRef(0);
  const nextStableKey = useCallback(() => ++stableKeyRef.current, []);
  const [markers, setMarkers] = useState<MarkerDef[]>([]);
  const [filters, setFilters] = useState<FilterDef[]>([]);
  const [markerKeys, setMarkerKeys] = useState<number[]>([]);
  const [filterKeys, setFilterKeys] = useState<number[]>([]);
  
  useEffect(() => {
    onCountsChange?.(markers.length, filters.length);
  }, [markers.length, filters.length, onCountsChange]);
  const [commit_message, setCommitMessage] = useState('');
  const [serverVersion, setServerVersion] = useState('');
  const [dirty, setDirty] = useState(false);

  // Notify parent when dirty state changes
  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);
  const [loading, setLoading] = useState(true);
  const [deploying, setDeploying] = useState(false);
  const [validating, setValidating] = useState(false);

  const [validationError, setValidationError] = useState<string | null>(null);
  const [deployError, setDeployError] = useState<string | null>(null);
  const [expandedRule, setExpandedRule] = useState<string | null>(null);
  const [showAddDialog, setShowAddDialog] = useState<'marker' | 'filter' | null>(null);
  const [addTab, setAddTab] = useState<'custom' | 'json' | 'library' | 'deployed'>('custom');
  const [libCategory, setLibCategory] = useState<string>('All');
  const [deployedState, setDeployedState] = useState<CtrlReplyBodyServiceState[] | null>(null);
  const [expandedSvc, setExpandedSvc] = useState<string | null>(null);
  const [selectedDeployedPort, setSelectedDeployedPort] = useState<{svcId: string, port: number} | null>(null);
  const [selectedDeployedRule, setSelectedDeployedRule] = useState<{ type: 'marker' | 'filter', item: MarkerDef | FilterDef } | null>(null);
  const [jsonImport, setJsonImport] = useState('');
  const [showRawJson, setShowRawJson] = useState(false);
  const [rawJsonText, setRawJsonText] = useState('');

  // Preload deployed state when Add dialog opens
  useEffect(() => {
    if (showAddDialog && !deployedState) {
      getApi().GET('/state').then(res => setDeployedState(res.data?.data?.services || []));
    }
  }, [showAddDialog, deployedState]);

  
  const [showHistory, setShowHistory] = useState(false);
  const serverRef = useRef<RuleSetRaw | null>(null);

  const loadRuleset = useCallback(async () => {
    setLoading(true);
    try {
      const api = getApi();
      const { data, error } = await api.GET('/services/{service_id}/ports/{port}/ruleset', {
        params: { path: { service_id: serviceId, port } },
      });
      if (error) throw error;
      const rs = data!.data;
      serverRef.current = rs.ruleset;
      setMarkers(rs.ruleset.markers);
      setFilters(rs.ruleset.filters);
      setMarkerKeys(rs.ruleset.markers.map(() => nextStableKey()));
      setFilterKeys(rs.ruleset.filters.map(() => nextStableKey()));
      setCommitMessage('');
      setServerVersion(rs.version);
      setDirty(false);
      setValidationError(null);
      setDeployError(null);
    } catch (e) {
      setDeployError(String(e));
    }
    setLoading(false);
  }, [serviceId, port, nextStableKey]);

  useEffect(() => { setTimeout(loadRuleset, 0); }, [loadRuleset]);

  const currentRuleset = useCallback((): RuleSetRaw => ({
    markers, filters, commit_message: commit_message || null,
  }), [markers, filters, commit_message]);

  const handleValidate = useCallback(async () => {
    setValidating(true);
    setValidationError(null);
    try {
      const api = getApi();
      const res = await api.PUT('/services/{service_id}/ports/{port}/ruleset/validate', {
        params: { path: { service_id: serviceId, port } },
        body: currentRuleset(),
      });
      if (res.error) {
        // Validation failures come back as an ErrorEnvelope with the logs in `detail`.
        const errData = res.error as { title?: string; detail?: string };
        setValidationError(errData.detail ?? errData.title ?? 'Validation failed');
        showToast('Validation failed', 'error');
      } else {
        setValidationError(null);
        setDeployError(null);
        showToast('Ruleset validation passed ✓', 'success');
      }
    } catch (e) {
      setValidationError(String(e));
      showToast('Validation error', 'error');
    }
    setValidating(false);
  }, [serviceId, port, currentRuleset, showToast]);


  const handleDeploy = useCallback(async () => {
    setDeploying(true);
    setDeployError(null);
    try {
      const api = getApi();
      const res = await api.PUT('/services/{service_id}/ports/{port}/ruleset', {
        params: { path: { service_id: serviceId, port } },
        body: currentRuleset(),
      });
      if (res.error) {
        const errData = res.error as { title?: string; detail?: string; error?: string };
        if (errData.error === 'ruleset_validation_failed') {
          // Ruleset rejected by the filter: show the validation logs inline.
          setValidationError(errData.detail ?? 'Validation failed');
        } else {
          // Any other failure (unreachable filter, unknown port, ...): show title + detail.
          setDeployError([errData.title, errData.detail].filter(Boolean).join(': ') || 'Deploy failed');
        }
      } else if (res.data) {
        setServerVersion(res.data.data.version);
        serverRef.current = res.data.data.ruleset;
        setDirty(false);
        setCommitMessage('');
        setValidationError(null);
        showToast('Ruleset deployed successfully');
      }
    } catch (e) {
      setDeployError(String(e));
    }
    setDeploying(false);
  }, [serviceId, port, currentRuleset, showToast]);

  const handleDiscard = () => {
    if (serverRef.current) {
      setMarkers(serverRef.current.markers);
      setFilters(serverRef.current.filters);
      setMarkerKeys(serverRef.current.markers.map(() => nextStableKey()));
      setFilterKeys(serverRef.current.filters.map(() => nextStableKey()));
      setCommitMessage('');
      setDirty(false);
      setValidationError(null);
      setDeployError(null);
    }
  };

  // Mutation helpers that set dirty
  const updateMarker = useCallback((index: number, patch: Partial<MarkerDef>) => {
    setMarkers(prev => prev.map((m, i) => i === index ? { ...m, ...patch } as MarkerDef : m));
    setDirty(true);
  }, []);
  const updateFilter = useCallback((index: number, patch: Partial<FilterDef>) => {
    setFilters(prev => prev.map((f, i) => i === index ? { ...f, ...patch } as FilterDef : f));
    setDirty(true);
  }, []);
  const removeMarker = useCallback((index: number) => {
    setMarkers(prev => prev.filter((_, i) => i !== index));
    setMarkerKeys(prev => prev.filter((_, i) => i !== index));
    setDirty(true);
  }, []);
  const removeFilter = useCallback((index: number) => {
    setFilters(prev => prev.filter((_, i) => i !== index));
    setFilterKeys(prev => prev.filter((_, i) => i !== index));
    setDirty(true);
  }, []);
  const moveMarker = useCallback((from: number, to: number) => {
    setMarkers(prev => { const arr = [...prev]; const [item] = arr.splice(from, 1); arr.splice(to, 0, item); return arr; });
    setMarkerKeys(prev => { const arr = [...prev]; const [item] = arr.splice(from, 1); arr.splice(to, 0, item); return arr; });
    setDirty(true);
  }, []);
  const moveFilter = useCallback((from: number, to: number) => {
    setFilters(prev => { const arr = [...prev]; const [item] = arr.splice(from, 1); arr.splice(to, 0, item); return arr; });
    setFilterKeys(prev => { const arr = [...prev]; const [item] = arr.splice(from, 1); arr.splice(to, 0, item); return arr; });
    setDirty(true);
  }, []);
  const duplicateMarker = useCallback((index: number) => {
    setMarkers(prev => {
      const src = prev[index];
      const copy = { ...JSON.parse(JSON.stringify(src)), id: src.id };
      return [...prev.slice(0, index + 1), copy, ...prev.slice(index + 1)];
    });
    setMarkerKeys(prev => [...prev.slice(0, index + 1), nextStableKey(), ...prev.slice(index + 1)]);
    setDirty(true);
  }, [nextStableKey]);
  const duplicateFilter = useCallback((index: number) => {
    setFilters(prev => {
      const src = prev[index];
      const copy = { ...JSON.parse(JSON.stringify(src)), id: src.id };
      return [...prev.slice(0, index + 1), copy, ...prev.slice(index + 1)];
    });
    setFilterKeys(prev => [...prev.slice(0, index + 1), nextStableKey(), ...prev.slice(index + 1)]);
    setDirty(true);
  }, [nextStableKey]);

  const handleAddCustom = useCallback((kind: 'marker' | 'filter') => {
    if (kind === 'marker') {
      setMarkers(prev => [...prev, newMarker()]);
      setMarkerKeys(prev => [...prev, nextStableKey()]);
    } else {
      setFilters(prev => [...prev, newFilter()]);
      setFilterKeys(prev => [...prev, nextStableKey()]);
    }
    setDirty(true);
    setShowAddDialog(null);
  }, [nextStableKey]);

  const handleImportJson = useCallback(() => {
    try {
      const parsed = JSON.parse(jsonImport);
      if (showAddDialog === 'marker' && parsed.id && parsed.scope && parsed.matcher) {
        setMarkers(prev => [...prev, parsed]);
        setMarkerKeys(prev => [...prev, nextStableKey()]);
      } else if (showAddDialog === 'filter' && parsed.id && parsed.scope && parsed.matcher && parsed.mode) {
        setFilters(prev => [...prev, parsed]);
        setFilterKeys(prev => [...prev, nextStableKey()]);
      } else {
        showToast('Invalid JSON structure', 'error');
        return;
      }
      setDirty(true);
      setShowAddDialog(null);
      setJsonImport('');
    } catch { showToast('Invalid JSON', 'error'); }
  }, [jsonImport, showAddDialog, showToast, nextStableKey]);

  // Shared helper: add a deployed rule with marker-dependency pruning
  const addDeployedRule = useCallback((rule: { type: 'marker' | 'filter'; item: MarkerDef | FilterDef }) => {
    const item = rule.item;
    if (rule.type === 'marker') {
      setMarkers(prev => [...prev, { ...(item as MarkerDef) }]);
      setMarkerKeys(prev => [...prev, nextStableKey()]);
    } else {
      setMarkers(currentMarkers => {
        // Read current markers to get valid IDs for pruning
        const validMarkerIds = new Set(currentMarkers.map(m => m.id));
        const f = item as FilterDef;
        const prunedFilter = {
          ...f,
          needed_markers_all: f.needed_markers_all?.filter(m => validMarkerIds.has(m)),
          needed_markers_any: f.needed_markers_any?.filter(m => validMarkerIds.has(m)),
        };
        if (prunedFilter.needed_markers_all?.length === 0) delete prunedFilter.needed_markers_all;
        if (prunedFilter.needed_markers_any?.length === 0) delete prunedFilter.needed_markers_any;
        setFilters(prev => [...prev, prunedFilter]);
        setFilterKeys(prev => [...prev, nextStableKey()]);
        return currentMarkers; // unchanged
      });
    }
    setDirty(true);
    setShowAddDialog(null);
    setSelectedDeployedRule(null);
  }, [nextStableKey]);

  useEffect(() => {
    if (!showAddDialog) return;
    const handleKeyDownDialog = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') return;
        if (e.key === 'Enter' && e.target instanceof HTMLTextAreaElement) return;
      }
      
      const tabs: Array<'custom' | 'library' | 'deployed' | 'json'> = ['custom', 'library', 'deployed', 'json'];
      const idx = tabs.indexOf(addTab);
      
      if (e.key === 'ArrowLeft') {
        e.preventDefault();
        setAddTab(tabs[(idx - 1 + tabs.length) % tabs.length]);
      } else if (e.key === 'ArrowRight') {
        e.preventDefault();
        setAddTab(tabs[(idx + 1) % tabs.length]);
      } else if (e.key === 'Enter') {
        e.preventDefault();
        if (addTab === 'custom') {
          handleAddCustom(showAddDialog);
        } else if (addTab === 'json') {
          handleImportJson();
        } else if (addTab === 'deployed' && selectedDeployedRule) {
          addDeployedRule(selectedDeployedRule);
        }
      }
    };
    window.addEventListener('keydown', handleKeyDownDialog);
    return () => window.removeEventListener('keydown', handleKeyDownDialog);
  }, [showAddDialog, addTab, selectedDeployedRule, handleAddCustom, handleImportJson, addDeployedRule]);

  const handleLibraryPick = (entryId: string) => {
    const entry = RULE_LIBRARY.find(e => e.id === entryId);
    if (!entry) return;
    const existingIds = new Set([...markers.map(m => m.id), ...filters.map(f => f.id)]);
    const { markers: newMarkers, filters: newFilters } = instantiateLibraryEntry(entry, existingIds);
    setMarkers(prev => [...prev, ...newMarkers]);
    setMarkerKeys(prev => [...prev, ...newMarkers.map(() => nextStableKey())]);
    setFilters(prev => [...prev, ...newFilters]);
    setFilterKeys(prev => [...prev, ...newFilters.map(() => nextStableKey())]);
    setDirty(true);
    setShowAddDialog(null);
  };

  // Export/Import whole ruleset
  const handleExportRuleset = () => {
    const blob = new Blob([JSON.stringify(currentRuleset(), null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `ruleset_${serviceId}_${port}.json`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  const handleImportRuleset = () => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json';
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (!file) return;
      const text = await file.text();
      try {
        const parsed = JSON.parse(text) as RuleSetRaw;
        if (!parsed.markers || !parsed.filters) throw new Error('Invalid ruleset');
        setMarkers(parsed.markers);
        setFilters(parsed.filters);
        setMarkerKeys(parsed.markers.map(() => nextStableKey()));
        setFilterKeys(parsed.filters.map(() => nextStableKey()));
        setCommitMessage(parsed.commit_message ?? '');
        setDirty(true);
      } catch { showToast('Invalid ruleset JSON', 'error'); }
    };
    input.click();
  };



  const handleRawJsonApply = () => {
    try {
      const parsed = JSON.parse(rawJsonText) as RuleSetRaw;
      if (!parsed.markers || !parsed.filters) throw new Error('Invalid');
      setMarkers(parsed.markers);
      setFilters(parsed.filters);
      setMarkerKeys(parsed.markers.map(() => nextStableKey()));
      setFilterKeys(parsed.filters.map(() => nextStableKey()));
      setCommitMessage(parsed.commit_message ?? '');
      setDirty(true);
      setShowRawJson(false);
    } catch { showToast('Invalid JSON', 'error'); }
  };

  useEffect(() => {
    const handleGlobalKeyDown = (e: KeyboardEvent) => {
      // Don't trigger if inside an input/textarea
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        // Allow Shift+Enter to deploy even from commit message textarea
        if (e.key === 'Enter' && e.shiftKey) {
          e.preventDefault();
          if (!deploying && dirty) handleDeploy();
        }
        return;
      }
      
      if (e.key === 'Enter' && e.shiftKey) {
        e.preventDefault();
        if (!deploying && dirty) handleDeploy();
      }
      
      if (e.key === '+' && !showAddDialog) {
        e.preventDefault();
        setShowAddDialog('filter');
      }
    };
    window.addEventListener('keydown', handleGlobalKeyDown);
    return () => window.removeEventListener('keydown', handleGlobalKeyDown);
  }, [deploying, dirty, showAddDialog, handleDeploy]);

  if (loading) {
    return <div className="page-center"><LoadingSpinner label="Loading ruleset…" /></div>;
  }

  const markerIds = markers.map(m => m.id);

  // Group filters by mode for display
  const blacklistFilters: { filter: FilterDef; originalIndex: number }[] = [];
  const whitelistAllFilters: { filter: FilterDef; originalIndex: number }[] = [];
  const whitelistAnyFilters: { filter: FilterDef; originalIndex: number }[] = [];
  filters.forEach((f, idx) => {
    switch (f.mode) {
      case 'blacklist': blacklistFilters.push({ filter: f, originalIndex: idx }); break;
      case 'whitelistall': whitelistAllFilters.push({ filter: f, originalIndex: idx }); break;
      case 'whitelistany': whitelistAnyFilters.push({ filter: f, originalIndex: idx }); break;
    }
  });


  return (
    <div className="ruleset-editor">
      {/* Toolbar */}
      <div className="ruleset-editor__toolbar flex items-center justify-between flex-wrap gap-2">
        <div className="flex items-center gap-2">
          {view === 'markers' && (
            <button className="btn btn--secondary btn--sm" onClick={() => setShowAddDialog('marker')}>+ Marker</button>
          )}
          {view === 'filters' && (
            <button className="btn btn--secondary btn--sm" onClick={() => setShowAddDialog('filter')}>+ Filter</button>
          )}
          <div className="toolbar-divider" />
          <button className="btn btn--ghost btn--sm" onClick={handleImportRuleset}>Import</button>
          <button className="btn btn--ghost btn--sm" onClick={handleExportRuleset}>Export</button>
          <button className="btn btn--ghost btn--sm" onClick={() => {
            if (!showRawJson) setRawJsonText(JSON.stringify(currentRuleset(), null, 2));
            setShowRawJson(!showRawJson);
          }}>
            {showRawJson ? 'Form View' : 'JSON View'}
          </button>
        </div>
        <div className="flex items-center gap-2">
          {dirty && <Badge variant="warning">Unsaved</Badge>}
          <span className="ruleset-editor__version">v:{serverVersion.slice(0, 8)}</span>
          <button className="btn btn--ghost btn--sm" onClick={handleDiscard} disabled={!dirty}>Discard</button>
          <button className="btn btn--secondary btn--sm" onClick={handleValidate} disabled={validating}>
            {validating ? 'Validating…' : 'Validate'}
          </button>
          <button className="btn btn--primary btn--sm" onClick={handleDeploy} disabled={deploying || !dirty}>
            {deploying ? 'Deploying…' : 'Deploy'}
          </button>
          <button className="btn btn--ghost btn--sm" onClick={() => setShowHistory(true)} title="View ruleset history">
            History
          </button>
        </div>
      </div>

      {isDegraded && (
        <div style={{ marginBottom: 'var(--space-4)' }}>
          <ErrorDisplay 
            variant="warning" 
            error={{ 
              title: 'Service is degraded or offline', 
              detail: 'The service may not be reachable. Deploying changes may fail or not be applied.',
              status: '',
              code: 0
            }} 
          />
        </div>
      )}

      {validationError && (
        <div style={{ marginBottom: 'var(--space-4)' }}>
          <ErrorDisplay error={{ title: 'Validation Error', detail: <pre style={{ fontFamily: 'var(--font-mono)', margin: 0, whiteSpace: 'pre-wrap' }}>{validationError}</pre>, status: 'error', code: 0 }} />
        </div>
      )}

      {deployError && <ErrorDisplay error={deployError} />}

      {showRawJson ? (
        <div className="ruleset-editor__raw">
          <textarea
            className="ruleset-editor__raw-textarea"
            value={rawJsonText}
            onChange={e => setRawJsonText(e.target.value)}
            spellCheck={false}
          />
          <div className="form-actions">
            <button className="btn btn--secondary btn--sm" onClick={() => setShowRawJson(false)}>Cancel</button>
            <button className="btn btn--primary btn--sm" onClick={handleRawJsonApply}>Apply JSON</button>
          </div>
        </div>
      ) : (
        <>
          {/* Markers view */}
          {view === 'markers' && (
            <div className="ruleset-editor__section">
              <h3 className="ruleset-editor__section-title flex items-center gap-2 mb-3">
                Markers <Badge variant="default">{markers.length}</Badge>
              </h3>
              {markers.length === 0 && <p className="meta-display__empty">No markers defined</p>}
              {markers.map((marker, idx) => (
                <RuleCard
                  key={markerKeys[idx]}
                  kind="marker"
                  rule={marker}
                  index={idx}
                  total={markers.length}
                  expanded={expandedRule === `m_${idx}`}
                  onToggleExpand={() => setExpandedRule(expandedRule === `m_${idx}` ? null : `m_${idx}`)}
                  onUpdate={(patch) => updateMarker(idx, patch)}
                  onRemove={() => removeMarker(idx)}
                  onMoveUp={() => idx > 0 && moveMarker(idx, idx - 1)}
                  onMoveDown={() => idx < markers.length - 1 && moveMarker(idx, idx + 1)}
                  onDuplicate={() => duplicateMarker(idx)}
                  enabledScopes={enabledScopes}
                  markerIds={markerIds}
                  hideModeBadge={false}
                />
              ))}
            </div>
          )}

          {/* Filters view — grouped by mode */}
          {view === 'filters' && (
            <>
              <FilterModeSection
                title="Blacklist"
                items={blacklistFilters}
                totalFilters={filters.length}
                expandedRule={expandedRule}
                setExpandedRule={setExpandedRule}
                updateFilter={updateFilter}
                removeFilter={removeFilter}
                moveFilter={moveFilter}
                duplicateFilter={duplicateFilter}
                enabledScopes={enabledScopes}
                markerIds={markerIds}
                filterKeys={filterKeys}
              />
              <FilterModeSection
                title="Whitelist All"
                items={whitelistAllFilters}
                totalFilters={filters.length}
                expandedRule={expandedRule}
                setExpandedRule={setExpandedRule}
                updateFilter={updateFilter}
                removeFilter={removeFilter}
                moveFilter={moveFilter}
                duplicateFilter={duplicateFilter}
                enabledScopes={enabledScopes}
                markerIds={markerIds}
                filterKeys={filterKeys}
              />
              <FilterModeSection
                title="Whitelist Any"
                items={whitelistAnyFilters}
                totalFilters={filters.length}
                expandedRule={expandedRule}
                setExpandedRule={setExpandedRule}
                updateFilter={updateFilter}
                removeFilter={removeFilter}
                moveFilter={moveFilter}
                duplicateFilter={duplicateFilter}
                enabledScopes={enabledScopes}
                markerIds={markerIds}
                filterKeys={filterKeys}
              />
            </>
          )}

          {/* Commit Message */}
          <div className="form-field" style={{ marginTop: 'var(--space-4)' }}>
            <label className="form-label">Commit Message</label>
            <textarea 
              value={commit_message} 
              onChange={e => { setCommitMessage(e.target.value); setDirty(true); }} 
              placeholder="Commit message" 
              rows={3}
              style={{ resize: 'vertical' }}
            />
          </div>
        </>
      )}


      {/* Add Rule Dialog */}
      {(() => {
        let modalFooter = null;
        if (addTab === 'custom') {
          modalFooter = <button className="btn btn--primary" onClick={() => handleAddCustom(showAddDialog!)}>Add {showAddDialog === 'marker' ? 'Marker' : 'Filter'}</button>;
        } else if (addTab === 'json') {
          modalFooter = <button className="btn btn--primary" onClick={handleImportJson}>Import</button>;
        } else if (addTab === 'deployed') {
          modalFooter = (
            <button
              className="btn btn--primary"
              disabled={!selectedDeployedRule}
              onClick={() => selectedDeployedRule && addDeployedRule(selectedDeployedRule)}
            >
              Add {showAddDialog === 'marker' ? 'Marker' : 'Filter'}
            </button>
          );
        }

        return (
          <Modal open={!!showAddDialog} onClose={() => setShowAddDialog(null)} title={`Add ${showAddDialog === 'marker' ? 'Marker' : 'Filter'}`} size="xxl" footer={modalFooter}>

        <div className="add-dialog__tabs flex gap-1 pb-2 border-b border-[var(--color-border-subtle)] mb-4">
          <button className={`add-dialog__tab flex items-center gap-2 ${addTab === 'custom' ? 'add-dialog__tab--active' : ''}`} onClick={() => setAddTab('custom')}><Plus size={16}/> Custom</button>
          <button className={`add-dialog__tab flex items-center gap-2 ${addTab === 'library' ? 'add-dialog__tab--active' : ''}`} onClick={() => setAddTab('library')}><Library size={16}/> Library</button>
          <button className={`add-dialog__tab flex items-center gap-2 ${addTab === 'deployed' ? 'add-dialog__tab--active' : ''}`} onClick={() => setAddTab('deployed')}>
            <Globe size={16}/> From Deployed
          </button>
          <button className={`add-dialog__tab flex items-center gap-2 ${addTab === 'json' ? 'add-dialog__tab--active' : ''}`} onClick={() => setAddTab('json')}><Code size={16}/> From JSON</button>
        </div>

        {addTab === 'custom' && (
          <div>
            <p className="meta-display__desc">Creates a new blank {showAddDialog} with default settings. You can configure it after adding.</p>

          </div>
        )}

        {addTab === 'json' && (
          <div>
            <textarea
              className="ruleset-editor__raw-textarea"
              value={jsonImport}
              onChange={e => setJsonImport(e.target.value)}
              placeholder={`Paste a ${showAddDialog} JSON object\u2026`}
              rows={8}
              spellCheck={false}
            />

          </div>
        )}

        {addTab === 'library' && (() => {
          const categories = ['All', ...Array.from(new Set(RULE_LIBRARY.map(e => e.category)))];
          const filtered = RULE_LIBRARY.filter(e => libCategory === 'All' || e.category === libCategory);
          return (
            <div className="flex gap-4" style={{ minHeight: 300, maxHeight: 400 }}>
              <div className="flex flex-col gap-1 shrink-0 w-48 border-r border-[var(--color-border-subtle)] pr-2 overflow-y-auto">
                {categories.map(cat => (
                  <button key={cat} className={`btn btn--sm justify-start ${libCategory === cat ? 'btn--primary' : 'btn--ghost'}`} onClick={() => setLibCategory(cat)}>
                    {cat}
                  </button>
                ))}
              </div>
              <div className="library-list flex-1 flex flex-col gap-2 overflow-y-auto pr-2">
                {filtered.map(entry => (
                  <div key={entry.id} className="library-item" onClick={() => handleLibraryPick(entry.id)}>
                    <div className="library-item__header flex items-center justify-between mb-1">
                      <span className="library-item__name">{entry.name}</span>
                      <Badge variant="accent">{entry.category}</Badge>
                    </div>
                    <p className="library-item__desc">{entry.description}</p>
                    <div className="library-item__counts flex gap-1 mt-2">
                      {entry.markers.length > 0 && <Badge variant="default">{entry.markers.length} marker{entry.markers.length !== 1 ? 's' : ''}</Badge>}
                      {entry.filters.length > 0 && <Badge variant="default">{entry.filters.length} filter{entry.filters.length !== 1 ? 's' : ''}</Badge>}
                    </div>
                  </div>
                ))}
                {filtered.length === 0 && <span className="text-muted text-sm mt-4">No entries in this category.</span>}
              </div>
            </div>
          );
        })()}

        {addTab === 'deployed' && (() => {
          const activeSvc = deployedState?.find(s => s.id === selectedDeployedPort?.svcId);
          const activePort = activeSvc?.ports.find(p => p.port === selectedDeployedPort?.port);
          const activeRuleset = activePort?.snapshot?.ruleset;
          const items = activeRuleset ? (showAddDialog === 'marker' ? activeRuleset.markers : activeRuleset.filters) : [];

          return (
            <div className="flex flex-col">
              <div className="flex gap-4" style={{ minHeight: 400, maxHeight: 550 }}>
                {/* Pane 1: Services & Ports */}
                <div className="flex flex-col gap-2 shrink-0 border-r border-[var(--color-border-subtle)] pr-4 overflow-y-auto" style={{ width: '25%' }}>
                  <h3 className="text-sm font-bold text-muted mb-2 uppercase tracking-wider">Targets</h3>
                  {!deployedState ? (
                    <div className="flex justify-center p-4"><LoadingSpinner size="sm" /></div>
                  ) : deployedState.length === 0 ? (
                    <span className="text-muted text-sm">No deployed services found.</span>
                  ) : (
                    deployedState.map(svc => (
                      <div key={svc.id} className="border border-[var(--color-border-subtle)] rounded-md overflow-hidden mb-2 shrink-0">
                        <div className="flex items-center gap-2 p-2 bg-[var(--color-bg-secondary)] cursor-pointer" style={expandedSvc === svc.id ? { borderLeft: `3px solid ${svc.metadata?.hex_color || 'var(--color-accent)'}` } : { borderLeft: '3px solid transparent' }} onClick={() => setExpandedSvc(expandedSvc === svc.id ? null : svc.id)}>
                          
                          <span className="font-semibold text-sm">{svc.metadata?.name || svc.id}</span>
                        </div>
                        {expandedSvc === svc.id && (
                          <div className="flex flex-col border-t border-[var(--color-border-subtle)] bg-[var(--color-bg-primary)] p-2 gap-2">
                            {svc.ports.map(p => {
                              const isSelected = selectedDeployedPort?.svcId === svc.id && selectedDeployedPort?.port === p.port;
                              return (
                                <div 
                                  key={p.port} 
                                  className={`ml-4 flex items-center gap-2 p-2 rounded cursor-pointer ${isSelected ? 'bg-[var(--color-bg-hover)]' : 'hover:bg-[var(--color-bg-hover)]'}`} 
                                  onClick={() => {
                                    setSelectedDeployedPort({ svcId: svc.id, port: p.port });
                                    setSelectedDeployedRule(null);
                                  }}
                                >
                                  
                                  <div className="flex-1">
                                    <span className="text-sm font-medium" style={{ borderBottom: isSelected ? '2px solid var(--color-accent)' : '2px solid transparent', paddingBottom: '2px' }}>Port {p.port}</span>
                                  </div>
                                  <Badge variant="default" size="sm">
                                    <span className={showAddDialog === 'marker' ? 'font-bold underline text-[var(--color-text-primary)]' : 'font-normal text-muted opacity-10'}>{p.snapshot?.ruleset?.markers?.length || 0}</span>
                                    <span className="font-normal text-muted px-1 opacity-10">/</span>
                                    <span className={showAddDialog === 'filter' ? 'font-bold underline text-[var(--color-text-primary)]' : 'font-normal text-muted opacity-10'}>{p.snapshot?.ruleset?.filters?.length || 0}</span>
                                  </Badge>
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    ))
                  )}
                </div>
                
                {/* Pane 2: Markers / Filters List */}
                <div className="flex flex-col gap-2 shrink-0 border-r border-[var(--color-border-subtle)] pr-4 overflow-y-auto" style={{ width: '30%' }}>
                  <h3 className="text-sm font-bold text-muted mb-2 uppercase tracking-wider">{showAddDialog}s</h3>
                  {!selectedDeployedPort ? (
                    <div className="flex h-full items-center justify-center text-muted text-sm text-center">
                      Select a port to see its {showAddDialog}s.
                    </div>
                  ) : items.length === 0 ? (
                    <div className="text-muted text-sm">No {showAddDialog}s configured.</div>
                  ) : (
                    <div className="flex flex-col gap-2">
                      {items.map((item, idx) => {
                        const isSelected = selectedDeployedRule?.item.id === item.id;
                        return (
                          <div 
                            key={idx} 
                            className={`flex items-center justify-between p-3 rounded cursor-pointer border ${isSelected ? 'bg-[var(--color-accent)] text-[var(--color-bg-primary)] border-[var(--color-accent)]' : 'bg-[var(--color-bg-tertiary)] hover:bg-[var(--color-bg-hover)] border-[var(--color-border-subtle)]'}`} 
                            onClick={() => setSelectedDeployedRule({ type: showAddDialog!, item })}
                          >
                            <span className="text-sm font-mono truncate">{item.id}</span>
                            <Badge variant="default" size="sm">{item.scope}</Badge>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>

                {/* Pane 3: Selected Rule Details & Add Button */}
                <div className="flex flex-col flex-1 pl-2 overflow-y-auto">
                  <h3 className="text-sm font-bold text-muted mb-2 uppercase tracking-wider">Details</h3>
                  {!selectedDeployedRule ? (
                    <div className="flex h-full items-center justify-center text-muted text-sm text-center">
                      Select a {showAddDialog} to view details.
                    </div>
                  ) : (() => {
                    const detailJson = JSON.stringify(selectedDeployedRule.item, null, 2);
                    return (
                    <div className="flex flex-col bg-[var(--color-bg-secondary)] border border-[var(--color-border-subtle)] rounded-lg p-4 mb-4">
                      <div className="flex items-center justify-between mb-4 pb-2 border-b border-[var(--color-border-subtle)]">
                        <span className="text-lg font-bold font-mono text-[var(--color-accent)]">{selectedDeployedRule.item.id}</span>
                        <Badge variant={selectedDeployedRule.item.enabled ? 'success' : 'default'}>{selectedDeployedRule.item.enabled ? 'Enabled' : 'Disabled'}</Badge>
                      </div>
                      
                      <div className="flex flex-col gap-4">
                        <div className="flex flex-col gap-1">
                          <span className="text-xs text-muted uppercase tracking-wider font-semibold">Scope</span>
                          <span className="text-sm">{selectedDeployedRule.item.scope}</span>
                        </div>
                        
                        <div className="flex flex-col gap-1">
                          <span className="text-xs text-muted uppercase tracking-wider font-semibold">Direction</span>
                          <span className="text-sm">{selectedDeployedRule.item.direction}</span>
                        </div>

                        {'mode' in selectedDeployedRule.item && (
                          <div className="flex flex-col gap-1">
                            <span className="text-xs text-muted uppercase tracking-wider font-semibold">Mode</span>
                            <span className="text-sm">{(selectedDeployedRule.item as FilterDef).mode}</span>
                          </div>
                        )}

                        <div className="flex flex-col gap-1 mt-4">
                          <span className="text-xs text-muted uppercase tracking-wider font-semibold">Raw Configuration</span>
                          <div style={{ position: 'relative', marginTop: '4px' }}>
                            <pre 
                              className="text-xs font-mono rounded overflow-x-auto border border-[var(--color-border-subtle)]"
                              style={{ backgroundColor: '#000', color: '#d4d4d4', padding: '16px' }}
                            >
                              {detailJson}
                            </pre>
                            <div style={{ position: 'absolute', top: '8px', right: '8px' }}>
                              <CopyButton text={detailJson} />
                            </div>
                          </div>
                        </div>
                      </div>
                    </div>
                    );
                  })()}
                </div>
              </div>
            </div>
          );
        })()}
      </Modal>
      );
      })()}

      {showHistory && (
        <HistoryModal 
          serviceId={serviceId} 
          port={port} 
          currentVersion={serverVersion}
          open={showHistory}
          onClose={() => setShowHistory(false)} 
          onRollback={() => { setShowHistory(false); loadRuleset(); }} 
        />
      )}
    </div>
  );
}

/* ───── Filter Mode Section ───── */

interface FilterModeSectionProps {
  title: string;
  items: { filter: FilterDef; originalIndex: number }[];
  totalFilters: number;
  expandedRule: string | null;
  setExpandedRule: (v: string | null) => void;
  updateFilter: (index: number, patch: Partial<FilterDef>) => void;
  removeFilter: (index: number) => void;
  moveFilter: (from: number, to: number) => void;
  duplicateFilter: (index: number) => void;
  enabledScopes: Scope[];
  markerIds: string[];
  filterKeys: number[];
}

function FilterModeSection({
  title, items, totalFilters, expandedRule, setExpandedRule,
  updateFilter, removeFilter, moveFilter, duplicateFilter,
  enabledScopes, markerIds, filterKeys,
}: FilterModeSectionProps) {
  return (
    <div className="ruleset-editor__section">
      <h3 className="ruleset-editor__section-title">
        {title} <Badge variant="default">{items.length}</Badge>
      </h3>
      {items.length === 0 && <p className="meta-display__empty">No {title.toLowerCase()} filters</p>}
      {items.map(({ filter, originalIndex }) => (
        <RuleCard
          key={filterKeys[originalIndex]}
          kind="filter"
          rule={filter}
          index={originalIndex}
          total={totalFilters}
          expanded={expandedRule === `f_${originalIndex}`}
          onToggleExpand={() => setExpandedRule(expandedRule === `f_${originalIndex}` ? null : `f_${originalIndex}`)}
          onUpdate={(patch) => updateFilter(originalIndex, patch)}
          onRemove={() => removeFilter(originalIndex)}
          onMoveUp={() => originalIndex > 0 && moveFilter(originalIndex, originalIndex - 1)}
          onMoveDown={() => originalIndex < totalFilters - 1 && moveFilter(originalIndex, originalIndex + 1)}
          onDuplicate={() => duplicateFilter(originalIndex)}
          enabledScopes={enabledScopes}
          markerIds={markerIds}
          hideModeBadge={true}
        />
      ))}
    </div>
  );
}

/* ───── Rule Card ───── */

interface RuleCardProps {
  kind: 'marker' | 'filter';
  rule: MarkerDef | FilterDef;
  index: number;
  total: number;
  expanded: boolean;
  onToggleExpand: () => void;
  onUpdate: (patch: Partial<MarkerDef> | Partial<FilterDef>) => void;
  onRemove: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onDuplicate: () => void;
  enabledScopes: Scope[];
  markerIds: string[];
  hideModeBadge: boolean;
}

function RuleCard({
  kind, rule, index, total, expanded, onToggleExpand,
  onUpdate, onRemove, onMoveUp, onMoveDown, onDuplicate,
  enabledScopes, markerIds, hideModeBadge,
}: RuleCardProps) {
  const scopeDisabled = !enabledScopes.includes(rule.scope);
  const filter = kind === 'filter' ? rule as FilterDef : null;

  const handleScopeChange = (scope: Scope) => {
    const newDir = createDefaultDirection(scope);
    const newMatcher = createDefaultMatcher(scope);
    onUpdate({ scope, direction: newDir, matcher: newMatcher });
  };

  const handleMatcherTypeChange = (type: MatcherDef['type']) => {
    switch (type) {
      case 'stream_regex': onUpdate({ matcher: { type, pattern: '', case_sensitive: false } }); break;
      case 'path_regex': onUpdate({ matcher: { type, pattern: '', case_sensitive: false } }); break;
      case 'body_regex': onUpdate({ matcher: { type, pattern: '', case_sensitive: false } }); break;
      case 'header_regex': onUpdate({ matcher: { type, pattern: '', header_name: '', case_sensitive: false } }); break;
      case 'method_regex': onUpdate({ matcher: { type, pattern: '', case_sensitive: false } }); break;
      case 'status_regex': onUpdate({ matcher: { type, pattern: '', case_sensitive: false } }); break;
      case 'size': onUpdate({ matcher: { type, op: '>' as CmpOp, value: 0 } }); break;
      case 'session.lifetime': onUpdate({ matcher: { type, op: '>' as CmpOp, value: 0 } }); break;
      case 'session.idle': onUpdate({ matcher: { type, op: '>' as CmpOp, value: 0 } }); break;
      case 'stream_bytes': onUpdate({ matcher: { type, pattern: '' } }); break;
      case 'body_bytes': onUpdate({ matcher: { type, pattern: '' } }); break;
      case 'bytes_hex': onUpdate({ matcher: { type, value: '' } }); break;
      case 'always': onUpdate({ matcher: { type, value: true } }); break;
    }
  };

  return (
    <div className={`rule-card ${rule.enabled === false ? 'rule-card--disabled' : ''} ${scopeDisabled ? 'rule-card--scope-disabled' : ''}`}>
      <div className="rule-card__header flex items-center justify-between gap-3" onClick={onToggleExpand}>
        <div className="flex items-center gap-2 min-w-0">
          <label className="rule-card__toggle" onClick={e => e.stopPropagation()}>
            <input type="checkbox" checked={rule.enabled !== false} onChange={e => onUpdate({ enabled: e.target.checked })} />
          </label>
          <span className="rule-card__id">{rule.id}</span>
          {rule.name && <span className="rule-card__name">{rule.name}</span>}
        </div>
        <div className="flex items-center gap-1 shrink-0">
          {scopeDisabled && <Badge variant="warning">Scope disabled</Badge>}
          <Badge variant="info">{SCOPE_LABELS[rule.scope]}</Badge>
          <Badge variant="default">{DIRECTION_LABELS[rule.direction]}</Badge>
          <Badge variant="accent">{MATCHER_TYPE_LABELS[rule.matcher.type]}</Badge>
          {filter && !hideModeBadge && <Badge variant={filter.mode === 'blacklist' ? 'error' : 'success'}>{MODE_LABELS[filter.mode]}</Badge>}
          <span className="rule-card__expand">{expanded ? '▾' : '▸'}</span>
        </div>
      </div>

      {expanded && (
        <div className="rule-card__body">
          <div className="flex flex-col gap-3">
            <div className="form-field">
              <label className="form-label">ID</label>
              <input value={rule.id} onChange={e => onUpdate({ id: e.target.value })} style={{ fontFamily: 'var(--font-mono)' }} />
            </div>
            <div className="form-field">
              <label className="form-label">Name</label>
              <input value={rule.name ?? ''} onChange={e => onUpdate({ name: e.target.value || null })} placeholder="Optional label" />
            </div>
            <div className="form-field">
              <label className="form-label">Scope</label>
              <select value={rule.scope} onChange={e => handleScopeChange(e.target.value as Scope)}>
                {ALL_SCOPES.map(s => (
                  <option key={s} value={s}>
                    {SCOPE_LABELS[s]} {!enabledScopes.includes(s) ? '⚠' : ''}
                  </option>
                ))}
              </select>
            </div>
            <div className="form-field">
              <label className="form-label">Direction</label>
              <DirectionControl
                value={rule.direction}
                onChange={dir => onUpdate({ direction: dir })}
                legalDirections={legalDirectionsForScope(rule.scope)}
              />
            </div>

            {/* Matcher */}
            <div className="form-field">
              <label className="form-label">Matcher Type</label>
              <select value={rule.matcher.type} onChange={e => handleMatcherTypeChange(e.target.value as MatcherDef['type'])}>
                {legalMatchersForScope(rule.scope).map(mt => (
                  <option key={mt} value={mt}>{MATCHER_TYPE_LABELS[mt]}</option>
                ))}
              </select>
            </div>

            <MatcherFields matcher={rule.matcher} onChange={m => onUpdate({ matcher: m })} scope={rule.scope} />

            {/* Filter-only fields */}
            {filter && (
              <>
                <div className="form-field">
                  <label className="form-label">Mode</label>
                  <select value={filter.mode} onChange={e => onUpdate({ mode: e.target.value as Mode })}>
                    <option value="blacklist">Blacklist — drop if matched</option>
                    <option value="whitelistall">Whitelist All — drop if NOT matched</option>
                    <option value="whitelistany">Whitelist Any — drop if none match</option>
                  </select>
                </div>
                <div className="form-field">
                  <label className="form-label">Needed Markers (ALL)</label>
                  <MarkerPicker
                    selected={filter.needed_markers_all ?? []}
                    onChange={ids => onUpdate({ needed_markers_all: ids })}
                    markerIds={markerIds}
                  />
                </div>
                <div className="form-field">
                  <label className="form-label">Needed Markers (ANY)</label>
                  <MarkerPicker
                    selected={filter.needed_markers_any ?? []}
                    onChange={ids => onUpdate({ needed_markers_any: ids })}
                    markerIds={markerIds}
                  />
                </div>
              </>
            )}
          </div>

          <div className="rule-card__actions flex gap-1 mt-3">
            <button className="btn btn--ghost btn--sm" onClick={onMoveUp} disabled={index === 0} title="Move up"><ArrowUp size={14}/></button>
            <button className="btn btn--ghost btn--sm" onClick={onMoveDown} disabled={index === total - 1} title="Move down"><ArrowDown size={14}/></button>
            <button className="btn btn--ghost btn--sm" onClick={onDuplicate} title="Duplicate"><CopyPlus size={14}/></button>
            <button className="btn btn--ghost btn--sm" onClick={() => { navigator.clipboard.writeText(JSON.stringify(rule, null, 2)); }} title="Copy JSON">
              <FileJson size={14}/>
            </button>
            <button className="btn btn--ghost btn--sm" onClick={onRemove} style={{ color: 'var(--color-error)' }} title="Delete"><Trash2 size={14}/></button>
          </div>
        </div>
      )}
    </div>
  );
}

/* ───── Matcher Fields ───── */

function MatcherFields({ matcher, onChange, scope }: { matcher: MatcherDef; onChange: (m: MatcherDef) => void; scope: Scope }) {
  switch (matcher.type) {
    case 'stream_regex':
    case 'path_regex':
    case 'body_regex':
    case 'method_regex':
    case 'status_regex':
      return (
        <>
          <div className="form-field">
            <label className="form-label">Pattern (regex)</label>
            <input
              value={matcher.pattern}
              onChange={e => onChange({ ...matcher, pattern: e.target.value })}
              placeholder="Regular expression"
              style={{ fontFamily: 'var(--font-mono)' }}
            />
          </div>
          <label className="form-checkbox">
            <input type="checkbox" checked={matcher.case_sensitive ?? false} onChange={e => onChange({ ...matcher, case_sensitive: e.target.checked })} />
            Case sensitive
          </label>
        </>
      );
    case 'header_regex':
      return (
        <>
          <div className="form-field">
            <label className="form-label">Header Name</label>
            <input value={matcher.header_name} onChange={e => onChange({ ...matcher, header_name: e.target.value })} placeholder="e.g. Content-Type" />
          </div>
          <div className="form-field">
            <label className="form-label">Pattern (regex)</label>
            <input value={matcher.pattern} onChange={e => onChange({ ...matcher, pattern: e.target.value })} placeholder="Regular expression" style={{ fontFamily: 'var(--font-mono)' }} />
          </div>
          <label className="form-checkbox">
            <input type="checkbox" checked={matcher.case_sensitive ?? false} onChange={e => onChange({ ...matcher, case_sensitive: e.target.checked })} />
            Case sensitive
          </label>
        </>
      );
    case 'size':
      return (
        <>
          <div className="form-field" style={{ display: 'flex', gap: 'var(--space-2)', alignItems: 'flex-end' }}>
            <div>
              <label className="form-label">Operator</label>
              <select value={matcher.op} onChange={e => onChange({ ...matcher, op: e.target.value as CmpOp })}>
                {(['<', '<=', '==', '!=', '>=', '>'] as CmpOp[]).map(op => <option key={op} value={op}>{op}</option>)}
              </select>
            </div>
            <div>
              <label className="form-label">Value (bytes)</label>
              <input type="number" value={matcher.value} onChange={e => onChange({ ...matcher, value: parseInt(e.target.value) || 0 })} min={0} />
            </div>
          </div>
          {scope === 'http.header' && (
            <div className="form-field">
              <label className="form-label">Field Name (header scope only)</label>
              <input value={matcher.field_name ?? ''} onChange={e => onChange({ ...matcher, field_name: e.target.value || null })} placeholder="Header name" />
            </div>
          )}
        </>
      );
    case 'session.lifetime':
    case 'session.idle':
      return (
        <div className="form-field" style={{ display: 'flex', gap: 'var(--space-2)', alignItems: 'flex-end' }}>
          <div>
            <label className="form-label">Operator</label>
            <select value={matcher.op} onChange={e => onChange({ ...matcher, op: e.target.value as CmpOp })}>
              {(['<', '<=', '==', '!=', '>=', '>'] as CmpOp[]).map(op => <option key={op} value={op}>{op}</option>)}
            </select>
          </div>
          <div>
            <label className="form-label">Value (seconds)</label>
            <input type="number" value={matcher.value} onChange={e => onChange({ ...matcher, value: parseFloat(e.target.value) || 0 })} min={0} step={0.1} />
          </div>
        </div>
      );
    case 'stream_bytes':
    case 'body_bytes':
      return (
        <div className="form-field">
          <label className="form-label">Byte-regex pattern</label>
          <input value={matcher.pattern} onChange={e => onChange({ ...matcher, pattern: e.target.value })} placeholder="Byte regex (\\xNN for literal bytes)" style={{ fontFamily: 'var(--font-mono)' }} />
          <span className="form-hint">Unicode off; \xNN matches literal bytes. For binary payloads.</span>
        </div>
      );
    case 'bytes_hex':
      return (
        <div className="form-field">
          <label className="form-label">Hex signature</label>
          <input value={matcher.value} onChange={e => onChange({ ...matcher, value: e.target.value })} placeholder="e.g. deadbeef" style={{ fontFamily: 'var(--font-mono)' }} />
          <span className="form-hint">Hex byte sequence. Whitespace allowed.</span>
        </div>
      );
    case 'always':
      return (
        <label className="form-checkbox">
          <input type="checkbox" checked={matcher.value} onChange={e => onChange({ ...matcher, value: e.target.checked })} />
          Always match (if unchecked, never matches)
        </label>
      );
    default:
      return null;
  }
}

/* ───── Marker Picker ───── */

function MarkerPicker({ selected, onChange, markerIds }: { selected: string[]; onChange: (ids: string[]) => void; markerIds: string[] }) {
  const toggle = (id: string) => {
    if (selected.includes(id)) {
      onChange(selected.filter(s => s !== id));
    } else {
      onChange([...selected, id]);
    }
  };

  const danglingIds = selected.filter(id => !markerIds.includes(id));

  return (
    <div className="marker-picker flex flex-wrap gap-1">
      {markerIds.map(id => (
        <label key={id} className={`marker-picker__item inline-flex items-center ${selected.includes(id) ? 'marker-picker__item--selected' : ''}`}>
          <input type="checkbox" checked={selected.includes(id)} onChange={() => toggle(id)} className="sr-only" />
          {id}
        </label>
      ))}
      {danglingIds.map(id => (
        <span key={id} className="marker-picker__item inline-flex items-center marker-picker__item--dangling" title="This marker ID does not exist">
          {id} ⚠
        </span>
      ))}
      {markerIds.length === 0 && <span className="form-hint">No markers defined</span>}
    </div>
  );
}
