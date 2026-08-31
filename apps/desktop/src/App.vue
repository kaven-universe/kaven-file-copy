<script setup lang="ts">
import { computed, onMounted, reactive, ref, toRaw } from "vue";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { api } from "./api/tauri";
import { buildPreviewTree, filterPlanItems, type PreviewActionFilter, type PreviewView } from "./preview-filter";
import PreviewTreeNode from "./PreviewTreeNode.vue";
import SourceTreeNode from "./SourceTreeNode.vue";
import {
  buildSourceTree,
  filterSourceEntries,
  hasPathDescendantOverride,
  isPathExcluded,
  normalizeTreePath,
  setPathExcluded,
  type SourceEntryTypeFilter,
  type SourceView,
} from "./source-tree";
import type { CopyPlan, CopyRule, JobEvent, JobSummary, PlanProgressEvent, ProgressEvent, RecentPaths, SourceEntry } from "./types/generated";

type View = "rules" | "editor" | "jobs";
const view = ref<View>("rules");
const rules = ref<CopyRule[]>([]);
const jobs = ref<JobSummary[]>([]);
const selectedRule = ref<CopyRule | null>(null);
const plan = ref<CopyPlan | null>(null);
const sourceEntries = ref<SourceEntry[]>([]);
const sourceQuery = ref("");
const sourceType = ref<SourceEntryTypeFilter>("all");
const sourceView = ref<SourceView>("tree");
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
  selectedRule.value?.filters.exclude.filter((item) => item.kind === "path").map((item) => normalizeTreePath(item.value)) ?? [],
));
const pathInclusions = computed(() => new Set(
  selectedRule.value?.filters.include.map(normalizeTreePath) ?? [],
));
const filteredSourceEntries = computed(() => filterSourceEntries(
  sourceEntries.value,
  sourceType.value,
  sourceQuery.value,
));
const sourceTree = computed(() => buildSourceTree(filteredSourceEntries.value));
const sourceFilterActive = computed(() => (
  sourceType.value !== "all" || sourceQuery.value.trim().length > 0
));
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
    filters: { exclude: [], include: [], includeHidden: true },
    copy: { mode: "incremental", compare: "size-and-mtime", overwrite: "if-changed", symlinks: "skip", preserveMtime: true, maxConcurrency: 4, retryCount: 2, versioned: { mappings: [], targetNameTemplate: "", versionPattern: "", renamePattern: "" } },
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
  copy.copy.versioned ??= { mappings: [] };
  copy.copy.versioned.targetNameTemplate ??= "";
  copy.copy.versioned.versionPattern ??= "";
  copy.copy.versioned.renamePattern ??= "";
  selectedRule.value = reactive(copy);
  sourceEntries.value = [];
  sourceQuery.value = "";
  sourceType.value = "all";
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
  sourceType.value = "all";
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

function toggleExcluded(path: string, checked: boolean) {
  if (!selectedRule.value) return;
  const globExclusions = selectedRule.value.filters.exclude.filter((item) => item.kind === "glob");
  const state = setPathExcluded(
    [...pathExclusions.value],
    selectedRule.value.filters.include,
    path,
    checked,
  );
  selectedRule.value.filters.exclude = [
    ...globExclusions,
    ...state.excluded.map((value) => ({ kind: "path" as const, value })),
  ];
  selectedRule.value.filters.include = state.included;
}

function sourceEntryExcluded(path: string) {
  return isPathExcluded(path, pathExclusions.value, pathInclusions.value);
}

function sourceEntryHasOverride(path: string) {
  return hasPathDescendantOverride(path, pathExclusions.value, pathInclusions.value);
}

function addGlob() {
  selectedRule.value?.filters.exclude.push({ kind: "glob", value: "**/*.tmp" });
}

function addVersionedMapping() {
  selectedRule.value?.copy.versioned.mappings.push({ sourcePattern: "", targetName: "", selection: "latest" });
}

async function detectVersionedMappings() {
  const rule = selectedRule.value;
  if (!rule) return;
  await act(async () => {
    const detected = await api.detectVersionedMappings(toRaw(rule));
    if (selectedRule.value) {
      selectedRule.value.copy.versioned.mappings = detected;
      message.value = `Detected ${detected.length} versioned file families from the source.`;
    }
  });
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
        <label>Mode<select v-model="selectedRule.copy.mode"><option value="incremental">Incremental mirror</option><option value="versioned">Versioned release</option></select></label>
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
        <p v-if="selectedRule.trigger.type !== 'manual'" class="hint wide">Automatic jobs run only while the desktop application is open.</p>
      </section>
      <section v-if="selectedRule.copy.mode === 'versioned'" class="card">
        <div class="toolbar"><h3>Versioned mappings</h3><span></span><button :disabled="busy || !selectedRule.source.path" @click="detectVersionedMappings">Detect mappings</button><button @click="addVersionedMapping">Add mapping</button></div>
        <p class="hint wide">The source folder can contain several releases of the same binary. Leave the mapping list empty to <strong>auto-detect</strong> every versioned file family (grouped by base name, newest version wins). Click <strong>Detect mappings</strong> to pre-fill the list for review. Target names support placeholders: <code>{name}</code> (base name without version), <code>{version}</code>, <code>{ext}</code>, and capture groups <code>{1}</code>, <code>{2}</code> from <code>renamePattern</code> — for example target <code>EDM.BIN0003{1}.dll</code> with rename pattern <code>^80([A-Za-z]+)_</code> turns <code>80R_12.1.8.72.bin</code> into <code>EDM.BIN0003R.dll</code>.</p>
        <div class="versioned-auto form-grid">
          <label>Target name template<input v-model="selectedRule.copy.versioned.targetNameTemplate" placeholder="{name}.{ext}" /></label>
          <label>Version pattern (optional)<input v-model="selectedRule.copy.versioned.versionPattern" placeholder="Leave empty for 1.2.0" /></label>
          <label>Rename pattern (optional)<input v-model="selectedRule.copy.versioned.renamePattern" placeholder="^80([A-Za-z]+)_" /></label>
        </div>
        <div v-for="(mapping, index) in selectedRule.copy.versioned.mappings" :key="index" class="versioned-mapping form-grid">
          <label>Source glob<input v-model="mapping.sourcePattern" placeholder="EDM.DSP.Core_*.dll" /></label>
          <label>Target name<input v-model="mapping.targetName" placeholder="EDM.DSP.Core.dll" /></label>
          <label>Selection<select v-model="mapping.selection"><option value="latest">Latest version</option><option value="exact">Exact version</option></select></label>
          <label v-if="mapping.selection === 'exact'">Version<input v-model="mapping.version" placeholder="1.2.0" /></label>
          <label>Version pattern (optional)<input v-model="mapping.versionPattern" placeholder="Inherit from rule" /></label>
          <div class="field-action"><span></span><button @click="selectedRule.copy.versioned.mappings.splice(index, 1)">Remove</button></div>
        </div>
      </section>
      <section class="card"><div class="toolbar"><h3>Exclusions</h3><span></span><button @click="addGlob">Add glob</button><button @click="browseSource">Browse source tree</button></div>
        <div v-for="(item,index) in selectedRule.filters.exclude.filter(i => i.kind === 'glob')" :key="index" class="field-action"><input v-model="item.value" /><button @click="selectedRule.filters.exclude.splice(selectedRule.filters.exclude.indexOf(item),1)">Remove</button></div>
        <div v-if="sourceEntries.length" class="result-filters source-filters" role="search">
          <input v-model="sourceQuery" type="search" placeholder="Filter source path" aria-label="Filter source entries by path" />
          <select v-model="sourceType" aria-label="Filter source entries by type">
            <option value="all">All entries</option>
            <option value="folders">Folders</option>
            <option value="files">Files</option>
          </select>
          <div class="view-switch" role="group" aria-label="Source layout">
            <button type="button" :class="{ active: sourceView === 'list' }" :aria-pressed="sourceView === 'list'" @click="sourceView = 'list'">List</button>
            <button type="button" :class="{ active: sourceView === 'tree' }" :aria-pressed="sourceView === 'tree'" @click="sourceView = 'tree'">Tree</button>
          </div>
          <small>{{ filteredSourceEntries.length }} of {{ sourceEntries.length }} entries</small>
        </div>
        <ul v-if="filteredSourceEntries.length && sourceView === 'tree'" class="source-tree" role="tree" aria-label="Source exclusions">
          <SourceTreeNode
            v-for="node in sourceTree"
            :key="node.relativePath"
            :node="node"
            :excluded-paths="pathExclusions"
            :included-paths="pathInclusions"
            :expand-all="sourceFilterActive"
            @toggle-excluded="toggleExcluded"
          />
        </ul>
        <ul v-else-if="filteredSourceEntries.length" class="source-list" aria-label="Source exclusions list">
          <li v-for="entry in filteredSourceEntries" :key="entry.relativePath">
            <label class="source-list-row">
              <input
                type="checkbox"
                :checked="sourceEntryExcluded(entry.relativePath)"
                :indeterminate="sourceEntryHasOverride(entry.relativePath)"
                :aria-label="`Exclude ${entry.relativePath}`"
                @change="toggleExcluded(entry.relativePath, ($event.target as HTMLInputElement).checked)"
              />
              <span :title="entry.relativePath">{{ entry.relativePath }}</span>
              <small>{{ entry.directory ? 'Folder' : 'File' }}</small>
            </label>
          </li>
        </ul>
        <p v-else-if="sourceEntries.length" class="filter-empty">No source entries match the current filter.</p>
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
        <div v-if="plan && filteredPlanItems.length && previewView === 'list'" class="plan"><div v-for="item in filteredPlanItems" :key="`${item.action}-${item.relativePath}`"><code>{{ item.action }}</code> {{ item.relativePath }} <small v-if="item.sourceRelativePath && item.sourceRelativePath !== item.relativePath">from {{ item.sourceRelativePath }} · </small><small>{{ item.reason }}</small></div></div>
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
