// Contract mirror for schemaVersion 1. Replace with generated output once the schema generator is wired into CI.
export type EndpointConfig = { type: "local"; path: string };
export type TriggerConfig =
  | { type: "manual" }
  | { type: "watch"; debounceMs: number }
  | { type: "interval"; seconds: number }
  | { type: "cron"; expression: string; timezone?: string };

export interface CopyRule {
  schemaVersion: 1;
  id: string;
  name: string;
  enabled: boolean;
  source: EndpointConfig;
  destination: EndpointConfig;
  filters: { include: string[]; exclude: string[]; includeHidden: boolean };
  copy: {
    mode: "incremental";
    compare: "size-and-mtime" | "size-only";
    overwrite: "if-changed" | "never" | "always";
    symlinks: "skip";
    preserveMtime: boolean;
    maxConcurrency: number;
    retryCount: number;
  };
  trigger: TriggerConfig;
}

export interface PlanStats {
  directoriesToCreate: number;
  filesToCopy: number;
  filesToSkip: number;
  warnings: number;
  bytesToCopy: number;
}

export type PlanAction = "create-directory" | "copy-file" | "skip" | "warn";
export interface PlanItem { action: PlanAction; relativePath: string; size: number; reason: string }
export interface CopyPlan { ruleId: string; items: PlanItem[]; stats: PlanStats }
export interface PlanProgressEvent {
  phase: "scanning" | "planning";
  completedEntries: number;
  totalEntries: number | null;
  relativePath: string | null;
}
export interface JobSummary {
  jobId: string; ruleId: string; startedAt: string; finishedAt: string;
  status: "succeeded" | "partial" | "failed" | "cancelled";
  copiedFiles: number; skippedFiles: number; failedFiles: number; copiedBytes: number;
  errors: Array<{ relativePath: string; message: string }>;
}
export interface ProgressEvent {
  jobId: string; relativePath: string; completedFiles: number; totalFiles: number;
  copiedBytes: number; totalBytes: number;
}
export interface JobEvent {
  timestamp: string; level: string; phase: string; relativePath?: string;
  operation: string; result: string; message?: string;
}
export interface SourceEntry {
  relativePath: string;
  directory: boolean;
  size: number;
  modified: number | null;
}
export interface RecentPaths { sources: string[]; destinations: string[] }
