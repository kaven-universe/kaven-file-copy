import type { PlanAction, PlanItem } from "./types/generated";

export type PreviewActionFilter = "all" | PlanAction;
export type PreviewSortKey = "action" | "source" | "target" | "description";
export type PreviewSortDirection = "asc" | "desc";

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