import assert from "node:assert/strict";
import test from "node:test";
import { buildPreviewTree, filterPlanItems } from "../src/preview-filter";
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

test("preview tree groups filtered plan items by path segment", () => {
  const tree = buildPreviewTree(items);

  assert.deepEqual(tree.map((node) => node.name), ["documents", "links", "photos"]);
  assert.equal(tree[0].children[0].name, "report.txt");
  assert.equal(tree[0].children[0].item, items[2]);
  assert.deepEqual(tree[2].children.map((node) => node.name), ["archive", "Beach.JPG"]);
  assert.equal(tree[2].children[0].directory, true);
  assert.equal(tree[2].children[0].item, items[1]);
});

test("preview tree normalizes separators and creates implicit directories", () => {
  const [root] = buildPreviewTree([
    { action: "copy-file", relativePath: "src\\components\\App.vue", size: 10, reason: "changed" },
  ]);

  assert.equal(root.relativePath, "src");
  assert.equal(root.children[0].relativePath, "src/components");
  assert.equal(root.children[0].children[0].relativePath, "src/components/App.vue");
});