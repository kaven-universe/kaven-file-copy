import type { SourceEntry } from "./types/generated";

export interface SourceTreeNode extends SourceEntry {
  name: string;
  children: SourceTreeNode[];
}

export interface PathSelectionState {
  included: string[];
  excluded: string[];
}

export type SourceEntryTypeFilter = "all" | "folders" | "files";
export type SourceView = "list" | "tree";
export type SourceSortKey = "name" | "type" | "size" | "modified";
export type SortDirection = "asc" | "desc";
export interface SourceSort {
  key: SourceSortKey;
  direction: SortDirection;
}

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

export function buildSourceTree(
  entries: SourceEntry[],
  sort: SourceSort = { key: "name", direction: "asc" },
): SourceTreeNode[] {
  const roots: SourceTreeNode[] = [];
  const nodes = new Map<string, SourceTreeNode>();

  for (const entry of entries) {
    const parts = normalizeTreePath(entry.relativePath).split("/").filter(Boolean);
    let children = roots;
    let relativePath = "";

    parts.forEach((name, index) => {
      relativePath = relativePath ? `${relativePath}/${name}` : name;
      const final = index === parts.length - 1;
      const directory = !final || entry.directory;
      let node = nodes.get(relativePath);
      if (!node) {
        node = {
          name,
          relativePath,
          directory,
          size: final ? entry.size : 0,
          modified: final ? entry.modified : null,
          children: [],
        };
        nodes.set(relativePath, node);
        children.push(node);
      } else {
        if (directory) node.directory = true;
        if (final) {
          node.size = entry.size;
          node.modified = entry.modified;
        }
      }
      children = node.children;
    });
  }

  sortTree(roots, sort);
  return roots;
}

function sortTree(nodes: SourceTreeNode[], sort: SourceSort) {
  nodes.sort((left, right) => compareSourceEntries(left, right, sort));
  nodes.forEach((node) => sortTree(node.children, sort));
}

export function sortSourceEntries(entries: SourceEntry[], sort: SourceSort): SourceEntry[] {
  return [...entries].sort((left, right) => compareSourceEntries(left, right, sort));
}

function compareSourceEntries(
  left: SourceEntry,
  right: SourceEntry,
  sort: SourceSort,
) {
  // Directories are grouped first unless sorting by type in descending order.
  const dirsFirst = sort.key !== "type" || sort.direction === "asc";
  if (left.directory !== right.directory) {
    return left.directory === dirsFirst ? -1 : 1;
  }
  let result = 0;
  switch (sort.key) {
    case "name":
    case "type":
      result = compareNames(left, right);
      break;
    case "size":
      result = (left.size ?? 0) - (right.size ?? 0);
      break;
    case "modified":
      result = (left.modified ?? 0) - (right.modified ?? 0);
      break;
  }
  if (result === 0) result = compareNames(left, right);
  return sort.direction === "desc" ? -result : result;
}

function compareNames(left: SourceEntry, right: SourceEntry) {
  return entryName(left).localeCompare(entryName(right), undefined, { numeric: true, sensitivity: "base" });
}

function entryName(entry: SourceEntry) {
  const normalized = normalizeTreePath(entry.relativePath);
  const index = normalized.lastIndexOf("/");
  return index === -1 ? normalized : normalized.slice(index + 1);
}

export function formatBytes(bytes: number | undefined | null) {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = -1;
  do {
    value /= 1024;
    unit += 1;
  } while (value >= 1024 && unit < units.length - 1);
  return `${value >= 100 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`;
}

export function formatModified(modified: number | null | undefined) {
  if (modified === null || modified === undefined) return "—";
  const date = new Date(modified);
  if (Number.isNaN(date.getTime())) return "—";
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function setPathIncluded(
  includedPaths: string[],
  excludedPaths: string[],
  path: string,
  included: boolean,
): PathSelectionState {
  const normalized = normalizeTreePath(path);
  const descendantPrefix = `${normalized}/`;
  const outsideSubtree = (candidate: string) => {
    const value = normalizeTreePath(candidate);
    return value !== normalized && !value.startsWith(descendantPrefix);
  };
  const nextIncluded = includedPaths.map(normalizeTreePath).filter(outsideSubtree);
  const nextExcluded = excludedPaths.map(normalizeTreePath).filter(outsideSubtree);
  const inheritedIncluded = isPathIncluded(normalized, nextIncluded, nextExcluded);

  if (included !== inheritedIncluded) {
    (included ? nextIncluded : nextExcluded).push(normalized);
  }
  return { included: nextIncluded, excluded: nextExcluded };
}

export function normalizeTreePath(path: string) {
  return path.replaceAll("\\", "/").replace(/^\/+|\/+$/g, "");
}

export function isPathIncluded(
  path: string,
  includedPaths: Iterable<string>,
  excludedPaths: Iterable<string>,
) {
  const normalized = normalizeTreePath(path);
  const includedDepth = deepestMatch([...includedPaths].map(normalizeTreePath), normalized);
  if (includedDepth === undefined) return false;
  const excludedDepth = deepestMatch([...excludedPaths].map(normalizeTreePath), normalized);
  if (excludedDepth === undefined) return true;
  return includedDepth > excludedDepth;
}

export function hasPathDescendantOverride(
  path: string,
  includedPaths: Iterable<string>,
  excludedPaths: Iterable<string>,
) {
  const normalized = normalizeTreePath(path);
  const prefix = `${normalized}/`;
  const overrides = isPathIncluded(normalized, includedPaths, excludedPaths)
    ? excludedPaths
    : includedPaths;
  return [...overrides]
    .map(normalizeTreePath)
    .some((candidate) => candidate.startsWith(prefix));
}

function deepestMatch(paths: string[], target: string) {
  return paths
    .filter((path) => target === path || target.startsWith(`${path}/`))
    .reduce<number | undefined>((deepest, path) => Math.max(deepest ?? 0, path.length), undefined);
}
