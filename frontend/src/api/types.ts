import type { components } from './schema';

type Schemas = components['schemas'];

// Domain Types
export type Scope = Schemas['Scope'];
export type MatcherDef = Schemas['MatcherDef'];
export type MatcherType = MatcherDef['type'];
export type RuleDirection = Schemas['RuleDirection'];
export type MarkerDef = Schemas['MarkerDef'];
export type FilterDef = Schemas['FilterDef'];
export type CmpOp = Schemas['CmpOp'];

// System Types
export type Health = Schemas['Health'];
export type EcamEvent = Schemas['EcamEvent'];
export type ServiceMetadata = Schemas['ServiceMetadata'];
export type CtrlReplyBodyServiceState = Schemas['CtrlReplyBodyServiceState'];
export type Mode = Schemas['Mode'];

// History & Rulesets
export type RuleSetRaw = Schemas['RuleSetRaw'];
export type RulesetHistoryEntry = Schemas['RulesetHistoryEntry'];

// Traffic & Records
export type RecordGroup = Schemas['RecordGroup'];
export type EvaluationRecord = Schemas['EvaluationRecord'];
export type EvaluationDetail = Schemas['EvaluationDetail'];
