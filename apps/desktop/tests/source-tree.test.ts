import assert from "node:assert/strict";
import test from "node:test";
import {
  applyRename,
  buildGroupTree,
  buildSourceTree,
  compareVersions,
  filterSourceEntries,
  formatBytes,
  formatModified,
  hasPathDescendantOverride,
  initialGroupRunSelection,
  isPathIncluded,
  latestGroupKey,
  limitGroupSelection,
  selectAllGroups,
  setPathIncluded,
  sortSourceEntries,
  toggleGroupSelection,
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

test("buildGroupTree groups top-level files by extracted version and sorts newest first", () => {
  const entries = [
    { relativePath: "DSP_2.0.0.0_Release.exe", directory: false, size: 1, modified: 1 },
    { relativePath: "DSP_1.0.0.2_Source.zip", directory: false, size: 2, modified: 2 },
    { relativePath: "DSP_1.0.0.10_Release.exe", directory: false, size: 3, modified: 3 },
    { relativePath: "DSP_1.0.0.1_Release.exe", directory: false, size: 4, modified: 4 },
    { relativePath: "nested/DSP_1.0.0.1_Extra.exe", directory: false, size: 5, modified: 5 },
    { relativePath: "docs", directory: true, size: 0, modified: 6 },
  ];
  const group = { label: "Version", pattern: "(\\d+\\.\\d+\\.\\d+\\.\\d+)" };

  const tree = buildGroupTree(entries, group, ["1.0.0.1"], "");

  assert.deepEqual(tree.map((node) => node.key), ["2.0.0.0", "1.0.0.10", "1.0.0.2", "1.0.0.1"]);
  assert.deepEqual(
    tree[3].matched.map((file) => file.relativePath),
    ["DSP_1.0.0.1_Release.exe"],
  );
  assert.equal(tree[3].selected, true);
});

test("compareVersions sorts dotted versions numerically", () => {
  const versions = ["1.0.0.10", "1.0.0.2", "2.0.0.0", "1.0.0.1", "1.1.0.0"];
  assert.deepEqual(versions.sort(compareVersions), ["1.0.0.1", "1.0.0.2", "1.0.0.10", "1.1.0.0", "2.0.0.0"]);
});

test("applyRename rewrites file names with capture groups", () => {
  const pattern = "^(.+)_(\\d+\\.\\d+\\.\\d+\\.\\d+)_(.+)\\.(.+)$";
  assert.equal(applyRename(pattern, "$1_$2.$4", "DSP_1.0.0.1_Release.exe"), "DSP_1.0.0.1.exe");
  assert.equal(applyRename(pattern, "$1_$2.$4", "readme.txt"), null);
  assert.equal(applyRename("^(.+)$", "prefix_$1", "a.txt"), "prefix_a.txt");
  assert.equal(applyRename("[", "$1", "a.txt"), null);
});

test("buildGroupTree puts unmatched files in the other group and filters by query", () => {
  const entries = [
    { relativePath: "DSP_1.0.0.1_Release.exe", directory: false, size: 1, modified: 1 },
    { relativePath: "readme.md", directory: false, size: 2, modified: 2 },
  ];
  const group = { label: "", pattern: "(\\d+\\.\\d+\\.\\d+\\.\\d+)" };

  const tree = buildGroupTree(entries, group, [], "");
  assert.deepEqual(tree.map((node) => node.key), ["1.0.0.1", ""]);
  assert.deepEqual(tree[1].matched.map((file) => file.relativePath), ["readme.md"]);
  assert.equal(tree[1].label, "Other files");

  const filtered = buildGroupTree(entries, group, [], "readme");
  assert.deepEqual(filtered.map((node) => node.key), [""]);
});

test("latestGroupKey returns the newest version among top-level files", () => {
  const entries = [
    { relativePath: "DSP_1.0.0.1_Release.exe", directory: false, size: 1, modified: 1 },
    { relativePath: "DSP_1.0.0.10_Release.exe", directory: false, size: 2, modified: 2 },
    { relativePath: "DSP_2.3.1.0_Release.exe", directory: false, size: 3, modified: 3 },
    { relativePath: "nested/DSP_9.9.9.9_Release.exe", directory: false, size: 4, modified: 4 },
    { relativePath: "readme.md", directory: false, size: 5, modified: 5 },
  ];
  const group = { label: "", pattern: "(\\d+\\.\\d+\\.\\d+\\.\\d+)" };

  assert.equal(latestGroupKey(entries, group), "2.3.1.0");
  assert.equal(latestGroupKey(entries, null), null);
  assert.equal(latestGroupKey([{ relativePath: "readme.md", directory: false, size: 1, modified: 1 }], group), null);
});

test("buildGroupTree tolerates invalid regex patterns and missing rules", () => {
  const entries = [{ relativePath: "DSP_1.0.0.1_Release.exe", directory: false, size: 1, modified: 1 }];

  const broken = buildGroupTree(entries, { label: "", pattern: "[" }, [], "");
  assert.equal(broken.length, 1);
  assert.equal(broken[0].label, "Other files");
  assert.deepEqual(broken[0].matched.map((file) => file.relativePath), ["DSP_1.0.0.1_Release.exe"]);

  const none = buildGroupTree(entries, null, [], "");
  assert.equal(none.length, 1);
  assert.equal(none[0].label, "Other files");
});

test("group selection supports bulk selection and single-group replacement", () => {
  assert.deepEqual(selectAllGroups(["2.0", "1.0", "2.0", ""]), ["2.0", "1.0"]);
  assert.deepEqual(toggleGroupSelection(["1.0"], "2.0", true, false), ["1.0", "2.0"]);
  assert.deepEqual(toggleGroupSelection(["1.0"], "2.0", true, true), ["2.0"]);
  assert.deepEqual(toggleGroupSelection(["2.0"], "2.0", false, true), []);
  assert.deepEqual(limitGroupSelection(["2.0", "1.0"]), ["2.0"]);
});

test("run confirmation starts from valid configured groups or the newest group", () => {
  const entries = [
    { relativePath: "DSP_1.0.bin", directory: false, size: 1, modified: 1 },
    { relativePath: "DSP_2.0.bin", directory: false, size: 1, modified: 2 },
  ];
  const group = { label: "", pattern: "(\\d+\\.\\d+)" };

  assert.deepEqual(
    initialGroupRunSelection(entries, group, ["1.0", "missing"], false, false),
    ["1.0"],
  );
  assert.deepEqual(initialGroupRunSelection(entries, group, ["1.0"], true, false), ["2.0"]);
  assert.deepEqual(
    initialGroupRunSelection(entries, group, ["1.0", "2.0"], false, true),
    ["1.0"],
  );
});
