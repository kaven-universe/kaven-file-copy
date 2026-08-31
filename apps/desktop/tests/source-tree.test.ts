import assert from "node:assert/strict";
import test from "node:test";
import {
  buildSourceTree,
  filterSourceEntries,
  formatBytes,
  formatModified,
  hasPathDescendantOverride,
  isPathIncluded,
  setPathIncluded,
  sortSourceEntries,
} from "../src/source-tree";

test("buildSourceTree groups entries by every path segment", () => {
  const tree = buildSourceTree([
    { relativePath: "projects/web/index.html", directory: false, size: 20, modified: 1 },
    { relativePath: "README.md", directory: false, size: 10, modified: 2 },
    { relativePath: "projects/api/src/main.rs", directory: false, size: 30, modified: 3 },
    { relativePath: "projects", directory: true, size: 0, modified: 4 },
    { relativePath: "projects/web", directory: true, size: 0, modified: 5 },
    { relativePath: "projects/api", directory: true, size: 0, modified: 6 },
  ]);

  assert.deepEqual(tree.map((node) => node.name), ["projects", "README.md"]);
  assert.deepEqual(tree[0].children.map((node) => node.name), ["api", "web"]);
  assert.equal(tree[0].children[0].children[0].name, "src");
  assert.equal(tree[0].children[0].children[0].children[0].relativePath, "projects/api/src/main.rs");
  assert.equal(tree[0].children[1].children[0].relativePath, "projects/web/index.html");
});

test("buildSourceTree carries size and modified through implicit directories", () => {
  const tree = buildSourceTree([
    { relativePath: "folder\\child.txt", directory: false, size: 42, modified: 1_700_000_000_000 },
  ]);

  assert.equal(tree[0].directory, true);
  assert.equal(tree[0].size, 0);
  assert.equal(tree[0].modified, null);
  assert.equal(tree[0].children[0].size, 42);
  assert.equal(tree[0].children[0].modified, 1_700_000_000_000);
});

test("source entries sort by name, size, and modified", () => {
  const entries = [
    { relativePath: "b.txt", directory: false, size: 300, modified: 1 },
    { relativePath: "a.txt", directory: false, size: 100, modified: 3 },
    { relativePath: "c.txt", directory: false, size: 200, modified: 2 },
  ];

  const bySizeAsc = sortSourceEntries(entries, { key: "size", direction: "asc" });
  assert.deepEqual(bySizeAsc.map((entry) => entry.relativePath), ["a.txt", "c.txt", "b.txt"]);

  const bySizeDesc = sortSourceEntries(entries, { key: "size", direction: "desc" });
  assert.deepEqual(bySizeDesc.map((entry) => entry.relativePath), ["b.txt", "c.txt", "a.txt"]);

  const byModifiedAsc = sortSourceEntries(entries, { key: "modified", direction: "asc" });
  assert.deepEqual(byModifiedAsc.map((entry) => entry.relativePath), ["b.txt", "c.txt", "a.txt"]);

  const byNameDesc = sortSourceEntries(entries, { key: "name", direction: "desc" });
  assert.deepEqual(byNameDesc.map((entry) => entry.relativePath), ["c.txt", "b.txt", "a.txt"]);
});

test("an empty selection includes nothing", () => {
  assert.equal(isPathIncluded("cache", [], []), false);
  assert.equal(isPathIncluded("README.md", [], []), false);
});

test("a child can be deselected under a selected parent and selected again", () => {
  const parentSelected = setPathIncluded([], [], "cache", true);
  assert.deepEqual(parentSelected, { included: ["cache"], excluded: [] });

  const childDeselected = setPathIncluded(
    parentSelected.included,
    parentSelected.excluded,
    "cache/keep",
    false,
  );
  assert.deepEqual(childDeselected, { included: ["cache"], excluded: ["cache/keep"] });

  const childSelectedAgain = setPathIncluded(
    childDeselected.included,
    childDeselected.excluded,
    "cache/keep",
    true,
  );
  assert.deepEqual(childSelectedAgain, { included: ["cache"], excluded: [] });
});

test("a deeper selection can override a deselected child", () => {
  const state = setPathIncluded(
    ["cache"],
    ["cache/keep"],
    "cache/keep/private",
    true,
  );
  assert.deepEqual(state, {
    included: ["cache", "cache/keep/private"],
    excluded: ["cache/keep"],
  });
});

test("source entries filter by path", () => {
  const entries = [
    { relativePath: "Photos", directory: true, size: 0, modified: 1 },
    { relativePath: "Photos/Beach.JPG", directory: false, size: 100, modified: 2 },
    { relativePath: "documents/report.txt", directory: false, size: 200, modified: 3 },
  ];

  assert.deepEqual(filterSourceEntries(entries, " beach "), [entries[1]]);
  assert.deepEqual(filterSourceEntries(entries, "photos"), [entries[0], entries[1]]);
  assert.deepEqual(filterSourceEntries(entries, ""), entries);
});

test("list selection state matches parent and child overrides", () => {
  const included = ["cache"];
  const excluded = ["cache/keep"];

  assert!(isPathIncluded("cache/drop.txt", included, excluded));
  assert.equal(isPathIncluded("cache/keep/file.txt", included, excluded), false);
  assert.equal(hasPathDescendantOverride("cache", included, excluded), true);
  assert.equal(hasPathDescendantOverride("cache/keep", included, excluded), false);
});

test("formatBytes renders human readable sizes", () => {
  assert.equal(formatBytes(0), "0 B");
  assert.equal(formatBytes(1023), "1023 B");
  assert.equal(formatBytes(1024), "1.0 KB");
  assert.equal(formatBytes(5 * 1024 * 1024), "5.0 MB");
  assert.equal(formatBytes(null), "—");
  assert.equal(formatBytes(undefined), "—");
});

test("formatModified renders a local timestamp or placeholder", () => {
  assert.equal(formatModified(null), "—");
  assert.equal(formatModified(undefined), "—");
  assert.match(formatModified(0), /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
});
