import { invoke } from "@tauri-apps/api/core";
import type { CopyPlan, CopyRule, JobEvent, JobSummary, RecentPaths, SourceEntry } from "../types/generated";

export const api = {
  listRules: () => invoke<CopyRule[]>("list_rules"),
  getRecentPaths: () => invoke<RecentPaths>("get_recent_paths"),
  rememberRecentPath: (kind: "source" | "destination", path: string) => invoke<RecentPaths>("remember_recent_path", { kind, path }),
  saveRule: (rule: CopyRule) => invoke<void>("save_rule", { rule }),
  deleteRule: (id: string) => invoke<void>("delete_rule", { id }),
  validateRule: (rule: CopyRule) => invoke<void>("validate_rule", { rule }),
  previewRule: (rule: CopyRule) => invoke<CopyPlan>("preview_rule", { rule }),
  executeRule: (rule: CopyRule) => invoke<JobSummary>("execute_rule", { rule }),
  cancelJob: () => invoke<void>("cancel_job"),
  listJobs: () => invoke<JobSummary[]>("list_jobs"),
  getJobEvents: (id: string) => invoke<JobEvent[]>("get_job_events", { id }),
  listSourceEntries: (rule: CopyRule) => invoke<SourceEntry[]>("list_source_entries", { rule }),
};
