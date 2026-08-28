import assert from "node:assert/strict";
import test from "node:test";
import {
  buildSourceTree,
  filterSourceEntries,
  hasPathDescendantOverride,
  isPathExcluded,
  setPathExcluded,
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

test("a child can override an excluded parent and be checked again", () => {
  const parentExcluded = setPathExcluded([], [], "cache", true);
  assert.deepEqual(parentExcluded, { excluded: ["cache"], included: [] });

  const childIncluded = setPathExcluded(
    parentExcluded.excluded,
    parentExcluded.included,
    "cache/keep",
    false,
  );
  assert.deepEqual(childIncluded, { excluded: ["cache"], included: ["cache/keep"] });

  const childExcludedAgain = setPathExcluded(
    childIncluded.excluded,
    childIncluded.included,
    "cache/keep",
    true,
  );
  assert.deepEqual(childExcludedAgain, { excluded: ["cache"], included: [] });
});

test("a deeper exclusion can override an included child", () => {
  const state = setPathExcluded(
    ["cache"],
    ["cache/keep"],
    "cache/keep/private",
    true,
  );
  assert.deepEqual(state, {
    excluded: ["cache", "cache/keep/private"],
    included: ["cache/keep"],
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

test("list exclusion state matches parent and child overrides", () => {
  const excluded = ["cache"];
  const included = ["cache/keep"];

  assert!(isPathExcluded("cache/drop.txt", excluded, included));
  assert.equal(isPathExcluded("cache/keep/file.txt", excluded, included), false);
  assert.equal(hasPathDescendantOverride("cache", excluded, included), true);
  assert.equal(hasPathDescendantOverride("cache/keep", excluded, included), false);
});