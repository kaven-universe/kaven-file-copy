import assert from "node:assert/strict";
import test from "node:test";
import {
  buildSourceTree,
  filterSourceEntries,
  hasPathDescendantOverride,
  isPathIncluded,
  setPathIncluded,
} from "../src/source-tree";

test("buildSourceTree groups entries by every path segment", () => {
  const tree = buildSourceTree([
    { relativePath: "projects/web/index.html", directory: false },
    { relativePath: "README.md", directory: false },
    { relativePath: "projects/api/src/main.rs", directory: false },
    { relativePath: "projects", directory: true },
    { relativePath: "projects/web", directory: true },
    { relativePath: "projects/api", directory: true },
  ]);

  assert.deepEqual(tree.map((node) => node.name), ["projects", "README.md"]);
  assert.deepEqual(tree[0].children.map((node) => node.name), ["api", "web"]);
  assert.equal(tree[0].children[0].children[0].name, "src");
  assert.equal(tree[0].children[0].children[0].children[0].relativePath, "projects/api/src/main.rs");
  assert.equal(tree[0].children[1].children[0].relativePath, "projects/web/index.html");
});

test("buildSourceTree normalizes separators and sorts directories before files", () => {
  const tree = buildSourceTree([
    { relativePath: "z.txt", directory: false },
    { relativePath: "folder\\child.txt", directory: false },
  ]);

  assert.equal(tree[0].name, "folder");
  assert.equal(tree[0].directory, true);
  assert.equal(tree[0].children[0].relativePath, "folder/child.txt");
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

test("source entries filter by path and file or folder type", () => {
  const entries = [
    { relativePath: "Photos", directory: true },
    { relativePath: "Photos/Beach.JPG", directory: false },
    { relativePath: "documents/report.txt", directory: false },
  ];

  assert.deepEqual(filterSourceEntries(entries, "all", " beach "), [entries[1]]);
  assert.deepEqual(filterSourceEntries(entries, "folders", "PHO"), [entries[0]]);
  assert.deepEqual(filterSourceEntries(entries, "files", "photos"), [entries[1]]);
});

test("list selection state matches parent and child overrides", () => {
  const included = ["cache"];
  const excluded = ["cache/keep"];

  assert!(isPathIncluded("cache/drop.txt", included, excluded));
  assert.equal(isPathIncluded("cache/keep/file.txt", included, excluded), false);
  assert.equal(hasPathDescendantOverride("cache", included, excluded), true);
  assert.equal(hasPathDescendantOverride("cache/keep", included, excluded), false);
});
