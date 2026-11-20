import type { JobEvent, JobSummary } from "./types/generated";

export type RunDetailResult = "copied" | "skipped" | "failed" | "warning";

export interface RunDetail {
  result: RunDetailResult;
  relativePath: string;
  message?: string;
}

export function buildRunDetails(summary: JobSummary, events: JobEvent[]): RunDetail[] {
  const failed = new Map(summary.errors.map((error) => [error.relativePath, error.message]));
  const details: RunDetail[] = [];
  const seen = new Set<string>();

  const add = (detail: RunDetail) => {
    const key = `${detail.result}\0${detail.relativePath}`;
    if (seen.has(key)) return;
    seen.add(key);
    details.push(detail);
  };

  for (const event of events) {
    const relativePath = event.relativePath;
    if (!relativePath) continue;

    if (event.result === "processed") {
      if (!failed.has(relativePath)) add({ result: "copied", relativePath });
    } else if (event.result === "skipped") {
      add({ result: "skipped", relativePath, message: event.message });
    } else if (event.result === "warning") {
      add({ result: "warning", relativePath, message: event.message });
    } else if (event.result === "failed") {
      add({ result: "failed", relativePath, message: event.message ?? failed.get(relativePath) });
    }
  }

  for (const error of summary.errors) {
    add({ result: "failed", relativePath: error.relativePath, message: error.message });
  }

  return details;
}
