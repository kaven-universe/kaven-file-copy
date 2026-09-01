<script setup lang="ts">
import { computed, onMounted, reactive, ref, toRaw } from "vue";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { api } from "./api/tauri";
import { buildPreviewTree, filterPlanItems, type PreviewActionFilter, type PreviewView } from "./preview-filter";
import PreviewTreeNode from "./PreviewTreeNode.vue";
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
const previewProgress = ref<PlanProgressEvent | null>(null);
const previewQuery = ref("");
const previewAction = ref<PreviewActionFilter>("all");
const previewView = ref<PreviewView>("list");
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
const groupTree = computed(() => buildGroupTree(
  filteredSourceEntries.value,
  selectedRule.value?.filters.group ?? null,
  selectedRule.value?.filters.selectedGroups ?? [],
  sourceQuery.value,
));
const groupFileRows = computed(() => groupTree.value.flatMap((group) =>
  group.matched.map((file) => ({
    ...file,
    groupKey: group.key,
    groupLabel: group.key
      ? (group.label ? `${group.label} ${group.key}` : group.key)
      : group.label,
    selected: group.selected,
  })),
));
const sourceFilterActive = computed(() => sourceQuery.value.trim().length > 0);
const filteredPlanItems = computed(() => filterPlanItems(
  plan.value?.items ?? [],
  previewAction.value,
  previewQuery.value,
));
const previewTree = computed(() => buildPreviewTree(filteredPlanItems.value));
const previewFilterActive = computed(() => (
  previewAction.value !== "all" || previewQuery.value.trim().length > 0
));

function newRule(): CopyRule {
  return {
    schemaVersion: 1, id: crypto.randomUUID(), name: "New copy rule", enabled: true,
    source: { type: "local", path: "" }, destination: { type: "local", path: "" },
    filters: { exclude: [], include: [], includeHidden: true, group: null, selectedGroups: [], selectionMode: "manual" },
    copy: { mode: "incremental", compare: "size-and-mtime", overwrite: "if-changed", symlinks: "skip", preserveMtime: true, maxConcurrency: 4, retryCount: 2, rename: null },
    trigger: { type: "manual" },
  };
}

async function refresh() {
  [rules.value, jobs.value] = await Promise.all([api.listRules(), api.listJobs()]);
  try {
    recentPaths.value = await api.getRecentPaths();
  } catch (error) {
    message.value = `Could not load recent paths: ${String(error)}`;
  }
}

async function rememberRecentPath(kind: "source" | "destination", path: string) {
  if (!path.trim()) return;
  try {
    recentPaths.value = await api.rememberRecentPath(kind, path);
  } catch (error) {
    message.value = `Could not update recent paths: ${String(error)}`;
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
  copy.filters.selectionMode ??= "manual";
  copy.copy.rename ??= null;
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
  if (!selectedRule.value) return;
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
  const first = sourceEntries.value.find(
    (entry) => !entry.directory && !normalizeTreePath(entry.relativePath).includes("/"),
  );
  if (!first) return "Browse the source tree to preview the renamed name.";
  const name = normalizeTreePath(first.relativePath).split("/").pop() ?? "";
  const newName = applyRename(rename.pattern, rename.destination, name);
  return newName && newName !== name
    ? `${name} → ${newName}`
    : `${name} does not match the pattern; it will be copied as-is.`;
});

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

async function previewSavedRule(rule: CopyRule) {
  edit(rule);
  await preview();
}

async function run(rule: CopyRule) {
  progress.value = null;
  view.value = "jobs";
  await act(async () => {
    await rememberRulePaths(rule);
    const summary = await api.executeRule(structuredClone(toRaw(rule)));
    jobs.value = [summary, ...jobs.value.filter((job) => job.jobId !== summary.jobId)];
    progress.value = null;
  });
}

async function toggleRuleEnabled(rule: CopyRule) {
  const updated = structuredClone(toRaw(rule));
  updated.enabled = !updated.enabled;
  await act(async () => { await api.saveRule(updated); await refresh(); });
}

function duplicateRule(rule: CopyRule) {
  const copy = structuredClone(toRaw(rule));
  copy.id = crypto.randomUUID();
  copy.name = `${copy.name} (copy)`;
  edit(copy);
}

async function removeRule(rule: CopyRule) {
  if (!confirm(`Delete rule “${rule.name}”? Copied files will not be deleted.`)) return;
  await act(async () => { await api.deleteRule(rule.id); await refresh(); });
}

async function loadJobEvents(jobId: string) {
  if (jobEvents.value[jobId]) return;
  await act(async () => { jobEvents.value[jobId] = await api.getJobEvents(jobId); });
}

async function cancelJob() {
  await act(async () => { await api.cancelJob(); });
}

async function exportRule(rule: CopyRule) {
  const path = await save({ defaultPath: `${rule.name.replaceAll(/[^a-zA-Z0-9_-]/g, "-")}.kfc.json` });
  if (path) await writeTextFile(path, JSON.stringify(rule, null, 2) + "\n");
}

async function importRule() {
  const path = await open({ multiple: false, filters: [{ name: "KFC rule", extensions: ["json"] }] });
  if (!path) return;
  await act(async () => {
    const rule = JSON.parse(await readTextFile(path)) as CopyRule;
    await api.validateRule(rule);
    if (rules.value.some((existing) => existing.id === rule.id)) {
      const overwrite = confirm(`A rule named “${rule.name}” already uses this ID. Select OK to overwrite it, or Cancel to import a new copy.`);
      if (!overwrite) {
        rule.id = crypto.randomUUID();
        rule.name = `${rule.name} (imported)`;
      }
    }
    edit(rule);
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
    <header><div><h1>Kaven File Copy</h1><p>Safe, reusable local copy rules</p></div><nav><button @click="view='rules'">Rules</button><button @click="view='jobs'">Jobs</button></nav></header>
    <p v-if="message" class="error">{{ message }}</p>
    <main v-if="view === 'rules'">
      <div class="toolbar"><h2>Copy rules</h2><span></span><button @click="importRule">Import</button><button class="primary" @click="edit()">New rule</button></div>
      <div v-if="!rules.length" class="empty">No rules yet. Create one to start copying.</div>
      <article v-for="rule in rules" :key="rule.id" class="card rule-card">
        <div><h3>{{ rule.name }}</h3><p>{{ rule.source.path }} <strong>→</strong> {{ rule.destination.path }}</p><small>{{ rule.trigger.type }} · {{ rule.enabled ? 'enabled' : 'disabled' }}</small></div>
        <div class="actions"><button @click="exportRule(rule)">Export</button><button @click="duplicateRule(rule)">Duplicate</button><button :disabled="busy" @click="previewSavedRule(rule)">Preview</button><button @click="edit(rule)">Edit</button><button :disabled="busy" @click="toggleRuleEnabled(rule)">{{ rule.enabled ? 'Disable' : 'Enable' }}</button><button @click="removeRule(rule)">Delete</button><button class="primary" :disabled="busy" @click="run(rule)">Run</button></div>
      </article>
    </main>
    <main v-else-if="view === 'editor' && selectedRule">
      <div class="toolbar"><h2>Edit rule</h2><span></span><button @click="view='rules'">Cancel</button><button class="primary" :disabled="busy" @click="persist">Save</button></div>
      <section class="card form-grid">
        <label class="wide">Rule name<input v-model="selectedRule.name" /></label>
        <label class="wide">Source<div class="field-action"><input v-model="selectedRule.source.path" list="recent-source-paths" autocomplete="off" @change="clearSourceBrowser" /><button @click="choosePath('source', false)">Choose file</button><button @click="choosePath('source', true)">Choose folder</button></div><datalist id="recent-source-paths"><option v-for="path in recentPaths.sources" :key="path" :value="path"></option></datalist></label>
        <label class="wide">Destination<div class="field-action"><input v-model="selectedRule.destination.path" list="recent-destination-paths" autocomplete="off" /><button @click="choosePath('destination', true)">Browse</button></div><datalist id="recent-destination-paths"><option v-for="path in recentPaths.destinations" :key="path" :value="path"></option></datalist></label>
        <label>Compare<select v-model="selectedRule.copy.compare"><option value="size-and-mtime">Size and modified time</option><option value="size-only">Size only</option></select></label>
        <label>Overwrite<select v-model="selectedRule.copy.overwrite"><option value="if-changed">If changed</option><option value="never">Never</option><option value="always">Always</option></select></label>
        <label>Trigger<select :value="selectedRule.trigger.type" @change="changeTrigger"><option value="manual">Manual</option><option value="watch">Watch changes</option><option value="interval">Fixed interval</option><option value="cron">Cron</option></select></label>
        <label>Concurrency<input v-model.number="selectedRule.copy.maxConcurrency" type="number" min="1" max="64" /></label>
        <label>Retries<input v-model.number="selectedRule.copy.retryCount" type="number" min="0" max="20" /></label>
        <label v-if="selectedRule.trigger.type === 'watch'">Debounce (ms)<input v-model.number="selectedRule.trigger.debounceMs" type="number" min="100" /></label>
        <label v-if="selectedRule.trigger.type === 'interval'">Interval (seconds)<input v-model.number="selectedRule.trigger.seconds" type="number" min="1" /></label>
        <label v-if="selectedRule.trigger.type === 'cron'" class="wide">Cron expression<input v-model="selectedRule.trigger.expression" /></label>
        <label class="checkbox"><input v-model="selectedRule.enabled" type="checkbox" /><span>Rule enabled</span></label>
        <label class="checkbox"><input v-model="selectedRule.filters.includeHidden" type="checkbox" /><span>Include hidden files</span></label>
        <label class="checkbox"><input v-model="selectedRule.copy.preserveMtime" type="checkbox" /><span>Preserve modification times</span></label>
        <label class="wide">Destination name
          <div class="field-action">
            <template v-if="selectedRule.copy.rename">
              <input v-model="selectedRule.copy.rename.pattern" placeholder="Source name pattern, e.g. ^(.+)_(\d+\.\d+\.\d+\.\d+)_(.+)\.(.+)$" aria-label="Source name pattern" />
              <input v-model="selectedRule.copy.rename.destination" placeholder="Destination template, e.g. $1_$2.$4" aria-label="Destination template" />
              <button type="button" @click="removeRename">Remove</button>
            </template>
            <button v-else type="button" class="add-group" @click="addRename">Add rename rule</button>
          </div>
          <small class="hint">Match the source file name with a regex; $1..$9 in the template become capture groups.</small>
          <small v-if="renameExample" class="hint rename-example">{{ renameExample }}</small>
        </label>
        <p v-if="selectedRule.trigger.type !== 'manual'" class="hint wide">Automatic jobs run only while the desktop application is open.</p>
      </section>
      <section class="card"><div class="toolbar"><h3>Files to copy</h3><span></span><div class="view-switch" role="group" aria-label="Selection mode">
        <button type="button" :class="{ active: sourceMode === 'manual' }" :aria-pressed="sourceMode === 'manual'" @click="setSourceMode('manual')">Manual</button>
        <button type="button" :class="{ active: sourceMode === 'groups' }" :aria-pressed="sourceMode === 'groups'" @click="setSourceMode('groups')">Group</button>
      </div><button @click="browseSource">Browse source tree</button></div>
        <p class="hint">{{ sourceMode === 'manual' ? 'Check the files and folders to copy. Nothing is copied until you make a selection; unchecked entries are skipped.' : 'Group mode extracts a version from each top-level file name; select the versions to copy.' }}</p>
        <template v-if="sourceMode === 'manual'">
        <div v-if="sourceEntries.length" class="result-filters source-filters" role="search">
          <input v-model="sourceQuery" type="search" placeholder="Filter source path" aria-label="Filter source entries by path" />
          <div class="view-switch" role="group" aria-label="Source layout">
            <button type="button" :class="{ active: sourceView === 'list' }" :aria-pressed="sourceView === 'list'" @click="sourceView = 'list'">List</button>
            <button type="button" :class="{ active: sourceView === 'tree' }" :aria-pressed="sourceView === 'tree'" @click="sourceView = 'tree'">Tree</button>
          </div>
          <small>{{ sourceView === 'list' ? `${sortedSourceFiles.length} of ${sourceFiles.length} files` : `${filteredSourceEntries.length} of ${sourceEntries.length} entries` }}</small>
        </div>
        <ul v-if="sourceView === 'tree' && filteredSourceEntries.length" class="source-tree" role="tree" aria-label="Files to copy">
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
                    Name <span v-if="sourceSortKey === 'name'">{{ sourceSortDirection === 'asc' ? '↑' : '↓' }}</span>
                  </button>
                </th>
                <th class="num" :aria-sort="ariaSort('size')">
                  <button type="button" class="sort-header" :class="{ active: sourceSortKey === 'size' }" @click="sortFilesBy('size')">
                    Size <span v-if="sourceSortKey === 'size'">{{ sourceSortDirection === 'asc' ? '↑' : '↓' }}</span>
                  </button>
                </th>
                <th class="num" :aria-sort="ariaSort('modified')">
                  <button type="button" class="sort-header" :class="{ active: sourceSortKey === 'modified' }" @click="sortFilesBy('modified')">
                    Modified <span v-if="sourceSortKey === 'modified'">{{ sourceSortDirection === 'asc' ? '↑' : '↓' }}</span>
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
        <p v-else-if="sourceEntries.length" class="filter-empty">{{ sourceView === 'list' ? 'No files match the current filter.' : 'No source entries match the current filter.' }}</p>
        </template>
        <template v-else>
          <div class="group-rules">
            <div v-if="selectedRule.filters.group" class="field-action">
              <input v-model="selectedRule.filters.group.label" placeholder="Label (optional)" aria-label="Group label" />
              <input v-model="selectedRule.filters.group.pattern" placeholder="Version pattern, e.g. (\\d+\\.\\d+\\.\\d+\\.\\d+)" aria-label="Version pattern" />
              <button type="button" @click="removeGroupRule">Remove</button>
            </div>
            <button v-else type="button" class="add-group" @click="addGroupRule">Add rule</button>
          </div>
          <div v-if="groupTree.length" class="result-filters source-filters" role="search">
            <input v-model="sourceQuery" type="search" placeholder="Filter group or file" aria-label="Filter groups and files" />
            <div class="view-switch" role="group" aria-label="Group layout">
              <button type="button" :class="{ active: groupView === 'list' }" :aria-pressed="groupView === 'list'" @click="groupView = 'list'">List</button>
              <button type="button" :class="{ active: groupView === 'tree' }" :aria-pressed="groupView === 'tree'" @click="groupView = 'tree'">Tree</button>
            </div>
            <small>{{ groupTree.length }} groups</small>
          </div>
          <ul v-if="groupView === 'tree' && groupTree.length" class="group-tree" role="tree" aria-label="File groups">
            <GroupTreeNode
              v-for="group in groupTree"
              :key="group.key"
              :group="group"
              :renames="sourceRenames"
              :expand-all="sourceFilterActive"
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
                  <th>Name</th>
                  <th>Group</th>
                  <th class="num">Size</th>
                  <th class="num">Modified</th>
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
                        :aria-label="`Include group ${row.groupKey}`"
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
          <p v-else-if="sourceEntries.length" class="filter-empty">No files match the current filter.</p>
        </template>
      </section>
      <section class="card"><div class="toolbar"><h3>Dry-run preview</h3><span></span><button :disabled="busy" @click="preview">{{ previewProgress ? 'Previewing...' : 'Preview' }}</button></div>
        <div v-if="previewProgress" class="preview-progress" role="status" aria-live="polite">
          <progress v-if="previewProgress.phase === 'planning'" :value="previewProgress.completedEntries" :max="previewProgress.totalEntries || 1"></progress>
          <progress v-else></progress>
          <p><span v-if="previewProgress.phase === 'scanning'">Scanning source · {{ previewProgress.completedEntries }} entries found</span><span v-else>Building plan · {{ previewProgress.completedEntries }} / {{ previewProgress.totalEntries ?? 0 }} entries</span><small v-if="previewProgress.relativePath">{{ previewProgress.relativePath }}</small></p>
        </div>
        <p v-if="plan">{{ plan.stats.filesToCopy }} files / {{ plan.stats.bytesToCopy }} bytes to copy; {{ plan.stats.filesToSkip }} skipped; {{ plan.stats.warnings }} warnings.</p>
        <div v-if="plan && plan.items.length" class="result-filters preview-filters" role="search">
          <input v-model="previewQuery" type="search" placeholder="Filter path or reason" aria-label="Filter preview by path or reason" />
          <select v-model="previewAction" aria-label="Filter preview by action">
            <option value="all">All actions</option>
            <option value="copy-file">Copy files</option>
            <option value="create-directory">Create folders</option>
            <option value="skip">Skipped</option>
            <option value="warn">Warnings</option>
          </select>
          <div class="view-switch" role="group" aria-label="Preview layout">
            <button type="button" :class="{ active: previewView === 'list' }" :aria-pressed="previewView === 'list'" @click="previewView = 'list'">List</button>
            <button type="button" :class="{ active: previewView === 'tree' }" :aria-pressed="previewView === 'tree'" @click="previewView = 'tree'">Tree</button>
          </div>
          <small>{{ filteredPlanItems.length }} of {{ plan.items.length }} items</small>
        </div>
        <div v-if="plan && filteredPlanItems.length && previewView === 'list'" class="plan"><div v-for="item in filteredPlanItems" :key="`${item.action}-${item.relativePath}`"><code>{{ item.action }}</code> <span :class="{ renamed: item.destinationPath && item.destinationPath !== item.relativePath }">{{ item.destinationPath ?? item.relativePath }}</span><small v-if="item.destinationPath && item.destinationPath !== item.relativePath">from {{ item.relativePath }}</small><small>{{ item.reason }}</small></div></div>
        <ul v-else-if="plan && filteredPlanItems.length" class="preview-tree" role="tree" aria-label="Preview plan tree">
          <PreviewTreeNode
            v-for="node in previewTree"
            :key="node.relativePath"
            :node="node"
            :expand-all="previewFilterActive"
          />
        </ul>
        <p v-else-if="plan && plan.items.length" class="filter-empty">No preview items match the current filter.</p>
        <p v-else-if="plan" class="filter-empty">No planned operations.</p>
      </section>
    </main>
    <main v-else>
      <div class="toolbar"><h2>Job history</h2><span></span><button @click="refresh">Refresh</button></div>
      <section v-if="progress" class="card"><div class="toolbar"><h3>Current job</h3><span></span><button @click="cancelJob">Cancel</button></div><progress :value="progress.completedFiles" :max="progress.totalFiles || 1"></progress><p>{{ progress.completedFiles }} / {{ progress.totalFiles }} files · {{ progress.relativePath }}</p></section>
      <article v-for="job in jobs" :key="job.jobId" class="card"><div class="toolbar"><strong :class="`status ${job.status}`">{{ job.status }}</strong><span></span><time>{{ new Date(job.finishedAt).toLocaleString() }}</time></div><p>{{ job.copiedFiles }} copied · {{ job.skippedFiles }} skipped · {{ job.failedFiles }} failed · {{ job.copiedBytes }} bytes</p><details @toggle="($event.target as HTMLDetailsElement).open && loadJobEvents(job.jobId)"><summary>Detailed log</summary><div class="plan"><div v-for="event in jobEvents[job.jobId] ?? []" :key="`${event.timestamp}-${event.relativePath}`"><code>{{ event.result }}</code> {{ event.relativePath }} <small>{{ event.message }}</small></div></div></details></article>
    </main>
  </div>
</template>
