import type { GroupRule, SourceEntry } from "./types/generated";

export interface SourceTreeNode extends SourceEntry {
  name: string;
  children: SourceTreeNode[];
}

export interface GroupTreeNode {
  /** Extracted value (for example a version) used as the group key. */
  key: string;
  /** Optional display label shown before the key. */
  label: string;
  selected: boolean;
  matched: SourceEntry[];
}

export interface PathSelectionState {
  included: string[];
  excluded: string[];
}

export type SourceView = "list" | "tree";
export type SourceSortKey = "name" | "size" | "modified";
export type SortDirection = "asc" | "desc";
export interface SourceSort {
  key: SourceSortKey;
  direction: SortDirection;
}

export function filterSourceEntries(entries: SourceEntry[], query: string) {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  return entries.filter((entry) => !normalizedQuery
    || normalizeTreePath(entry.relativePath).toLocaleLowerCase().includes(normalizedQuery));
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

export function sortSourceEntries(entries: SourceEntry[], sort: SourceSort): SourceEntry[] {
  return [...entries].sort((left, right) => {
    let result = 0;
    switch (sort.key) {
      case "size":
        result = (left.size ?? 0) - (right.size ?? 0);
        break;
      case "modified":
        result = (left.modified ?? 0) - (right.modified ?? 0);
        break;
      default:
        result = compareNames(left, right);
        break;
    }
    if (result === 0) result = compareNames(left, right);
    return sort.direction === "desc" ? -result : result;
  });
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

export function buildGroupTree(
  entries: SourceEntry[],
  group: GroupRule | null,
  selectedGroups: Iterable<string>,
  query: string,
): GroupTreeNode[] {
  const selected = new Set(selectedGroups);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const topLevelFiles = entries.filter(
    (entry) => !entry.directory && !normalizeTreePath(entry.relativePath).includes("/"),
  );
  const nodesByKey = new Map<string, GroupTreeNode>();
  const unmatched: GroupTreeNode = {
    key: "",
    label: "Other files",
    selected: false,
    matched: [],
  };
  for (const file of topLevelFiles) {
    const name = entryName(file);
    const key = group ? extractGroupKey(group.pattern, name) : null;
    if (key === null || !group) {
      unmatched.matched.push(file);
      continue;
    }
    let node = nodesByKey.get(key);
    if (!node) {
      node = { key, label: group.label, selected: selected.has(key), matched: [] };
      nodesByKey.set(key, node);
    }
    node.matched.push(file);
  }
  let result = [...nodesByKey.values()].sort((left, right) => compareVersions(right.key, left.key));
  if (unmatched.matched.length > 0) result.push(unmatched);
  if (!normalizedQuery) return result;
  return result.filter(
    (node) =>
      node.key.toLocaleLowerCase().includes(normalizedQuery)
      || node.label.toLocaleLowerCase().includes(normalizedQuery)
      || node.matched.some((file) =>
        normalizeTreePath(file.relativePath).toLocaleLowerCase().includes(normalizedQuery),
      ),
  );
}

function extractGroupKey(pattern: string, file: string): string | null {
  try {
    const match = new RegExp(pattern).exec(file);
    if (!match) return null;
    const value = match[1] ?? match[0];
    return value ? value : null;
  } catch {
    return null;
  }
}

/**
 * Applies a rename rule to a source file name, mirroring the Rust engine.
 * Returns the new name or `null` when the pattern does not match.
 */
export function applyRename(
  pattern: string,
  destination: string,
  fileName: string,
): string | null {
  try {
    const match = new RegExp(pattern).exec(fileName);
    if (!match) return null;
    const renamed = destination.replace(/\$(\d+)/g, (_, index: string) => match[Number(index)] ?? "");
    return renamed ? renamed : null;
  } catch {
    return null;
  }
}

/** Compares dotted version strings numerically, segment by segment (1.0.0.10 > 1.0.0.2). */
export function compareVersions(left: string, right: string) {
  const leftParts = left.split(".");
  const rightParts = right.split(".");
  const length = Math.max(leftParts.length, rightParts.length);
  for (let index = 0; index < length; index += 1) {
    const leftPart = leftParts[index];
    const rightPart = rightParts[index];
    if (leftPart === undefined) return -1;
    if (rightPart === undefined) return 1;
    const leftNumber = Number(leftPart);
    const rightNumber = Number(rightPart);
    if (Number.isFinite(leftNumber) && Number.isFinite(rightNumber)) {
      if (leftNumber !== rightNumber) return leftNumber - rightNumber;
    } else {
      const compared = leftPart.localeCompare(rightPart, undefined, {
        numeric: true,
        sensitivity: "base",
      });
      if (compared !== 0) return compared;
    }
  }
  return 0;
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
