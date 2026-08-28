import type { SourceEntry } from "./types/generated";

export interface SourceTreeNode extends SourceEntry {
  name: string;
  children: SourceTreeNode[];
}

export function buildSourceTree(entries: SourceEntry[]): SourceTreeNode[] {
  const roots: SourceTreeNode[] = [];
  const nodes = new Map<string, SourceTreeNode>();

  for (const entry of entries) {
    const parts = entry.relativePath.replaceAll("\\", "/").split("/").filter(Boolean);
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