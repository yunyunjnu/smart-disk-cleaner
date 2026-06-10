export interface FileRecord {
  path: string;
  size: number;
  extension: string | null;
  modifiedAt: string | null;
  isEmpty: boolean;
}

export interface PathIssue {
  path: string;
  message: string;
}

export interface TypeStat {
  extension: string;
  fileCount: number;
  totalSize: number;
}

export interface AnalysisResult {
  totalFiles: number;
  totalSize: number;
  emptyFiles: FileRecord[];
  emptyDirs: string[];
  largeFiles: FileRecord[];
  temporaryFiles: FileRecord[];
  archiveFiles: FileRecord[];
  typeBreakdown: TypeStat[];
}

export type ScanModuleKind =
  | "duplicate_files"
  | "empty_files"
  | "empty_directories"
  | "large_files"
  | "temporary_files"
  | "archive_files";

export interface ScanModuleSummary {
  kind: ScanModuleKind;
  itemCount: number;
  totalSize: number;
}

export interface DuplicateGroup {
  hash: string;
  totalSize: number;
  files: FileRecord[];
  suggestedKeep: string | null;
}

export interface DedupResult {
  groups: DuplicateGroup[];
  failures: PathIssue[];
  groupCount?: number;
  truncated?: boolean;
}

export type SuggestedAction = "delete" | "keep" | "move" | "review";
export type RiskLevel = "low" | "medium" | "high";
export type AiInsightTargetKind = "file" | "directory";
export type MigrationCategory =
  | "large_files"
  | "download_archives"
  | "wechat_data"
  | "conda_package_cache"
  | "conda_environments"
  | "conda_installation";
export type MigrationSupportLevel = "one_click" | "guided" | "manual";
export type MigrationObjectKind =
  | "file"
  | "directory"
  | "app_data"
  | "package_cache"
  | "environment_store"
  | "installation_root";
export type MigrationDocSourceKind =
  | "local_rule"
  | "official_doc"
  | "local_observation"
  | "historical_case";
export type MigrationActionKind =
  | "stop_process"
  | "move_path"
  | "update_yaml_list"
  | "set_env_var"
  | "create_junction"
  | "verify_path_exists";
export type MigrationCheckpointKind = "file_content" | "env_var" | "path_state";
export type MigrationRunStatus = "dry_run" | "succeeded" | "failed" | "rolled_back";

export interface FileSuggestion {
  path: string;
  action: SuggestedAction;
  risk: RiskLevel;
  reason: string;
}

export type GovernanceTargetKind =
  | "file"
  | "directory"
  | "app_data"
  | "app_installation"
  | "registry_entry";

export type GovernanceScenarioKind =
  | "wechat_data"
  | "download_archive"
  | "cloud_sync"
  | "large_app"
  | "duplicate_file"
  | "temporary_file"
  | "empty_file"
  | "empty_directory"
  | "registry_startup"
  | "registry_path_reference"
  | "unknown";

export interface GovernanceSuggestion {
  id: string;
  targetKind: GovernanceTargetKind;
  targetPath: string;
  scenarioKind: GovernanceScenarioKind;
  title: string;
  action: SuggestedAction;
  riskLevel: RiskLevel;
  summary: string;
  reason: string;
  tags: string[];
  preCheckItems: string[];
  postCheckItems: string[];
  dryRunSupported: boolean;
  rollbackSupported: boolean;
}

export interface MigrationActionStep {
  title: string;
  detail: string;
  required: boolean;
}

export interface MigrationOpportunity {
  id: string;
  title: string;
  category: MigrationCategory;
  supportLevel: MigrationSupportLevel;
  risk: RiskLevel;
  estimatedSize: number;
  sourcePath: string;
  recommendedTargetDir: string;
  recommendedTargetPath: string;
  reason: string;
  blockedProcesses: string[];
  requiredSteps: MigrationActionStep[];
  oneClickPaths: string[];
  tags: string[];
}

export interface MigrationAdvisorOutput {
  source: string;
  summary: string;
  opportunities: MigrationOpportunity[];
  plans: MigrationPlan[];
}

export interface MigrationDocSource {
  title: string;
  kind: MigrationDocSourceKind;
  uri: string | null;
  note: string;
}

export interface MigrationDocExcerpt {
  title: string;
  kind: MigrationDocSourceKind;
  uri: string | null;
  excerpt: string;
}

export interface MigrationAction {
  id: string;
  kind: MigrationActionKind;
  title: string;
  detail: string;
  required: boolean;
  enabledByDefault: boolean;
  params: Record<string, unknown>;
}

export interface MigrationCheckpoint {
  key: string;
  kind: MigrationCheckpointKind;
  target: string;
  snapshot: Record<string, unknown> | string | null;
}

export interface MigrationRollbackAction {
  id: string;
  kind: MigrationActionKind;
  title: string;
  detail: string;
  params: Record<string, unknown>;
}

export interface MigrationPlan {
  id: string;
  title: string;
  category: MigrationCategory;
  objectKind: MigrationObjectKind;
  supportLevel: MigrationSupportLevel;
  risk: RiskLevel;
  estimatedSize: number;
  sourcePath: string;
  recommendedTargetDir: string;
  recommendedTargetPath: string;
  summary: string;
  rationale: string;
  tags: string[];
  docSources: MigrationDocSource[];
  actions: MigrationAction[];
  verificationSteps: MigrationActionStep[];
}

export interface MigrationHistoricalCase {
  runId: string;
  title: string;
  status: MigrationRunStatus;
  dryRun: boolean;
  actionTitles: string[];
  failureReason: string | null;
}

export interface MigrationExecutionRecord {
  runId: string;
  planId: string;
  title: string;
  startedAt: string;
  finishedAt: string;
  dryRun: boolean;
  status: MigrationRunStatus;
  checkpoints: MigrationCheckpoint[];
  rollbackActions: MigrationRollbackAction[];
  logs: OperationLogEntry[];
  failureReason: string | null;
}

export interface MigrationPlanReview {
  plan: MigrationPlan;
  source: string;
  remoteAttempted: boolean;
  usedFallback: boolean;
  fallbackReason: string | null;
  docExcerpts: MigrationDocExcerpt[];
  historicalCases: MigrationHistoricalCase[];
}

export interface AdvisorOutput {
  source: string;
  summary: string;
  suggestions: FileSuggestion[];
  governanceSuggestions: GovernanceSuggestion[];
  suggestionCount?: number;
  truncated?: boolean;
}

export interface FileAiInsight {
  path: string;
  targetKind: AiInsightTargetKind;
  source: string;
  summary: string;
  suggestedAction: SuggestedAction;
  risk: RiskLevel;
  reason: string;
  remoteAttempted: boolean;
  usedFallback: boolean;
  fallbackReason: string | null;
}

export type ProcessSuggestedAction =
  | "safe_to_end"
  | "end_after_save"
  | "review"
  | "avoid_end";

export interface ProcessRecord {
  pid: number;
  parentPid: number | null;
  name: string;
  exePath: string | null;
  command: string[];
  cpuUsage: number;
  memoryBytes: number;
  virtualMemoryBytes: number;
  diskReadBytes: number;
  diskWrittenBytes: number;
  runTimeSeconds: number;
  status: string;
  category: string;
  isCritical: boolean;
  resourceScore: number;
}

export interface ProcessAiInsight {
  pid: number;
  name: string;
  source: string;
  summary: string;
  suggestedAction: ProcessSuggestedAction;
  risk: RiskLevel;
  reason: string;
  remoteAttempted: boolean;
  usedFallback: boolean;
  fallbackReason: string | null;
}

export interface ProcessAiFollowUpAnswer {
  pid: number;
  name: string;
  question: string;
  answer: string;
  source: string;
  remoteAttempted: boolean;
  usedFallback: boolean;
  fallbackReason: string | null;
}

export interface ProcessAiFollowUpTurn {
  question: string;
  answer: string;
}

export interface ProcessMonitorSnapshot {
  collectedAt: string;
  systemCpuUsage: number;
  memoryUsedBytes: number;
  memoryTotalBytes: number;
  diskBytesPerSec: number;
  topProcesses: ProcessRecord[];
}

export interface ScanReport {
  generatedAt: string;
  scanDurationMs: number;
  root: string;
  scannedFiles: FileRecord[];
  analysis: AnalysisResult;
  dedup: DedupResult;
  modules: ScanModuleSummary[];
  advisor: AdvisorOutput;
  failures: PathIssue[];
  dedupPending?: boolean;
  dedupPhase?: string | null;
  dedupMessage?: string | null;
  dedupError?: string | null;
}

export interface FileTreeRow {
  key: string;
  name: string;
  path: string;
  kind: "directory" | "file";
  size: number;
  extension: string;
  fileCount: number;
  children?: FileTreeRow[];
}

export interface DirectoryOverviewRow {
  key: string;
  name: string;
  path: string;
  kind: "directory" | "file";
  fileCount: number;
  totalSize: number;
  preview: string;
}

export interface AppOverviewRow {
  key: string;
  appName: string;
  vendor: string;
  category: string;
  sourceSummary: string;
  statusTags: string[];
  iconDataUri: string;
  iconSource: string;
  detectedRoot: string;
  fileCount: number;
  totalSize: number;
  samplePaths: string[];
}

export interface AiCleanupAppCandidate {
  key: string;
  appName: string;
  publisher: string | null;
  installLocation: string | null;
  estimatedSize: number;
  reason: string;
  uninstallAvailable: boolean;
}

export interface AiCleanupPlan {
  source: string;
  summary: string;
  fileSuggestions: FileSuggestion[];
  uninstallCandidates: AiCleanupAppCandidate[];
}

export interface FileTreeQueryResult {
  matchedCount: number;
  nodeCount: number;
  truncated: boolean;
  rows: FileTreeRow[];
}

export interface LoadChildrenRequest {
  dirPath: string;
}

export type ExecutionMode = "recycle" | "move";
export type OperationRecordKind = "file_cleanup" | "migration" | "registry_change" | "registry_rollback";

export type DiagnosticSeverity = "info" | "warning" | "critical";
export type DiagnosticCode =
  | "ok"
  | "not_found"
  | "permission_denied"
  | "in_use_by_another_process"
  | "locked_region"
  | "read_only"
  | "already_exists"
  | "invalid_input"
  | "directory_not_empty"
  | "unsupported"
  | "unknown";

export interface PathDiagnosis {
  path: string;
  operation: string;
  code: DiagnosticCode;
  severity: DiagnosticSeverity;
  summary: string;
  details: string[];
  suggestions: string[];
  possibleRelatedApps: string[];
  errorKind: string | null;
  rawOsError: number | null;
}

export interface OperationLogEntry {
  at: string;
  path: string;
  mode: ExecutionMode;
  dryRun: boolean;
  success: boolean;
  detail: string;
  recordKind: OperationRecordKind;
  reason: string | null;
  rollbackReference: string | null;
  diagnosis: PathDiagnosis | null;
}

export interface BlockingProcessInfo {
  pid: number;
  appName: string;
  serviceName: string | null;
  restartable: boolean;
  processStartTime: number;
}

export interface AppConfig {
  largeFileThresholdMb: number;
  maxAiItems: number;
  apiKey: string | null;
  aiBaseUrl: string;
  aiModel: string;
  strictFileAiRemoteOnly: boolean;
  excludePatterns: string[];
  theme: string;
}

export type RegistryEntryKind = "startup" | "uninstall" | "path_reference";
export type RegistryRootKey = "hkcu" | "hklm";
export type RegistryValueDataKind = "string" | "expand_string" | "unknown";
export type RegistryIssueKind = "missing_target" | "missing_quoted_path" | "startup_entry_enabled";

export interface RegistryEntryRecord {
  id: string;
  rootKey: RegistryRootKey;
  subKey: string;
  valueName: string;
  valueKind: RegistryValueDataKind;
  valueData: string;
  entryKind: RegistryEntryKind;
  displayName: string;
  targetPath: string | null;
  existsOnDisk: boolean;
  safeToModify: boolean;
  riskLevel: RiskLevel;
  tags: string[];
}

export interface RegistryIssue {
  id: string;
  entryId: string;
  issueKind: RegistryIssueKind;
  title: string;
  summary: string;
  riskLevel: RiskLevel;
  suggestedAction: SuggestedAction;
  preCheckItems: string[];
  postCheckItems: string[];
}

export interface RegistryBackup {
  backupId: string;
  entry: RegistryEntryRecord;
  originalValueData: string;
  createdAt: string;
}

export interface RegistryChangePreview {
  entry: RegistryEntryRecord;
  riskLevel: RiskLevel;
  backupId: string | null;
  dryRunSupported: boolean;
  rollbackSupported: boolean;
  beforeValue: string;
  afterValue: string;
  postCheckItems: string[];
}

export interface RegistryRollbackRecord {
  backupId: string;
  restoredAt: string;
  entry: RegistryEntryRecord;
}

// Progress event types
export interface ScanProgressEvent {
  kind: "Scan";
  phase: string;
  filesFound: number;
  dirsVisited: number;
  bytesFound: number;
  currentPath: string | null;
}

export interface DedupProgressEvent {
  kind: "Dedup";
  phase: string;
  filesHashed: number;
  filesTotal: number;
  currentPath: string | null;
}

export interface AnalyzeProgressEvent {
  kind: "Analyze";
  phase: string;
  detail: string;
}

export type ProgressEvent =
  | ScanProgressEvent
  | DedupProgressEvent
  | AnalyzeProgressEvent;


export interface RawNode {
  name: string;
  value: number;
  path: string[];
  kind: "dir"|"file";
  load?: "undo"|"wait"|"done";
  hidden: boolean;
  chioced?: boolean;
  itemStyle?: any;
  upperLable?:any;
  children?: RawNode[];
  aiStatu?: "undo"|"wait"|"done",
  aiBrief?: string,
  aiRank?: "low" | "medium" | "high",
  aiReson?:string,
}