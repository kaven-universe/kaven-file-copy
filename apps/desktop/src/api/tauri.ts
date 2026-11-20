import { invoke } from "@tauri-apps/api/core";
import type { CopyPlan, CopyRule, JobEvent, JobSummary, RecentPaths, SourceEntry } from "../types/generated";

export interface UpdateInfo {
  version: string;
  notes: string | null;
  publishedAt: string | null;
}

export const api = {
  exitApp: () => invoke<void>("exit_app"),
  checkForUpdates: () => invoke<UpdateInfo | null>("check_for_updates"),
  installUpdate: () => invoke<void>("install_update"),
  listRules: () => invoke<CopyRule[]>("list_rules"),
  getRecentPaths: () => invoke<RecentPaths>("get_recent_paths"),
  setLocale: (locale: "en" | "zh") => invoke<void>("set_locale", { locale }),
  rememberRecentPath: (kind: "source" | "destination", path: string) => invoke<RecentPaths>("remember_recent_path", { kind, path }),
  saveRule: (rule: CopyRule) => invoke<void>("save_rule", { rule }),
  deleteRule: (id: string) => invoke<void>("delete_rule", { id }),
  validateRule: (rule: CopyRule) => invoke<void>("validate_rule", { rule }),
  previewRule: (rule: CopyRule) => invoke<CopyPlan>("preview_rule", { rule }),
  executeRule: (rule: CopyRule) => invoke<JobSummary>("execute_rule", { rule }),
  cancelJob: () => invoke<void>("cancel_job"),
  listJobs: () => invoke<JobSummary[]>("list_jobs"),
  getJobEvents: (id: string) => invoke<JobEvent[]>("get_job_events", { id }),
  deleteJob: (id: string) => invoke<void>("delete_job", { id }),
  deleteAllJobs: () => invoke<void>("delete_all_jobs"),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  listSourceEntries: (rule: CopyRule) => invoke<SourceEntry[]>("list_source_entries", { rule }),
};
