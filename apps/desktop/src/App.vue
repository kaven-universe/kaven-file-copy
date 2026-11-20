<script setup lang="ts">
import { computed, onMounted, reactive, ref, toRaw } from "vue";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { api } from "./api/tauri";
import { locale, setLocale, t, type Locale } from "./i18n";
import { filterPlanItems, sortPreviewItems, type PreviewActionFilter, type PreviewSortDirection, type PreviewSortKey } from "./preview-filter";
import { buildRunDetails } from "./run-results";
import SourceTreeNode from "./SourceTreeNode.vue";
import GroupTreeNode from "./GroupTreeNode.vue";
import {
  applyRename,
  buildGroupTree,
  buildSourceTree,
  filterSourceEntries,
  formatBytes,
  formatModified,
  hasPathDescendantOverride,
  initialGroupRunSelection,
  isPathIncluded,
  latestGroupKey,
  limitGroupSelection,
  normalizeTreePath,
  selectAllGroups as collectAllGroups,
  setPathIncluded,
  sortSourceEntries,
  toggleGroupSelection,
  type SourceSort,
  type SourceSortKey,
  type SourceView,
  type SortDirection,
} from "./source-tree";
import type { CopyPlan, CopyRule, JobEvent, JobSummary, PlanProgressEvent, ProgressEvent, RecentPaths, SelectionMode, SourceEntry } from "./types/generated";

type View = "rules" | "editor" | "jobs";
type AppMenu = "file" | "edit" | "view" | "help";
interface GroupRunDialog {
  rule: CopyRule;
  entries: SourceEntry[];
  selectedGroups: string[];
  query: string;
}

const view = ref<View>("rules");
const activeMenu = ref<AppMenu | null>(null);
const isMaximized = ref(false);
const appWindow = getCurrentWindow();
const appVersion = ref("");
const rules = ref<CopyRule[]>([]);
const jobs = ref<JobSummary[]>([]);
const selectedRule = ref<CopyRule | null>(null);
const originalRule = ref<CopyRule | null>(null);
const confirmDialog = ref<{ title: string; message: string; onConfirm: () => void; onCancel?: () => void } | null>(null);
const noticeDialog = ref<{ title: string; message: string } | null>(null);
const checkingUpdates = ref(false);
const plan = ref<CopyPlan | null>(null);
const sourceEntries = ref<SourceEntry[]>([]);
const sourceQuery = ref("");
const sourceView = ref<SourceView>("tree");
const groupView = ref<"tree" | "list">("tree");
const sourceMode = ref<SelectionMode>("manual");
const sourceSortKey = ref<SourceSortKey>("name");
const sourceSortDirection = ref<SortDirection>("asc");
const recentPaths = ref<RecentPaths>({ schemaVersion: 1, sources: [], destinations: [], locale: "en" });
const busy = ref(false);
const message = ref("");
const progress = ref<ProgressEvent | null>(null);
const runModal = ref(false);
const runFinished = ref(false);
const runResult = ref<JobSummary | null>(null);
const runEvents = ref<JobEvent[]>([]);
const groupRunDialog = ref<GroupRunDialog | null>(null);
const previewProgress = ref<PlanProgressEvent | null>(null);
const previewQuery = ref("");
const previewAction = ref<PreviewActionFilter>("all");
const previewSortKey = ref<PreviewSortKey>("action");
const previewSortDirection = ref<PreviewSortDirection>("asc");
const jobEvents = ref<Record<string, JobEvent[]>>({});
const pathExclusions = computed(() => new Set(
  selectedRule.value?.filters.exclude.map(normalizeTreePath) ?? [],
));
const pathInclusions = computed(() => new Set(
  selectedRule.value?.filters.include.map(normalizeTreePath) ?? [],
));
const filteredSourceEntries = computed(() => filterSourceEntries(
  sourceEntries.value,
  sourceQuery.value,
));
const sourceFiles = computed(() => filteredSourceEntries.value.filter((entry) => !entry.directory));
const sourceSort = computed<SourceSort>(() => ({
  key: sourceSortKey.value,
  direction: sourceSortDirection.value,
}));
const sortedSourceFiles = computed(() => sortSourceEntries(sourceFiles.value, sourceSort.value));
const sourceRenames = computed(() => {
  const rename = selectedRule.value?.copy.rename;
  const map = new Map<string, string>();
  if (!rename) return map;
  for (const entry of sourceEntries.value) {
    if (entry.directory) continue;
    const fileName = normalizeTreePath(entry.relativePath).split("/").pop() ?? "";
    const newName = applyRename(rename.pattern, rename.destination, fileName);
    if (newName && newName !== fileName) map.set(entry.relativePath, newName);
  }
  return map;
});
const sourceTree = computed(() => buildSourceTree(filteredSourceEntries.value));
const groupTree = computed(() => {
  const rule = selectedRule.value;
  if (!rule) return [];
  const selected: string[] = [];
  if (rule.filters.autoLatestGroup) {
    const latest = latestGroupKey(sourceEntries.value, rule.filters.group);
    if (latest) selected.push(latest);
  } else {
    selected.push(...rule.filters.selectedGroups);
  }
  return buildGroupTree(filteredSourceEntries.value, rule.filters.group, selected, sourceQuery.value);
});
const allGroupKeys = computed(() => {
  const rule = selectedRule.value;
  if (!rule) return [];
  return buildGroupTree(sourceEntries.value, rule.filters.group, [], "")
    .map((group) => group.key)
    .filter(Boolean);
});
const selectedGroupCount = computed(() => {
  const rule = selectedRule.value;
  if (rule?.filters.autoLatestGroup) {
    return latestGroupKey(sourceEntries.value, rule.filters.group) ? 1 : 0;
  }
  const available = new Set(allGroupKeys.value);
  return new Set(
    (rule?.filters.selectedGroups ?? []).filter((key) => available.has(key)),
  ).size;
});
const allGroupsSelected = computed(() => allGroupKeys.value.length > 0
  && allGroupKeys.value.every((key) => selectedRule.value?.filters.selectedGroups.includes(key)));
const groupRunGroups = computed(() => {
  const dialog = groupRunDialog.value;
  if (!dialog) return [];
  return buildGroupTree(
    dialog.entries,
    dialog.rule.filters.group,
    dialog.selectedGroups,
    dialog.query,
  ).filter((group) => Boolean(group.key));
});
const allRunGroupKeys = computed(() => {
  const dialog = groupRunDialog.value;
  if (!dialog) return [];
  return buildGroupTree(dialog.entries, dialog.rule.filters.group, [], "")
    .map((group) => group.key)
    .filter(Boolean);
});
const allRunGroupsSelected = computed(() => allRunGroupKeys.value.length > 0
  && allRunGroupKeys.value.every((key) => groupRunDialog.value?.selectedGroups.includes(key)));
const groupFileRows = computed(() => groupTree.value.flatMap((group) =>
  group.matched.map((file) => ({
    ...file,
    groupKey: group.key,
    groupLabel: group.key
      ? (group.label ? `${group.label} ${group.key}` : group.key)
      : t("files.groupOther"),
    selected: group.selected,
  })),
));
const sourceFilterActive = computed(() => sourceQuery.value.trim().length > 0);
const filteredPlanItems = computed(() => filterPlanItems(
  plan.value?.items ?? [],
  previewAction.value,
  previewQuery.value,
));
const sortedPlanItems = computed(() => sortPreviewItems(
  filteredPlanItems.value,
  previewSortKey.value,
  previewSortDirection.value,
));
const runDetails = computed(() => runResult.value
  ? buildRunDetails(runResult.value, runEvents.value)
  : []);

const triggerLabelKeys: Record<string, string> = {
  manual: "editor.trigger.manual",
  watch: "editor.trigger.watch",
  interval: "editor.trigger.interval",
  cron: "editor.trigger.cron",
};

function newRule(): CopyRule {
  return {
    schemaVersion: 1, id: crypto.randomUUID(), name: t("defaults.newRuleName"), enabled: true,
    source: { type: "local", path: "" }, destination: { type: "local", path: "" },
    filters: { exclude: [], include: [], includeHidden: true, group: null, selectedGroups: [], autoLatestGroup: false, singleGroup: false, confirmGroupsBeforeRun: false, selectionMode: "manual" },
    copy: { mode: "incremental", compare: "size-and-mtime", overwrite: "if-changed", symlinks: "skip", preserveMtime: true, maxConcurrency: 4, retryCount: 2, rename: null },
    trigger: { type: "manual" },
  };
}

async function refresh() {
  [rules.value, jobs.value] = await Promise.all([api.listRules(), api.listJobs()]);
  try {
    recentPaths.value = await api.getRecentPaths();
    setLocale(recentPaths.value.locale);
  } catch (error) {
    message.value = t("msg.loadRecentPaths", { error: String(error) });
  }
}

async function changeLocale(next: Locale) {
  const previous = locale.value;
  setLocale(next);
  try {
    await api.setLocale(next);
    recentPaths.value.locale = next;
  } catch (error) {
    setLocale(previous);
    message.value = String(error);
  }
}

async function rememberRecentPath(kind: "source" | "destination", path: string) {
  if (!path.trim()) return;
  try {
    recentPaths.value = await api.rememberRecentPath(kind, path);
  } catch (error) {
    message.value = t("msg.updateRecentPaths", { error: String(error) });
  }
}

async function rememberRulePaths(rule: CopyRule) {
  await rememberRecentPath("source", rule.source.path);
  await rememberRecentPath("destination", rule.destination.path);
}

function edit(rule?: CopyRule) {
  const copy = structuredClone(rule ? toRaw(rule) : newRule());
  copy.filters.include ??= [];
  copy.filters.exclude ??= [];
  copy.filters.group ??= null;
  copy.filters.selectedGroups ??= [];
  copy.filters.autoLatestGroup ??= false;
  copy.filters.singleGroup ??= false;
  copy.filters.confirmGroupsBeforeRun ??= false;
  copy.filters.selectionMode ??= "manual";
  copy.copy.rename ??= null;
  originalRule.value = structuredClone(copy);
  sourceMode.value = copy.filters.selectionMode;
  selectedRule.value = reactive(copy);
  sourceEntries.value = [];
  sourceQuery.value = "";
  plan.value = null;
  previewQuery.value = "";
  previewAction.value = "all";
  message.value = "";
  view.value = "editor";
}

async function choosePath(kind: "source" | "destination", directory: boolean) {
  await act(async () => {
    const value = await open({ directory, multiple: false });
    if (value && selectedRule.value) {
      selectedRule.value[kind].path = value;
      if (kind === "source") clearSourceBrowser();
      await rememberRecentPath(kind, value);
    }
  });
}

function clearSourceBrowser() {
  sourceEntries.value = [];
  sourceQuery.value = "";
}

function changeTrigger(event: Event) {
  if (!selectedRule.value) return;
  const type = (event.target as HTMLSelectElement).value;
  if (type === "watch") selectedRule.value.trigger = { type, debounceMs: 1000 };
  else if (type === "interval") selectedRule.value.trigger = { type, seconds: 300 };
  else if (type === "cron") selectedRule.value.trigger = { type, expression: "0 0/15 * * * * *" };
  else selectedRule.value.trigger = { type: "manual" };
}

async function browseSource() {
  if (!selectedRule.value) return;
  await act(async () => { sourceEntries.value = await api.listSourceEntries(selectedRule.value!); });
}

function toggleIncluded(path: string, checked: boolean) {
  if (!selectedRule.value) return;
  const state = setPathIncluded(
    [...pathInclusions.value],
    [...pathExclusions.value],
    path,
    checked,
  );
  selectedRule.value.filters.include = state.included;
  selectedRule.value.filters.exclude = state.excluded;
}

function sourceEntryIncluded(path: string) {
  return isPathIncluded(path, pathInclusions.value, pathExclusions.value);
}

function sourceEntryHasOverride(path: string) {
  return hasPathDescendantOverride(path, pathInclusions.value, pathExclusions.value);
}

function sortFilesBy(key: SourceSortKey) {
  if (sourceSortKey.value === key) {
    sourceSortDirection.value = sourceSortDirection.value === "asc" ? "desc" : "asc";
  } else {
    sourceSortKey.value = key;
    sourceSortDirection.value = "asc";
  }
}

function ariaSort(key: SourceSortKey) {
  return sourceSortKey.value === key
    ? (sourceSortDirection.value === "asc" ? "ascending" : "descending")
    : "none";
}

function previewSortBy(key: PreviewSortKey) {
  if (previewSortKey.value === key) {
    previewSortDirection.value = previewSortDirection.value === "asc" ? "desc" : "asc";
  } else {
    previewSortKey.value = key;
    previewSortDirection.value = "asc";
  }
}

function previewAriaSort(key: PreviewSortKey) {
  return previewSortKey.value === key
    ? (previewSortDirection.value === "asc" ? "ascending" : "descending")
    : "none";
}

function setSourceMode(mode: SelectionMode) {
  sourceMode.value = mode;
  if (!selectedRule.value) return;
  selectedRule.value.filters.selectionMode = mode;
  if (mode === "groups" && !selectedRule.value.filters.group) {
    addGroupRule();
  }
}

function addGroupRule() {
  if (!selectedRule.value) return;
  selectedRule.value.filters.group = {
    label: "",
    pattern: "(\\d+\\.\\d+\\.\\d+\\.\\d+)",
  };
}

function removeGroupRule() {
  if (!selectedRule.value) return;
  selectedRule.value.filters.group = null;
  selectedRule.value.filters.selectedGroups = [];
}

function toggleGroup(name: string, checked: boolean) {
  if (!selectedRule.value || selectedRule.value.filters.autoLatestGroup) return;
  selectedRule.value.filters.selectedGroups = toggleGroupSelection(
    selectedRule.value.filters.selectedGroups,
    name,
    checked,
    selectedRule.value.filters.singleGroup,
  );
}

function setSingleGroup(enabled: boolean) {
  if (!selectedRule.value) return;
  selectedRule.value.filters.singleGroup = enabled;
  if (enabled) {
    const available = new Set(allGroupKeys.value);
    selectedRule.value.filters.selectedGroups = limitGroupSelection(
      selectedRule.value.filters.selectedGroups.filter((key) => available.has(key)),
    );
  }
}

function selectEveryGroup() {
  if (!selectedRule.value
    || selectedRule.value.filters.autoLatestGroup
    || selectedRule.value.filters.singleGroup) return;
  selectedRule.value.filters.selectedGroups = collectAllGroups(allGroupKeys.value);
}

function clearGroupSelection() {
  if (!selectedRule.value || selectedRule.value.filters.autoLatestGroup) return;
  selectedRule.value.filters.selectedGroups = [];
}

function addRename() {
  if (!selectedRule.value) return;
  selectedRule.value.copy.rename = {
    pattern: "^(.+)_(\\d+\\.\\d+\\.\\d+\\.\\d+)_(.+)\\.(.+)$",
    destination: "$1_$2.$4",
  };
}

function removeRename() {
  if (!selectedRule.value) return;
  selectedRule.value.copy.rename = null;
}

const renameExample = computed(() => {
  const rename = selectedRule.value?.copy.rename;
  if (!rename) return "";
  const files = sourceEntries.value.filter((entry) => !entry.directory);
  if (files.length === 0) return t("rename.previewBrowse");
  for (const entry of files) {
    const fileName = normalizeTreePath(entry.relativePath).split("/").pop() ?? "";
    const newName = applyRename(rename.pattern, rename.destination, fileName);
    if (newName && newName !== fileName) return `${fileName} → ${newName}`;
  }
  const first = normalizeTreePath(files[0].relativePath).split("/").pop() ?? "";
  return t("rename.noMatch", { name: first });
});

function askConfirm(title: string, message: string, onConfirm: () => void, onCancel?: () => void) {
  confirmDialog.value = { title, message, onConfirm, onCancel };
}

function resolveConfirm(confirmed: boolean) {
  const dialog = confirmDialog.value;
  confirmDialog.value = null;
  if (!dialog) return;
  if (confirmed) dialog.onConfirm();
  else if (dialog.onCancel) dialog.onCancel();
}

function afterDiscardConfirmation(action: () => void) {
  if (view.value !== "editor" || !selectedRule.value) {
    action();
    return;
  }
  const dirty = JSON.stringify(toRaw(selectedRule.value)) !== JSON.stringify(originalRule.value);
  if (dirty) {
    askConfirm(t("dialog.discardTitle"), t("dialog.discardMessage"), action);
    return;
  }
  action();
}

function navigate(target: "rules" | "jobs") {
  afterDiscardConfirmation(() => { view.value = target; });
}

function cancelEdit() {
  if (!selectedRule.value) {
    view.value = "rules";
    return;
  }
  const dirty = JSON.stringify(toRaw(selectedRule.value)) !== JSON.stringify(originalRule.value);
  if (dirty) {
    askConfirm(
      t("dialog.discardTitle"),
      t("dialog.discardMessage"),
      () => { view.value = "rules"; },
    );
  } else {
    view.value = "rules";
  }
}

async function persist() {
  if (!selectedRule.value) return;
  await act(async () => {
    await api.validateRule(selectedRule.value!);
    await api.saveRule(selectedRule.value!);
    await rememberRulePaths(selectedRule.value!);
    await refresh();
    view.value = "rules";
  });
}

async function preview() {
  if (!selectedRule.value) return;
  plan.value = null;
  previewQuery.value = "";
  previewAction.value = "all";
  previewProgress.value = {
    phase: "scanning",
    completedEntries: 0,
    totalEntries: null,
    relativePath: null,
  };
  try {
    await act(async () => {
      plan.value = await api.previewRule(selectedRule.value!);
      await rememberRulePaths(selectedRule.value!);
    });
  } finally {
    previewProgress.value = null;
  }
}

async function run(rule: CopyRule) {
  progress.value = null;
  runModal.value = true;
  runFinished.value = false;
  runResult.value = null;
  runEvents.value = [];
  await act(async () => {
    await rememberRulePaths(rule);
    const summary = await api.executeRule(structuredClone(toRaw(rule)));
    jobs.value = [summary, ...jobs.value.filter((job) => job.jobId !== summary.jobId)];
    progress.value = null;
    runResult.value = summary;
    try {
      runEvents.value = await api.getJobEvents(summary.jobId);
      jobEvents.value[summary.jobId] = runEvents.value;
    } catch (error) {
      message.value = t("msg.loadRunDetails", { error: String(error) });
    }
  });
  runFinished.value = true;
}

async function requestRun(rule: CopyRule) {
  if (rule.filters.selectionMode !== "groups" || !rule.filters.confirmGroupsBeforeRun) {
    await run(rule);
    return;
  }
  await act(async () => {
    const runRule = structuredClone(toRaw(rule));
    const entries = await api.listSourceEntries(runRule);
    groupRunDialog.value = {
      rule: runRule,
      entries,
      selectedGroups: initialGroupRunSelection(
        entries,
        runRule.filters.group,
        runRule.filters.selectedGroups,
        runRule.filters.autoLatestGroup,
        runRule.filters.singleGroup,
      ),
      query: "",
    };
  });
}

function toggleRunGroup(name: string, checked: boolean) {
  const dialog = groupRunDialog.value;
  if (!dialog) return;
  dialog.selectedGroups = toggleGroupSelection(
    dialog.selectedGroups,
    name,
    checked,
    dialog.rule.filters.singleGroup,
  );
}

function selectEveryRunGroup() {
  const dialog = groupRunDialog.value;
  if (!dialog || dialog.rule.filters.singleGroup) return;
  dialog.selectedGroups = collectAllGroups(allRunGroupKeys.value);
}

function clearRunGroupSelection() {
  if (groupRunDialog.value) groupRunDialog.value.selectedGroups = [];
}

function cancelGroupRun() {
  groupRunDialog.value = null;
}

function confirmGroupRun() {
  const dialog = groupRunDialog.value;
  if (!dialog || dialog.selectedGroups.length === 0) return;
  const executionRule = structuredClone(toRaw(dialog.rule));
  executionRule.filters.autoLatestGroup = false;
  executionRule.filters.selectedGroups = [...dialog.selectedGroups];
  groupRunDialog.value = null;
  void run(executionRule);
}

function cancelRun() {
  void cancelJob();
}

function closeRun() {
  runModal.value = false;
  runFinished.value = false;
  runResult.value = null;
  runEvents.value = [];
}

function runCurrentRule() {
  if (!selectedRule.value) return;
  void requestRun(selectedRule.value);
}

async function toggleRuleEnabled(rule: CopyRule) {
  const updated = structuredClone(toRaw(rule));
  updated.enabled = !updated.enabled;
  await act(async () => { await api.saveRule(updated); await refresh(); });
}

function duplicateRule(rule: CopyRule) {
  const copy = structuredClone(toRaw(rule));
  copy.id = crypto.randomUUID();
  copy.name = `${copy.name} (${t("suffix.copy")})`;
  edit(copy);
}

function removeRule(rule: CopyRule) {
  askConfirm(
    t("dialog.deleteTitle"),
    t("dialog.deleteMessage", { name: rule.name }),
    async () => {
      await act(async () => { await api.deleteRule(rule.id); await refresh(); });
    },
  );
}

async function loadJobEvents(jobId: string) {
  if (jobEvents.value[jobId]) return;
  await act(async () => { jobEvents.value[jobId] = await api.getJobEvents(jobId); });
}

async function removeJob(job: JobSummary) {
  await act(async () => {
    await api.deleteJob(job.jobId);
    jobs.value = jobs.value.filter((entry) => entry.jobId !== job.jobId);
    delete jobEvents.value[job.jobId];
  });
}

function removeAllJobs() {
  askConfirm(
    t("dialog.removeAllTitle"),
    t("dialog.removeAllMessage"),
    async () => {
      await act(async () => {
        await api.deleteAllJobs();
        jobs.value = [];
        jobEvents.value = {};
      });
    },
  );
}

async function cancelJob() {
  await act(async () => { await api.cancelJob(); });
}

async function openPath(path: string) {
  if (!path.trim()) return;
  await act(async () => { await api.openPath(path); });
}

async function exportRule(rule: CopyRule) {
  const path = await save({ defaultPath: `${rule.name.replaceAll(/[^a-zA-Z0-9_-]/g, "-")}.kfc.json` });
  if (path) await writeTextFile(path, JSON.stringify(rule, null, 2) + "\n");
}

async function importRule() {
  const path = await open({ multiple: false, filters: [{ name: t("import.filterName"), extensions: ["json"] }] });
  if (!path) return;
  await act(async () => {
    const rule = JSON.parse(await readTextFile(path)) as CopyRule;
    await api.validateRule(rule);
    if (rules.value.some((existing) => existing.id === rule.id)) {
      askConfirm(
        t("dialog.importConflictTitle"),
        t("dialog.importConflictMessage", { name: rule.name }),
        () => { edit(rule); },
        () => {
          rule.id = crypto.randomUUID();
          rule.name = `${rule.name} (${t("suffix.imported")})`;
          edit(rule);
        },
      );
    } else {
      edit(rule);
    }
  });
}

async function act(action: () => Promise<void>) {
  busy.value = true; message.value = "";
  try { await action(); } catch (error) { message.value = String(error); }
  finally { busy.value = false; }
}

function handleAppMenu(id: string) {
  activeMenu.value = null;
  switch (id) {
    case "menu-new-rule":
      afterDiscardConfirmation(() => edit());
      break;
    case "menu-import-rule":
      afterDiscardConfirmation(() => { void importRule(); });
      break;
    case "menu-undo":
      document.execCommand("undo");
      break;
    case "menu-redo":
      document.execCommand("redo");
      break;
    case "menu-cut":
      document.execCommand("cut");
      break;
    case "menu-copy":
      document.execCommand("copy");
      break;
    case "menu-paste":
      document.execCommand("paste");
      break;
    case "menu-select-all":
      document.execCommand("selectAll");
      break;
    case "menu-rules":
      navigate("rules");
      break;
    case "menu-history":
      navigate("jobs");
      break;
    case "menu-refresh":
      void act(refresh);
      break;
    case "menu-fullscreen":
      void (async () => appWindow.setFullscreen(!(await appWindow.isFullscreen())))();
      break;
    case "menu-exit":
      void api.exitApp();
      break;
    case "menu-check-updates":
      void checkForUpdates(true);
      break;
    case "menu-about":
      noticeDialog.value = {
        title: t("about.title"),
        message: t("about.message", { version: appVersion.value || t("updates.unknownVersion") }),
      };
      break;
  }
}

async function checkForUpdates(manual: boolean) {
  if (checkingUpdates.value) return;
  checkingUpdates.value = true;
  if (manual) {
    noticeDialog.value = { title: t("updates.title"), message: t("updates.checking") };
  }
  try {
    const update = await api.checkForUpdates();
    if (!update) {
      if (manual) {
        noticeDialog.value = {
          title: t("updates.title"),
          message: t("updates.upToDate", { version: appVersion.value || t("updates.unknownVersion") }),
        };
      }
      return;
    }
    noticeDialog.value = null;
    const notes = update.notes?.trim() ? `\n\n${update.notes.trim()}` : "";
    askConfirm(
      t("updates.availableTitle"),
      t("updates.available", { current: appVersion.value, latest: update.version, notes }),
      () => { void installAvailableUpdate(); },
    );
  } catch (error) {
    if (manual) {
      noticeDialog.value = {
        title: t("updates.failedTitle"),
        message: t("updates.failed", { error: String(error) }),
      };
    }
  } finally {
    checkingUpdates.value = false;
  }
}

async function installAvailableUpdate() {
  noticeDialog.value = { title: t("updates.installingTitle"), message: t("updates.installing") };
  try {
    await api.installUpdate();
  } catch (error) {
    noticeDialog.value = {
      title: t("updates.failedTitle"),
      message: t("updates.installFailed", { error: String(error) }),
    };
  }
}

function toggleMenu(menu: AppMenu) {
  activeMenu.value = activeMenu.value === menu ? null : menu;
}

function switchOpenMenu(menu: AppMenu) {
  if (activeMenu.value) activeMenu.value = menu;
}

async function minimizeWindow() {
  await appWindow.minimize();
}

async function toggleMaximizeWindow() {
  await appWindow.toggleMaximize();
  isMaximized.value = await appWindow.isMaximized();
}

async function closeWindow() {
  await appWindow.close();
}

function handleShortcut(event: KeyboardEvent) {
  if (event.key === "Escape") {
    activeMenu.value = null;
    return;
  }
  if (event.key === "F11") {
    event.preventDefault();
    handleAppMenu("menu-fullscreen");
    return;
  }
  if (!event.ctrlKey || event.altKey || event.shiftKey) return;
  const commands: Record<string, string> = {
    n: "menu-new-rule",
    o: "menu-import-rule",
    "1": "menu-rules",
    "2": "menu-history",
    r: "menu-refresh",
  };
  const command = commands[event.key.toLowerCase()];
  if (!command) return;
  event.preventDefault();
  handleAppMenu(command);
}

onMounted(async () => {
  document.addEventListener("keydown", handleShortcut);
  document.addEventListener("pointerdown", () => { activeMenu.value = null; });
  await listen<PlanProgressEvent>("preview-progress", (event) => { previewProgress.value = event.payload; });
  await listen<ProgressEvent>("job-progress", (event) => { progress.value = event.payload; });
  await listen<JobSummary>("job-finished", (event) => {
    jobs.value = [event.payload, ...jobs.value.filter((job) => job.jobId !== event.payload.jobId)];
    progress.value = null;
  });
  await listen<string>("scheduler-error", (event) => { message.value = event.payload; });
  try {
    appVersion.value = await getVersion();
    isMaximized.value = await appWindow.isMaximized();
    await appWindow.onResized(async () => { isMaximized.value = await appWindow.isMaximized(); });
  } catch {
    // Running outside Tauri (e.g. dev:web in a browser): leave the version empty.
  }
  await act(refresh);
  if (appVersion.value) void checkForUpdates(false);
});
</script>

<template>
  <div class="app-titlebar" data-tauri-drag-region @dblclick="toggleMaximizeWindow">
    <div class="titlebar-app" data-tauri-drag-region>
      <svg class="titlebar-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 3h9l5 5v13H5z"/><path d="M14 3v5h5M8 13h8M8 17h6"/></svg>
    </div>
    <nav class="app-menu" :aria-label="t('menu.aria')" @dblclick.stop>
      <div class="app-menu-group" @mouseenter="switchOpenMenu('file')">
        <button type="button" :class="{ active: activeMenu === 'file' }" :aria-expanded="activeMenu === 'file'" @pointerdown.stop @click.stop="toggleMenu('file')">{{ t("menu.file") }}</button>
        <div v-if="activeMenu === 'file'" class="app-menu-popup" @pointerdown.stop>
          <button type="button" @click="handleAppMenu('menu-new-rule')"><span>{{ t("menu.newRule") }}</span><kbd>Ctrl+N</kbd></button>
          <button type="button" @click="handleAppMenu('menu-import-rule')"><span>{{ t("menu.importRule") }}</span><kbd>Ctrl+O</kbd></button>
          <hr />
          <button type="button" @click="handleAppMenu('menu-exit')"><span>{{ t("menu.exit") }}</span></button>
        </div>
      </div>
      <div class="app-menu-group" @mouseenter="switchOpenMenu('edit')">
        <button type="button" :class="{ active: activeMenu === 'edit' }" :aria-expanded="activeMenu === 'edit'" @pointerdown.stop @click.stop="toggleMenu('edit')">{{ t("menu.edit") }}</button>
        <div v-if="activeMenu === 'edit'" class="app-menu-popup" @pointerdown.stop>
          <button type="button" @click="handleAppMenu('menu-undo')"><span>{{ t("menu.undo") }}</span><kbd>Ctrl+Z</kbd></button>
          <button type="button" @click="handleAppMenu('menu-redo')"><span>{{ t("menu.redo") }}</span><kbd>Ctrl+Y</kbd></button>
          <hr />
          <button type="button" @click="handleAppMenu('menu-cut')"><span>{{ t("menu.cut") }}</span><kbd>Ctrl+X</kbd></button>
          <button type="button" @click="handleAppMenu('menu-copy')"><span>{{ t("menu.copy") }}</span><kbd>Ctrl+C</kbd></button>
          <button type="button" @click="handleAppMenu('menu-paste')"><span>{{ t("menu.paste") }}</span><kbd>Ctrl+V</kbd></button>
          <button type="button" @click="handleAppMenu('menu-select-all')"><span>{{ t("menu.selectAll") }}</span><kbd>Ctrl+A</kbd></button>
        </div>
      </div>
      <div class="app-menu-group" @mouseenter="switchOpenMenu('view')">
        <button type="button" :class="{ active: activeMenu === 'view' }" :aria-expanded="activeMenu === 'view'" @pointerdown.stop @click.stop="toggleMenu('view')">{{ t("menu.view") }}</button>
        <div v-if="activeMenu === 'view'" class="app-menu-popup" @pointerdown.stop>
          <button type="button" @click="handleAppMenu('menu-rules')"><span>{{ t("menu.rules") }}</span><kbd>Ctrl+1</kbd></button>
          <button type="button" @click="handleAppMenu('menu-history')"><span>{{ t("menu.history") }}</span><kbd>Ctrl+2</kbd></button>
          <hr />
          <button type="button" @click="handleAppMenu('menu-refresh')"><span>{{ t("menu.refresh") }}</span><kbd>Ctrl+R</kbd></button>
          <button type="button" @click="handleAppMenu('menu-fullscreen')"><span>{{ t("menu.fullscreen") }}</span><kbd>F11</kbd></button>
        </div>
      </div>
      <div class="app-menu-group" @mouseenter="switchOpenMenu('help')">
        <button type="button" :class="{ active: activeMenu === 'help' }" :aria-expanded="activeMenu === 'help'" @pointerdown.stop @click.stop="toggleMenu('help')">{{ t("menu.help") }}</button>
        <div v-if="activeMenu === 'help'" class="app-menu-popup" @pointerdown.stop>
          <button type="button" @click="handleAppMenu('menu-check-updates')"><span>{{ t("menu.checkUpdates") }}</span></button>
          <hr />
          <button type="button" @click="handleAppMenu('menu-about')"><span>{{ t("menu.about") }}</span></button>
        </div>
      </div>
    </nav>
    <div class="titlebar-drag" data-tauri-drag-region></div>
    <div class="window-controls" @dblclick.stop>
      <button type="button" :aria-label="t('window.minimize')" @click="minimizeWindow"><span class="window-minimize"></span></button>
      <button type="button" :aria-label="isMaximized ? t('window.restore') : t('window.maximize')" @click="toggleMaximizeWindow"><span :class="isMaximized ? 'window-restore' : 'window-maximize'"></span></button>
      <button type="button" class="window-close" :aria-label="t('window.close')" @click="closeWindow"><span>×</span></button>
    </div>
  </div>
  <div class="shell">
    <header><div class="brand"><h1>Kaven File Copy</h1><p>{{ t("app.tagline") }}</p></div><nav class="topbar-tabs" :aria-label="t('nav.aria')"><button type="button" :class="{ active: view === 'rules' || view === 'editor' }" @click="navigate('rules')">{{ t("nav.rules") }}</button><button type="button" :class="{ active: view === 'jobs' }" @click="navigate('jobs')">{{ t("nav.jobs") }}</button></nav><div class="view-switch lang-switch" role="group" :aria-label="t('lang.aria')"><button type="button" :class="{ active: locale === 'en' }" :aria-pressed="locale === 'en'" @click="changeLocale('en')">EN</button><button type="button" :class="{ active: locale === 'zh' }" :aria-pressed="locale === 'zh'" @click="changeLocale('zh')">中文</button></div></header>
    <p v-if="message" class="error">{{ message }}</p>
    <main v-if="view === 'rules'">
      <div class="toolbar"><h2>{{ t("rules.title") }}</h2><span></span><button @click="importRule">{{ t("action.import") }}</button><button class="primary" @click="edit()">{{ t("action.newRule") }}</button></div>
      <div v-if="!rules.length" class="empty">{{ t("rules.empty") }}</div>
      <article v-for="rule in rules" :key="rule.id" class="card rule-card" :title="t('rules.clickToEdit')" @click="edit(rule)">
        <div><h3>{{ rule.name }}</h3><p><button type="button" class="path-link" :title="t('action.openPathTitle')" @click.stop="openPath(rule.source.path)">{{ rule.source.path }}</button> <strong>→</strong> <button type="button" class="path-link" :title="t('action.openPathTitle')" @click.stop="openPath(rule.destination.path)">{{ rule.destination.path }}</button></p><small>{{ t(triggerLabelKeys[rule.trigger.type] ?? rule.trigger.type) }}{{ rule.trigger.type !== 'manual' ? ' · ' + (rule.enabled ? t('rules.status.enabled') : t('rules.status.disabled')) : '' }}</small></div>
        <div class="actions"><button @click.stop="exportRule(rule)">{{ t("action.export") }}</button><button @click.stop="duplicateRule(rule)">{{ t("action.duplicate") }}</button><button v-if="rule.trigger.type !== 'manual'" :disabled="busy" @click.stop="toggleRuleEnabled(rule)">{{ rule.enabled ? t('action.disable') : t('action.enable') }}</button><button @click.stop="removeRule(rule)">{{ t("action.delete") }}</button><button class="primary" :disabled="busy" @click.stop="requestRun(rule)">{{ t("action.run") }}</button></div>
      </article>
    </main>
    <main v-else-if="view === 'editor' && selectedRule">
      <div class="toolbar"><h2>{{ t("editor.title") }}</h2><span></span><button @click="cancelEdit">{{ t("action.cancel") }}</button><button class="primary" :disabled="busy" @click="persist">{{ t("action.save") }}</button></div>
      <section class="card form-grid">
        <label class="wide">{{ t("editor.name") }}<input v-model="selectedRule.name" /></label>
        <label class="wide">{{ t("editor.source") }}<div class="field-action"><input v-model="selectedRule.source.path" list="recent-source-paths" autocomplete="off" @change="clearSourceBrowser" /><button @click="choosePath('source', false)">{{ t("editor.chooseFile") }}</button><button @click="choosePath('source', true)">{{ t("editor.chooseFolder") }}</button></div><datalist id="recent-source-paths"><option v-for="path in recentPaths.sources" :key="path" :value="path"></option></datalist></label>
        <label class="wide">{{ t("editor.destination") }}<div class="field-action"><input v-model="selectedRule.destination.path" list="recent-destination-paths" autocomplete="off" /><button @click="choosePath('destination', true)">{{ t("action.browse") }}</button></div><datalist id="recent-destination-paths"><option v-for="path in recentPaths.destinations" :key="path" :value="path"></option></datalist></label>
        <label>{{ t("editor.compare") }}<select v-model="selectedRule.copy.compare"><option value="size-and-mtime">{{ t("editor.compare.sizeAndMtime") }}</option><option value="size-only">{{ t("editor.compare.sizeOnly") }}</option></select></label>
        <label>{{ t("editor.overwrite") }}<select v-model="selectedRule.copy.overwrite"><option value="if-changed">{{ t("editor.overwrite.ifChanged") }}</option><option value="never">{{ t("editor.overwrite.never") }}</option><option value="always">{{ t("editor.overwrite.always") }}</option></select></label>
        <label>{{ t("editor.trigger") }}<select :value="selectedRule.trigger.type" @change="changeTrigger"><option value="manual">{{ t("editor.trigger.manual") }}</option><option value="watch">{{ t("editor.trigger.watch") }}</option><option value="interval">{{ t("editor.trigger.interval") }}</option><option value="cron">{{ t("editor.trigger.cron") }}</option></select></label>
        <label>{{ t("editor.concurrency") }}<input v-model.number="selectedRule.copy.maxConcurrency" type="number" min="1" max="64" /></label>
        <label>{{ t("editor.retries") }}<input v-model.number="selectedRule.copy.retryCount" type="number" min="0" max="20" /></label>
        <label v-if="selectedRule.trigger.type === 'watch'">{{ t("editor.debounceMs") }}<input v-model.number="selectedRule.trigger.debounceMs" type="number" min="100" /></label>
        <label v-if="selectedRule.trigger.type === 'interval'">{{ t("editor.intervalSeconds") }}<input v-model.number="selectedRule.trigger.seconds" type="number" min="1" /></label>
        <label v-if="selectedRule.trigger.type === 'cron'" class="wide">{{ t("editor.cronExpression") }}<input v-model="selectedRule.trigger.expression" /></label>
        <label v-if="selectedRule.trigger.type !== 'manual'" class="checkbox"><input v-model="selectedRule.enabled" type="checkbox" /><span>{{ t("editor.ruleEnabled") }}</span></label>
        <label class="checkbox"><input v-model="selectedRule.filters.includeHidden" type="checkbox" /><span>{{ t("editor.includeHidden") }}</span></label>
        <label class="checkbox"><input v-model="selectedRule.copy.preserveMtime" type="checkbox" /><span>{{ t("editor.preserveMtime") }}</span></label>
        <label class="wide">{{ t("editor.destName") }}
          <div class="field-action">
            <template v-if="selectedRule.copy.rename">
              <input v-model="selectedRule.copy.rename.pattern" :placeholder="t('editor.renamePatternPlaceholder')" :aria-label="t('editor.renamePatternAria')" />
              <input v-model="selectedRule.copy.rename.destination" :placeholder="t('editor.renameDestPlaceholder')" :aria-label="t('editor.renameDestAria')" />
              <button type="button" @click="removeRename">{{ t("action.remove") }}</button>
            </template>
            <button v-else type="button" class="add-group" @click="addRename">{{ t("editor.addRename") }}</button>
          </div>
          <small class="hint">{{ t("editor.renameHint") }}</small>
          <small v-if="renameExample" class="hint rename-example">{{ renameExample }}</small>
        </label>
        <p v-if="selectedRule.trigger.type !== 'manual'" class="hint wide">{{ t("editor.autoJobHint") }}</p>
      </section>
      <section class="card"><div class="toolbar"><h3>{{ t("files.title") }}</h3><span></span><div class="view-switch" role="group" :aria-label="t('files.modeAria')">
        <button type="button" :class="{ active: sourceMode === 'manual' }" :aria-pressed="sourceMode === 'manual'" @click="setSourceMode('manual')">{{ t("files.mode.manual") }}</button>
        <button type="button" :class="{ active: sourceMode === 'groups' }" :aria-pressed="sourceMode === 'groups'" @click="setSourceMode('groups')">{{ t("files.mode.group") }}</button>
      </div><button @click="browseSource">{{ t("files.browse") }}</button></div>
        <p class="hint">{{ sourceMode === 'manual' ? t('files.manualHint') : t('files.groupHint') }}</p>
        <template v-if="sourceMode === 'manual'">
        <div v-if="sourceEntries.length" class="result-filters source-filters" role="search">
          <input v-model="sourceQuery" type="search" :placeholder="t('files.filterPath')" :aria-label="t('files.filterEntriesAria')" />
          <div class="view-switch" role="group" :aria-label="t('files.layoutAria')">
            <button type="button" :class="{ active: sourceView === 'list' }" :aria-pressed="sourceView === 'list'" @click="sourceView = 'list'">{{ t("files.view.list") }}</button>
            <button type="button" :class="{ active: sourceView === 'tree' }" :aria-pressed="sourceView === 'tree'" @click="sourceView = 'tree'">{{ t("files.view.tree") }}</button>
          </div>
          <small>{{ sourceView === 'list' ? t('files.countFiles', { n: sortedSourceFiles.length, m: sourceFiles.length }) : t('files.countEntries', { n: filteredSourceEntries.length, m: sourceEntries.length }) }}</small>
        </div>
        <ul v-if="sourceView === 'tree' && filteredSourceEntries.length" class="source-tree" role="tree" :aria-label="t('files.treeAria')">
          <SourceTreeNode
            v-for="node in sourceTree"
            :key="node.relativePath"
            :node="node"
            :included-paths="pathInclusions"
            :excluded-paths="pathExclusions"
            :renames="sourceRenames"
            :expand-all="sourceFilterActive"
            @toggle-included="toggleIncluded"
          />
        </ul>
        <div v-else-if="sourceView === 'list' && sortedSourceFiles.length" class="source-list-wrap">
          <table class="source-list">
            <colgroup>
              <col />
              <col class="size-col" />
              <col class="modified-col" />
            </colgroup>
            <thead>
              <tr>
                <th :aria-sort="ariaSort('name')">
                  <button type="button" class="sort-header" :class="{ active: sourceSortKey === 'name' }" @click="sortFilesBy('name')">
                    {{ t("files.col.name") }} <span v-if="sourceSortKey === 'name'">{{ sourceSortDirection === 'asc' ? '↑' : '↓' }}</span>
                  </button>
                </th>
                <th class="num" :aria-sort="ariaSort('size')">
                  <button type="button" class="sort-header" :class="{ active: sourceSortKey === 'size' }" @click="sortFilesBy('size')">
                    {{ t("files.col.size") }} <span v-if="sourceSortKey === 'size'">{{ sourceSortDirection === 'asc' ? '↑' : '↓' }}</span>
                  </button>
                </th>
                <th class="num" :aria-sort="ariaSort('modified')">
                  <button type="button" class="sort-header" :class="{ active: sourceSortKey === 'modified' }" @click="sortFilesBy('modified')">
                    {{ t("files.col.modified") }} <span v-if="sourceSortKey === 'modified'">{{ sourceSortDirection === 'asc' ? '↑' : '↓' }}</span>
                  </button>
                </th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="entry in sortedSourceFiles" :key="entry.relativePath">
                <td>
                  <label class="source-list-name">
                    <input
                      type="checkbox"
                      :checked="sourceEntryIncluded(entry.relativePath)"
                      :indeterminate="sourceEntryHasOverride(entry.relativePath)"
                      :aria-label="`Include ${entry.relativePath}`"
                      @change="toggleIncluded(entry.relativePath, ($event.target as HTMLInputElement).checked)"
                    />
                    <span :title="entry.relativePath">{{ entry.relativePath }}</span>
                    <small v-if="sourceRenames.get(entry.relativePath)" class="source-list-rename">→ {{ sourceRenames.get(entry.relativePath) }}</small>
                  </label>
                </td>
                <td class="num">{{ formatBytes(entry.size) }}</td>
                <td class="num">{{ formatModified(entry.modified) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <p v-else-if="sourceEntries.length" class="filter-empty">{{ sourceView === 'list' ? t('files.noMatch') : t('files.noMatchEntries') }}</p>
        </template>
        <template v-else>
          <div class="group-selection-options">
            <label class="checkbox"><input v-model="selectedRule.filters.autoLatestGroup" type="checkbox" /><span>{{ t("files.autoLatestGroup") }}</span></label>
            <label class="checkbox"><input :checked="selectedRule.filters.singleGroup" type="checkbox" @change="setSingleGroup(($event.target as HTMLInputElement).checked)" /><span>{{ t("files.singleGroup") }}</span></label>
            <label class="checkbox"><input v-model="selectedRule.filters.confirmGroupsBeforeRun" type="checkbox" /><span>{{ t("files.confirmGroupsBeforeRun") }}</span></label>
          </div>
          <p v-if="selectedRule.filters.autoLatestGroup" class="hint wide">{{ t("files.autoLatestHint") }}</p>
          <p v-else-if="selectedRule.filters.singleGroup" class="hint wide">{{ t("files.singleGroupHint") }}</p>
          <div class="group-rules">
            <div v-if="selectedRule.filters.group" class="field-action">
              <input v-model="selectedRule.filters.group.label" :placeholder="t('files.labelOptional')" :aria-label="t('files.groupLabelAria')" />
              <input v-model="selectedRule.filters.group.pattern" :placeholder="t('files.versionPattern')" :aria-label="t('files.versionPatternAria')" />
              <button type="button" @click="removeGroupRule">{{ t("action.remove") }}</button>
            </div>
            <button v-else type="button" class="add-group" @click="addGroupRule">{{ t("files.addRule") }}</button>
          </div>
          <div v-if="allGroupKeys.length" class="result-filters source-filters" role="search">
            <input v-model="sourceQuery" type="search" :placeholder="t('files.filterGroup')" :aria-label="t('files.filterGroupsAria')" />
            <div class="view-switch" role="group" :aria-label="t('files.groupLayoutAria')">
              <button type="button" :class="{ active: groupView === 'list' }" :aria-pressed="groupView === 'list'" @click="groupView = 'list'">{{ t("files.view.list") }}</button>
              <button type="button" :class="{ active: groupView === 'tree' }" :aria-pressed="groupView === 'tree'" @click="groupView = 'tree'">{{ t("files.view.tree") }}</button>
            </div>
            <div class="group-selection-actions">
              <button type="button" :disabled="selectedRule.filters.autoLatestGroup || selectedRule.filters.singleGroup || allGroupsSelected" @click="selectEveryGroup">{{ t("action.selectAll") }}</button>
              <button type="button" :disabled="selectedRule.filters.autoLatestGroup || selectedGroupCount === 0" @click="clearGroupSelection">{{ t("action.clearAll") }}</button>
            </div>
            <small>{{ t('files.countSelectedGroups', { selected: selectedGroupCount, total: allGroupKeys.length }) }}</small>
          </div>
          <ul v-if="groupView === 'tree' && groupTree.length" class="group-tree" role="tree" :aria-label="t('files.groupsAria')">
            <GroupTreeNode
              v-for="group in groupTree"
              :key="group.key"
              :group="group"
              :renames="sourceRenames"
              :expand-all="sourceFilterActive"
              :disabled="selectedRule.filters.autoLatestGroup"
              @toggle-group="toggleGroup"
            />
          </ul>
          <div v-else-if="groupView === 'list' && groupFileRows.length" class="source-list-wrap">
            <table class="source-list">
              <colgroup>
                <col />
                <col class="group-col" />
                <col class="size-col" />
                <col class="modified-col" />
              </colgroup>
              <thead>
                <tr>
                  <th>{{ t("files.col.name") }}</th>
                  <th>{{ t("files.col.group") }}</th>
                  <th class="num">{{ t("files.col.size") }}</th>
                  <th class="num">{{ t("files.col.modified") }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="row in groupFileRows" :key="row.relativePath">
                  <td>
                    <label class="source-list-name">
                      <input
                        v-if="row.groupKey"
                        type="checkbox"
                        :checked="row.selected"
                        :disabled="selectedRule.filters.autoLatestGroup"
                        :aria-label="t('files.includeGroupAria', { name: row.groupKey })"
                        @change="toggleGroup(row.groupKey, ($event.target as HTMLInputElement).checked)"
                      />
                      <span v-else class="tree-spacer" aria-hidden="true"></span>
                      <span :title="row.relativePath">{{ row.relativePath }}</span>
                      <small v-if="sourceRenames.get(row.relativePath)" class="source-list-rename">→ {{ sourceRenames.get(row.relativePath) }}</small>
                    </label>
                  </td>
                  <td>{{ row.groupLabel }}</td>
                  <td class="num">{{ formatBytes(row.size) }}</td>
                  <td class="num">{{ formatModified(row.modified) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <p v-else-if="sourceEntries.length" class="filter-empty">{{ t('files.noMatch') }}</p>
        </template>
      </section>
      <section class="card"><div class="toolbar"><h3>{{ t("preview.title") }}</h3><span></span><button class="primary" :disabled="busy" @click="runCurrentRule">{{ t("action.run") }}</button><button :disabled="busy" @click="preview">{{ previewProgress ? t('preview.buttonProgress') : t('preview.button') }}</button></div>
        <div v-if="previewProgress" class="preview-progress" role="status" aria-live="polite">
          <progress v-if="previewProgress.phase === 'planning'" :value="previewProgress.completedEntries" :max="previewProgress.totalEntries || 1"></progress>
          <progress v-else></progress>
          <p><span v-if="previewProgress.phase === 'scanning'">{{ t('preview.scanning', { n: previewProgress.completedEntries }) }}</span><span v-else>{{ t('preview.building', { n: previewProgress.completedEntries, m: previewProgress.totalEntries ?? 0 }) }}</span><small v-if="previewProgress.relativePath">{{ previewProgress.relativePath }}</small></p>
        </div>
        <p v-if="plan">{{ t('preview.stats', { files: plan.stats.filesToCopy, bytes: formatBytes(plan.stats.bytesToCopy), skipped: plan.stats.filesToSkip, warnings: plan.stats.warnings }) }}</p>
        <div v-if="plan && plan.items.length" class="result-filters preview-filters" role="search">
          <input v-model="previewQuery" type="search" :placeholder="t('preview.filterPath')" :aria-label="t('preview.filterAria')" />
          <select v-model="previewAction" :aria-label="t('preview.filterActionAria')">
            <option value="all">{{ t("preview.action.all") }}</option>
            <option value="copy-file">{{ t("preview.action.copyFile") }}</option>
            <option value="create-directory">{{ t("preview.action.createDirectory") }}</option>
            <option value="skip">{{ t("preview.action.skip") }}</option>
            <option value="warn">{{ t("preview.action.warn") }}</option>
          </select>
          <small>{{ t('preview.countItems', { n: filteredPlanItems.length, m: plan.items.length }) }}</small>
        </div>
        <div v-if="plan && sortedPlanItems.length" class="source-list-wrap">
          <table class="source-list">
            <colgroup>
              <col class="preview-action-col" />
              <col />
              <col />
              <col class="preview-desc-col" />
            </colgroup>
            <thead>
              <tr>
                <th :aria-sort="previewAriaSort('action')">
                  <button type="button" class="sort-header" :class="{ active: previewSortKey === 'action' }" @click="previewSortBy('action')">
                    {{ t("preview.col.action") }} <span v-if="previewSortKey === 'action'">{{ previewSortDirection === 'asc' ? '↑' : '↓' }}</span>
                  </button>
                </th>
                <th :aria-sort="previewAriaSort('source')">
                  <button type="button" class="sort-header" :class="{ active: previewSortKey === 'source' }" @click="previewSortBy('source')">
                    {{ t("preview.col.source") }} <span v-if="previewSortKey === 'source'">{{ previewSortDirection === 'asc' ? '↑' : '↓' }}</span>
                  </button>
                </th>
                <th :aria-sort="previewAriaSort('target')">
                  <button type="button" class="sort-header" :class="{ active: previewSortKey === 'target' }" @click="previewSortBy('target')">
                    {{ t("preview.col.target") }} <span v-if="previewSortKey === 'target'">{{ previewSortDirection === 'asc' ? '↑' : '↓' }}</span>
                  </button>
                </th>
                <th :aria-sort="previewAriaSort('description')">
                  <button type="button" class="sort-header" :class="{ active: previewSortKey === 'description' }" @click="previewSortBy('description')">
                    {{ t("preview.col.description") }} <span v-if="previewSortKey === 'description'">{{ previewSortDirection === 'asc' ? '↑' : '↓' }}</span>
                  </button>
                </th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="item in sortedPlanItems" :key="`${item.action}-${item.relativePath}`">
                <td><code>{{ item.action }}</code></td>
                <td class="preview-src">{{ item.relativePath }}</td>
                <td :class="{ renamed: item.destinationPath && item.destinationPath !== item.relativePath }">{{ item.destinationPath ?? item.relativePath }}</td>
                <td class="preview-desc"><small v-if="item.warning" class="plan-warning">⚠ {{ item.warning }}</small> {{ item.reason }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <p v-else-if="plan && plan.items.length" class="filter-empty">{{ t('preview.noMatch') }}</p>
        <p v-else-if="plan" class="filter-empty">{{ t('preview.noPlanned') }}</p>
      </section>
    </main>
    <main v-else>
      <div class="toolbar"><h2>{{ t("jobs.title") }}</h2><span></span><button @click="refresh">{{ t("action.refresh") }}</button><button class="danger" :disabled="busy || !jobs.length" @click="removeAllJobs">{{ t("action.removeAll") }}</button></div>
      <section v-if="progress" class="card"><div class="toolbar"><h3>{{ t("jobs.current") }}</h3><span></span><button @click="cancelJob">{{ t("action.cancel") }}</button></div><progress :value="progress.completedFiles" :max="progress.totalFiles || 1"></progress><p>{{ t('jobs.progress', { completed: progress.completedFiles, total: progress.totalFiles ?? '', path: progress.relativePath ?? '' }) }}</p></section>
      <article v-for="job in jobs" :key="job.jobId" class="card"><div class="toolbar"><strong :class="`status ${job.status}`">{{ job.status }}</strong><span></span><time>{{ new Date(job.finishedAt).toLocaleString() }}</time><button :disabled="busy" @click="removeJob(job)">{{ t("action.remove") }}</button></div><p>{{ t('jobs.summary', { copied: job.copiedFiles, skipped: job.skippedFiles, failed: job.failedFiles, bytes: formatBytes(job.copiedBytes) }) }}</p><details @toggle="($event.target as HTMLDetailsElement).open && loadJobEvents(job.jobId)"><summary>{{ t("jobs.log") }}</summary><div class="plan"><div v-for="event in jobEvents[job.jobId] ?? []" :key="`${event.timestamp}-${event.relativePath}`"><code>{{ event.result }}</code> {{ event.relativePath }} <small>{{ event.message }}</small></div></div></details></article>
    </main>
    <footer class="app-footer">
      <span>{{ t("footer.copyright", { year: new Date().getFullYear() }) }}</span>
      <span v-if="appVersion" class="app-footer-version">{{ t("footer.version") }} {{ appVersion }}</span>
    </footer>
  </div>
  <div v-if="groupRunDialog" class="modal-backdrop" @click.self="cancelGroupRun">
    <div class="modal group-run-modal" role="dialog" aria-modal="true" :aria-label="t('run.selectGroupsTitle')">
      <h3>{{ t("run.selectGroupsTitle") }}</h3>
      <p>{{ t("run.selectGroupsHint") }}</p>
      <div class="group-run-tools">
        <input v-model="groupRunDialog.query" type="search" :placeholder="t('files.filterGroup')" :aria-label="t('files.filterGroupsAria')" />
        <div class="group-selection-actions">
          <button type="button" :disabled="groupRunDialog.rule.filters.singleGroup || allRunGroupsSelected" @click="selectEveryRunGroup">{{ t("action.selectAll") }}</button>
          <button type="button" :disabled="groupRunDialog.selectedGroups.length === 0" @click="clearRunGroupSelection">{{ t("action.clearAll") }}</button>
        </div>
      </div>
      <small class="group-run-count">{{ t('files.countSelectedGroups', { selected: groupRunDialog.selectedGroups.length, total: allRunGroupKeys.length }) }}</small>
      <ul v-if="groupRunGroups.length" class="group-run-list">
        <li v-for="group in groupRunGroups" :key="group.key">
          <label>
            <input
              type="checkbox"
              :checked="group.selected"
              :aria-label="t('files.includeGroupAria', { name: group.key })"
              @change="toggleRunGroup(group.key, ($event.target as HTMLInputElement).checked)"
            />
            <strong>{{ group.label ? `${group.label} ${group.key}` : group.key }}</strong>
            <small>{{ t('group.files', { n: group.matched.length }) }}</small>
          </label>
        </li>
      </ul>
      <p v-else class="run-details-empty">{{ t("files.noMatch") }}</p>
      <div class="actions">
        <button type="button" @click="cancelGroupRun">{{ t("action.cancel") }}</button>
        <button type="button" class="primary" :disabled="groupRunDialog.selectedGroups.length === 0" @click="confirmGroupRun">{{ t("run.executeSelection") }}</button>
      </div>
    </div>
  </div>
  <div v-if="confirmDialog" class="modal-backdrop" @click.self="resolveConfirm(false)">
    <div class="modal" role="alertdialog" aria-modal="true">
      <h3>{{ confirmDialog.title }}</h3>
      <p>{{ confirmDialog.message }}</p>
      <div class="actions">
        <button type="button" @click="resolveConfirm(false)">{{ t("action.cancel") }}</button>
        <button type="button" class="primary" @click="resolveConfirm(true)">{{ t("action.confirm") }}</button>
      </div>
    </div>
  </div>
  <div v-if="noticeDialog" class="modal-backdrop" @click.self="noticeDialog = null">
    <div class="modal" role="dialog" aria-modal="true">
      <h3>{{ noticeDialog.title }}</h3>
      <p class="notice-message">{{ noticeDialog.message }}</p>
      <div class="actions">
        <button type="button" class="primary" @click="noticeDialog = null">{{ t("action.close") }}</button>
      </div>
    </div>
  </div>
  <div v-if="runModal" class="modal-backdrop run-backdrop">
    <div class="modal run-modal" role="alertdialog" aria-modal="true" aria-live="polite">
      <h3>{{ runFinished ? t("run.done") : t("run.title") }}</h3>
      <template v-if="!runFinished">
        <progress v-if="progress" :value="progress.completedFiles" :max="progress.totalFiles || 1"></progress>
        <progress v-else></progress>
        <p class="run-status">{{ progress ? t('jobs.progress', { completed: progress.completedFiles, total: progress.totalFiles ?? '', path: progress.relativePath ?? '' }) : '' }}</p>
      </template>
      <template v-else-if="runResult">
        <p class="run-result"><strong :class="`status ${runResult.status}`">{{ runResult.status }}</strong></p>
        <p class="run-summary">{{ t('jobs.summary', { copied: runResult.copiedFiles, skipped: runResult.skippedFiles, failed: runResult.failedFiles, bytes: formatBytes(runResult.copiedBytes) }) }}</p>
        <h4 class="run-details-title">{{ t('run.details', { count: runDetails.length }) }}</h4>
        <ul v-if="runDetails.length" class="run-details">
          <li v-for="detail in runDetails" :key="`${detail.result}-${detail.relativePath}`" class="run-detail">
            <span class="run-detail-result" :class="detail.result">{{ t(`run.result.${detail.result}`) }}</span>
            <span class="run-detail-path" :title="detail.relativePath">{{ detail.relativePath }}</span>
            <small v-if="detail.message" class="run-detail-message">{{ detail.message }}</small>
          </li>
        </ul>
        <p v-else class="run-details-empty">{{ t("run.noDetails") }}</p>
      </template>
      <template v-else>
        <p class="run-result">{{ t("run.failed") }}</p>
      </template>
      <div class="actions">
        <button v-if="!runFinished" type="button" class="danger" @click="cancelRun">{{ t("action.cancel") }}</button>
        <button v-else type="button" class="primary" @click="closeRun">{{ t("action.close") }}</button>
      </div>
    </div>
  </div>
</template>
