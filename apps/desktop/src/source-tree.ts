import type { SourceEntry } from "./types/generated";

export interface SourceTreeNode extends SourceEntry {
  name: string;
  children: SourceTreeNode[];
}

export interface PathExclusionState {
  excluded: string[];
  included: string[];
}

export type SourceEntryTypeFilter = "all" | "folders" | "files";
export type SourceView = "list" | "tree";

export function filterSourceEntries(
  entries: SourceEntry[],
  type: SourceEntryTypeFilter,
  query: string,
) {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  return entries.filter((entry) => {
    if (type === "folders" && !entry.directory) return false;
    if (type === "files" && entry.directory) return false;
    return !normalizedQuery
      || normalizeTreePath(entry.relativePath).toLocaleLowerCase().includes(normalizedQuery);
  });
}

export function buildSourceTree(entries: SourceEntry[]): SourceTreeNode[] {
  const roots: SourceTreeNode[] = [];
  const nodes = new Map<string, SourceTreeNode>();

  for (const entry of entries) {
    const parts = normalizeTreePath(entry.relativePath).split("/").filter(Boolean);
    let children = roots;
    let relativePath = "";

    parts.forEach((name, index) => {
      relativePath = relativePath ? `${relativePath}/${name}` : name;
      const directory = index < parts.length - 1 || entry.directory;
      let node = nodes.get(relativePath);
      if (!node) {
        node = { name, relativePath, directory, children: [] };
        nodes.set(relativePath, node);
        children.push(node);
      } else if (directory) {
        node.directory = true;
      }
      children = node.children;
    });
  }

  sortTree(roots);
  return roots;
}

function sortTree(nodes: SourceTreeNode[]) {
  nodes.sort((left, right) => {
    if (left.directory !== right.directory) return left.directory ? -1 : 1;
    return left.name.localeCompare(right.name, undefined, { numeric: true, sensitivity: "base" });
  });
  nodes.forEach((node) => sortTree(node.children));
}

export function setPathExcluded(
  excludedPaths: string[],
  includedPaths: string[],
  path: string,
  excluded: boolean,
): PathExclusionState {
  const normalized = normalizeTreePath(path);
  const descendantPrefix = `${normalized}/`;
  const outsideSubtree = (candidate: string) => {
    const value = normalizeTreePath(candidate);
    return value !== normalized && !value.startsWith(descendantPrefix);
  };
  const nextExcluded = excludedPaths.map(normalizeTreePath).filter(outsideSubtree);
  const nextIncluded = includedPaths.map(normalizeTreePath).filter(outsideSubtree);
  const inheritedExcluded = isPathExcluded(normalized, nextExcluded, nextIncluded);

  if (excluded !== inheritedExcluded) {
    (excluded ? nextExcluded : nextIncluded).push(normalized);
  }
  return { excluded: nextExcluded, included: nextIncluded };
}

export function normalizeTreePath(path: string) {
  return path.replaceAll("\\", "/").replace(/^\/+|\/+$/g, "");
}

export function isPathExcluded(
  path: string,
  excludedPaths: Iterable<string>,
  includedPaths: Iterable<string>,
) {
  const normalized = normalizeTreePath(path);
  const excludedDepth = deepestMatch([...excludedPaths].map(normalizeTreePath), normalized);
  const includedDepth = deepestMatch([...includedPaths].map(normalizeTreePath), normalized);
  if (excludedDepth === undefined) return false;
  if (includedDepth === undefined) return true;
  return excludedDepth > includedDepth;
}

export function hasPathDescendantOverride(
  path: string,
  excludedPaths: Iterable<string>,
  includedPaths: Iterable<string>,
) {
  const normalized = normalizeTreePath(path);
  const prefix = `${normalized}/`;
  const overrides = isPathExcluded(normalized, excludedPaths, includedPaths)
    ? includedPaths
    : excludedPaths;
  return [...overrides]
    .map(normalizeTreePath)
    .some((candidate) => candidate.startsWith(prefix));
}

function deepestMatch(paths: string[], target: string) {
  return paths
    .filter((path) => target === path || target.startsWith(`${path}/`))
    .reduce<number | undefined>((deepest, path) => Math.max(deepest ?? 0, path.length), undefined);
}