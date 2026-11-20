import { test } from "node:test";
import assert from "node:assert/strict";
import { buildRunDetails } from "../src/run-results";
import type { JobEvent, JobSummary } from "../src/types/generated";

const summary: JobSummary = {
  jobId: "job-1",
  ruleId: "rule-1",
  startedAt: "2026-11-19T00:00:00Z",
  finishedAt: "2026-11-19T00:00:01Z",
  status: "partial",
  copiedFiles: 1,
  skippedFiles: 1,
  failedFiles: 1,
  copiedBytes: 4,
  errors: [{ relativePath: "failed.txt", message: "access denied" }],
};

const event = (relativePath: string, result: string, message?: string): JobEvent => ({
  timestamp: "2026-11-19T00:00:01Z",
  level: result === "failed" ? "error" : "info",
  phase: "finished",
  relativePath,
  operation: "copy-file",
  result,
  message,
});

test("buildRunDetails lists copied, skipped, failed, and warning outcomes", () => {
  const events = [
    event("copied.txt", "processed"),
    event("failed.txt", "processed"),
    event("skipped.txt", "skipped", "unchanged"),
    event("warning.txt", "warning", "name kept"),
    event("failed.txt", "failed", "access denied"),
  ];

  assert.deepEqual(buildRunDetails(summary, events), [
    { result: "copied", relativePath: "copied.txt" },
    { result: "skipped", relativePath: "skipped.txt", message: "unchanged" },
    { result: "warning", relativePath: "warning.txt", message: "name kept" },
    { result: "failed", relativePath: "failed.txt", message: "access denied" },
  ]);
});

test("buildRunDetails includes summary failures when the event log is incomplete", () => {
  assert.deepEqual(buildRunDetails(summary, []), [
    { result: "failed", relativePath: "failed.txt", message: "access denied" },
  ]);
});
