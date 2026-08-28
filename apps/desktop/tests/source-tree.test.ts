import assert from "node:assert/strict";
import test from "node:test";
import { buildSourceTree } from "../src/source-tree";

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