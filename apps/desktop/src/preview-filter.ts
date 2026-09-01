import type { PlanAction, PlanItem } from "./types/generated";

export type PreviewActionFilter = "all" | PlanAction;
export type PreviewView = "list" | "tree";
export type PreviewSortKey = "action" | "source" | "target" | "description";
export type PreviewSortDirection = "asc" | "desc";

export interface PreviewTreeNode {
  name: string;
  relativePath: string;
  directory: boolean;
  item?: PlanItem;
  children: PreviewTreeNode[];
}

export function filterPlanItems(
  items: PlanItem[],
  action: PreviewActionFilter,
  query: string,
) {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  return items.filter((item) => {
    if (action !== "all" && item.action !== action) return false;
    if (!normalizedQuery) return true;
    return item.relativePath.toLocaleLowerCase().includes(normalizedQuery)
      || item.reason.toLocaleLowerCase().includes(normalizedQuery)
      || (item.warning ?? "").toLocaleLowerCase().includes(normalizedQuery);
  });
}

export function sortPreviewItems(
  items: PlanItem[],
  key: PreviewSortKey,
  direction: PreviewSortDirection,
): PlanItem[] {
  return [...items].sort((left, right) => {
    const result = comparePreviewItems(left, right, key);
    return direction === "desc" ? -result : result;
  });
}

function comparePreviewItems(left: PlanItem, right: PlanItem, key: PreviewSortKey): number {
  const compared = previewSortValue(left, key).localeCompare(
    previewSortValue(right, key),
    undefined,
    { numeric: true, sensitivity: "base" },
  );
  if (compared !== 0) return compared;
  return left.relativePath.localeCompare(right.relativePath, undefined, {
    numeric: true,
    sensitivity: "base",
  });
}

function previewSortValue(item: PlanItem, key: PreviewSortKey): string {
  switch (key) {
    case "action":
      return item.action;
    case "source":
      return item.relativePath;
    case "target":
      return item.destinationPath ?? item.relativePath;
    case "description":
      return item.reason;
  }
}

export function buildPreviewTree(items: PlanItem[]): PreviewTreeNode[] {
  const roots: PreviewTreeNode[] = [];
  const nodes = new Map<string, PreviewTreeNode>();

  for (const item of items) {
    const parts = item.relativePath.replaceAll("\\", "/").split("/").filter(Boolean);
    let children = roots;
    let relativePath = "";

    parts.forEach((name, index) => {
      relativePath = relativePath ? `${relativePath}/${name}` : name;
      const final = index === parts.length - 1;
      let node = nodes.get(relativePath);
      if (!node) {
        node = {
          name,
          relativePath,
          directory: !final || item.action === "create-directory",
          children: [],
        };
        nodes.set(relativePath, node);
        children.push(node);
      }
      if (final) {
        node.item = item;
        node.directory ||= item.action === "create-directory";
      }
      children = node.children;
    });
  }

  sortPreviewTree(roots);
  return roots;
}

function sortPreviewTree(nodes: PreviewTreeNode[]) {
  nodes.sort((left, right) => {
    if (left.directory !== right.directory) return left.directory ? -1 : 1;
    return left.name.localeCompare(right.name, undefined, { numeric: true, sensitivity: "base" });
  });
  nodes.forEach((node) => sortPreviewTree(node.children));
}