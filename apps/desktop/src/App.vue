<script setup lang="ts">
import { computed, onMounted, reactive, ref, toRaw } from "vue";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { api } from "./api/tauri";
import { locale, setLocale, t } from "./i18n";
import { filterPlanItems, sortPreviewItems, type PreviewActionFilter, type PreviewSortDirection, type PreviewSortKey } from "./preview-filter";
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
  isPathIncluded,
  latestGroupKey,
  normalizeTreePath,
  setPathIncluded,
  sortSourceEntries,
  type SourceSort,
  type SourceSortKey,
  type SourceView,
  type SortDirection,
} from "./source-tree";
import type { CopyPlan, CopyRule, JobEvent, JobSummary, PlanProgressEvent, ProgressEvent, RecentPaths, SelectionMode, SourceEntry } from "./types/generated";

type View = "rules" | "editor" | "jobs";
const view = ref<View>("rules");
const rules = ref<CopyRule[]>([]);
const jobs = ref<JobSummary[]>([]);
const selectedRule = ref<CopyRule | null>(null);
const originalRule = ref<CopyRule | null>(null);
const confirmDialog = ref<{ title: string; message: string; onConfirm: () => void; onCancel?: () => void } | null>(null);
const plan = ref<CopyPlan | null>(null);
const sourceEntries = ref<SourceEntry[]>([]);
const sourceQuery = ref("");
const sourceView = ref<SourceView>("tree");
const groupView = ref<"tree" | "list">("tree");
const sourceMode = ref<SelectionMode>("manual");
const sourceSortKey = ref<SourceSortKey>("name");
const sourceSortDirection = ref<SortDirection>("asc");
const recentPaths = ref<RecentPaths>({ sources: [], destinations: [] });
const busy = ref(false);
const message = ref("");
const progress = ref<ProgressEvent | null>(null);
const runModal = ref(false);
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
    filters: { exclude: [], include: [], includeHidden: true, group: null, selectedGroups: [], autoLatestGroup: false, selectionMode: "manual" },
    copy: { mode: "incremental", compare: "size-and-mtime", overwrite: "if-changed", symlinks: "skip", preserveMtime: true, maxConcurrency: 4, retryCount: 2, rename: null },
    trigger: { type: "manual" },
  };
}

async function refresh() {
  [rules.value, jobs.value] = await Promise.all([api.listRules(), api.listJobs()]);
  try {
    recentPaths.value = await api.getRecentPaths();
  } catch (error) {
    message.value = t("msg.loadRecentPaths", { error: String(error) });
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
  const selected = new Set(selectedRule.value.filters.selectedGroups);
  if (checked) {
    selected.add(name);
  } else {
    selected.delete(name);
  }
  selectedRule.value.filters.selectedGroups = [...selected];
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

function navigate(target: "rules" | "jobs") {
  if (view.value !== "editor" || !selectedRule.value) {
    view.value = target;
    return;
  }
  const dirty = JSON.stringify(toRaw(selectedRule.value)) !== JSON.stringify(originalRule.value);
  if (dirty) {
    askConfirm(
      t("dialog.discardTitle"),
      t("dialog.discardMessage"),
      () => { view.value = target; },
    );
    return;
  }
  view.value = target;
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
  await act(async () => {
    await rememberRulePaths(rule);
    const summary = await api.executeRule(structuredClone(toRaw(rule)));
    jobs.value = [summary, ...jobs.value.filter((job) => job.jobId !== summary.jobId)];
    progress.value = null;
  });
  runModal.value = false;
}

function cancelRun() {
  runModal.value = false;
  void cancelJob();
}

function runCurrentRule() {
  if (!selectedRule.value) return;
  void run(selectedRule.value);
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

onMounted(async () => {
  await listen<PlanProgressEvent>("preview-progress", (event) => { previewProgress.value = event.payload; });
  await listen<ProgressEvent>("job-progress", (event) => { progress.value = event.payload; });
  await listen<JobSummary>("job-finished", (event) => {
    jobs.value = [event.payload, ...jobs.value.filter((job) => job.jobId !== event.payload.jobId)];
    progress.value = null;
  });
  await listen<string>("scheduler-error", (event) => { message.value = event.payload; });
  await act(refresh);
});
</script>

<template>
  <div class="shell">
    <header><div class="brand"><h1>Kaven File Copy</h1><p>{{ t("app.tagline") }}</p></div><nav class="topbar-tabs" :aria-label="t('nav.aria')"><button type="button" :class="{ active: view === 'rules' || view === 'editor' }" @click="navigate('rules')">{{ t("nav.rules") }}</button><button type="button" :class="{ active: view === 'jobs' }" @click="navigate('jobs')">{{ t("nav.jobs") }}</button></nav><div class="view-switch lang-switch" role="group" :aria-label="t('lang.aria')"><button type="button" :class="{ active: locale === 'en' }" :aria-pressed="locale === 'en'" @click="setLocale('en')">EN</button><button type="button" :class="{ active: locale === 'zh' }" :aria-pressed="locale === 'zh'" @click="setLocale('zh')">中文</button></div></header>
    <p v-if="message" class="error">{{ message }}</p>
    <main v-if="view === 'rules'">
      <div class="toolbar"><h2>{{ t("rules.title") }}</h2><span></span><button @click="importRule">{{ t("action.import") }}</button><button class="primary" @click="edit()">{{ t("action.newRule") }}</button></div>
      <div v-if="!rules.length" class="empty">{{ t("rules.empty") }}</div>
      <article v-for="rule in rules" :key="rule.id" class="card rule-card" :title="t('rules.clickToEdit')" @click="edit(rule)">
        <div><h3>{{ rule.name }}</h3><p>{{ rule.source.path }} <strong>→</strong> {{ rule.destination.path }}</p><small>{{ t(triggerLabelKeys[rule.trigger.type] ?? rule.trigger.type) }}{{ rule.trigger.type !== 'manual' ? ' · ' + (rule.enabled ? t('rules.status.enabled') : t('rules.status.disabled')) : '' }}</small></div>
        <div class="actions"><button @click.stop="exportRule(rule)">{{ t("action.export") }}</button><button @click.stop="duplicateRule(rule)">{{ t("action.duplicate") }}</button><button v-if="rule.trigger.type !== 'manual'" :disabled="busy" @click.stop="toggleRuleEnabled(rule)">{{ rule.enabled ? t('action.disable') : t('action.enable') }}</button><button @click.stop="removeRule(rule)">{{ t("action.delete") }}</button><button class="primary" :disabled="busy" @click.stop="run(rule)">{{ t("action.run") }}</button></div>
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
          <label class="checkbox"><input v-model="selectedRule.filters.autoLatestGroup" type="checkbox" /><span>{{ t("files.autoLatestGroup") }}</span></label>
          <p v-if="selectedRule.filters.autoLatestGroup" class="hint wide">{{ t("files.autoLatestHint") }}</p>
          <div class="group-rules">
            <div v-if="selectedRule.filters.group" class="field-action">
              <input v-model="selectedRule.filters.group.label" :placeholder="t('files.labelOptional')" :aria-label="t('files.groupLabelAria')" />
              <input v-model="selectedRule.filters.group.pattern" :placeholder="t('files.versionPattern')" :aria-label="t('files.versionPatternAria')" />
              <button type="button" @click="removeGroupRule">{{ t("action.remove") }}</button>
            </div>
            <button v-else type="button" class="add-group" @click="addGroupRule">{{ t("files.addRule") }}</button>
          </div>
          <div v-if="groupTree.length" class="result-filters source-filters" role="search">
            <input v-model="sourceQuery" type="search" :placeholder="t('files.filterGroup')" :aria-label="t('files.filterGroupsAria')" />
            <div class="view-switch" role="group" :aria-label="t('files.groupLayoutAria')">
              <button type="button" :class="{ active: groupView === 'list' }" :aria-pressed="groupView === 'list'" @click="groupView = 'list'">{{ t("files.view.list") }}</button>
              <button type="button" :class="{ active: groupView === 'tree' }" :aria-pressed="groupView === 'tree'" @click="groupView = 'tree'">{{ t("files.view.tree") }}</button>
            </div>
            <small>{{ t('files.countGroups', { n: groupTree.length }) }}</small>
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
        <p v-if="plan">{{ t('preview.stats', { files: plan.stats.filesToCopy, bytes: plan.stats.bytesToCopy, skipped: plan.stats.filesToSkip, warnings: plan.stats.warnings }) }}</p>
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
      <article v-for="job in jobs" :key="job.jobId" class="card"><div class="toolbar"><strong :class="`status ${job.status}`">{{ job.status }}</strong><span></span><time>{{ new Date(job.finishedAt).toLocaleString() }}</time><button :disabled="busy" @click="removeJob(job)">{{ t("action.remove") }}</button></div><p>{{ t('jobs.summary', { copied: job.copiedFiles, skipped: job.skippedFiles, failed: job.failedFiles, bytes: job.copiedBytes }) }}</p><details @toggle="($event.target as HTMLDetailsElement).open && loadJobEvents(job.jobId)"><summary>{{ t("jobs.log") }}</summary><div class="plan"><div v-for="event in jobEvents[job.jobId] ?? []" :key="`${event.timestamp}-${event.relativePath}`"><code>{{ event.result }}</code> {{ event.relativePath }} <small>{{ event.message }}</small></div></div></details></article>
    </main>
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
  <div v-if="runModal" class="modal-backdrop run-backdrop">
    <div class="modal run-modal" role="alertdialog" aria-modal="true" aria-live="polite">
      <h3>{{ t("run.title") }}</h3>
      <progress v-if="progress" :value="progress.completedFiles" :max="progress.totalFiles || 1"></progress>
      <progress v-else></progress>
      <p class="run-status">{{ progress ? t('jobs.progress', { completed: progress.completedFiles, total: progress.totalFiles ?? '', path: progress.relativePath ?? '' }) : '' }}</p>
      <div class="actions">
        <button type="button" class="danger" @click="cancelRun">{{ t("action.cancel") }}</button>
      </div>
    </div>
  </div>
</template>
