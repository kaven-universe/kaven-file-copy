import assert from "node:assert/strict";
import test from "node:test";
import { buildSourceTree, setPathExcluded } from "../src/source-tree";

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