import assert from "node:assert/strict";
import test from "node:test";
import { filterPlanItems, sortPreviewItems } from "../src/preview-filter";
import type { PlanItem } from "../src/types/generated";

const items: PlanItem[] = [
  { action: "copy-file", relativePath: "photos/Beach.JPG", size: 12, reason: "destination file does not exist" },
  { action: "create-directory", relativePath: "photos/archive", size: 0, reason: "destination directory does not exist" },
  { action: "skip", relativePath: "documents/report.txt", size: 4, reason: "source and destination metadata match" },
  { action: "warn", relativePath: "links/shared", size: 0, reason: "symbolic link skipped by policy" },
];

test("preview filter combines action and case-insensitive path search", () => {
  assert.deepEqual(
    filterPlanItems(items, "copy-file", " beach "),
    [items[0]],
  );
  assert.deepEqual(filterPlanItems(items, "skip", "beach"), []);
});

test("preview filter searches reasons and returns all items for blank filters", () => {
  assert.deepEqual(filterPlanItems(items, "all", "SYMBOLIC"), [items[3]]);
  assert.deepEqual(filterPlanItems(items, "all", "   "), items);
});

test("preview items sort by action, source, target, and description", () => {
  const entries: PlanItem[] = [
    { action: "copy-file", relativePath: "m.txt", destinationPath: "a_renamed.txt", size: 1, reason: "z reason" },
    { action: "copy-file", relativePath: "a.txt", destinationPath: "z_renamed.txt", size: 1, reason: "a reason" },
    { action: "skip", relativePath: "c.txt", size: 1, reason: "m reason" },
  ];

  const bySource = sortPreviewItems(entries, "source", "asc");
  assert.deepEqual(bySource.map((item) => item.relativePath), ["a.txt", "c.txt", "m.txt"]);

  const byAction = sortPreviewItems(entries, "action", "asc");
  assert.deepEqual(byAction.map((item) => item.relativePath), ["a.txt", "m.txt", "c.txt"]);

  const byTarget = sortPreviewItems(entries, "target", "asc");
  assert.deepEqual(byTarget.map((item) => item.relativePath), ["m.txt", "c.txt", "a.txt"]);

  const byDescription = sortPreviewItems(entries, "description", "asc");
  assert.deepEqual(byDescription.map((item) => item.relativePath), ["a.txt", "c.txt", "m.txt"]);

  const bySourceDesc = sortPreviewItems(entries, "source", "desc");
  assert.deepEqual(bySourceDesc.map((item) => item.relativePath), ["m.txt", "c.txt", "a.txt"]);
});