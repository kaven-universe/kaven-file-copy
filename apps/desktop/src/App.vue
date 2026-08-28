<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { api } from "./api/tauri";
import type { CopyPlan, CopyRule, JobEvent, JobSummary, ProgressEvent, SourceEntry } from "./types/generated";

type View = "rules" | "editor" | "jobs";
const view = ref<View>("rules");
const rules = ref<CopyRule[]>([]);
const jobs = ref<JobSummary[]>([]);
const selectedRule = ref<CopyRule | null>(null);
const plan = ref<CopyPlan | null>(null);
const sourceEntries = ref<SourceEntry[]>([]);
const busy = ref(false);
const message = ref("");
const progress = ref<ProgressEvent | null>(null);
const jobEvents = ref<Record<string, JobEvent[]>>({});
const pathExclusions = computed(() => new Set(
  selectedRule.value?.filters.exclude.filter((item) => item.kind === "path").map((item) => item.value) ?? [],
));

function newRule(): CopyRule {
  return reactive({
    schemaVersion: 1, id: crypto.randomUUID(), name: "New copy rule", enabled: true,
    source: { type: "local", path: "" }, destination: { type: "local", path: "" },
    filters: { exclude: [], includeHidden: true },
    copy: { mode: "incremental", compare: "size-and-mtime", overwrite: "if-changed", symlinks: "skip", preserveMtime: true, maxConcurrency: 4, retryCount: 2 },
    trigger: { type: "manual" },
  });
}

async function refresh() {
  [rules.value, jobs.value] = await Promise.all([api.listRules(), api.listJobs()]);
}

function edit(rule?: CopyRule) {
  selectedRule.value = reactive(structuredClone(rule ?? newRule()));
  sourceEntries.value = [];
  plan.value = null;
  view.value = "editor";
}

async function choosePath(kind: "source" | "destination", directory: boolean) {
  const value = await open({ directory, multiple: false });
  if (value && selectedRule.value) selectedRule.value[kind].path = value;
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
  selectedRule.value.filters.exclude = selectedRule.value.filters.exclude.filter((item) => !(item.kind === "path" && item.value === path));
  if (checked) selectedRule.value.filters.exclude.push({ kind: "path", value: path });
}

function addGlob() {
  selectedRule.value?.filters.exclude.push({ kind: "glob", value: "**/*.tmp" });
}

async function persist() {
  if (!selectedRule.value) return;
  await act(async () => { await api.validateRule(selectedRule.value!); await api.saveRule(selectedRule.value!); await refresh(); view.value = "rules"; });
}

async function preview() {
  if (!selectedRule.value) return;
  await act(async () => { plan.value = await api.previewRule(selectedRule.value!); });
}

async function run(rule: CopyRule) {
  progress.value = null;
  await act(async () => { const summary = await api.executeRule(rule); jobs.value.unshift(summary); progress.value = null; view.value = "jobs"; });
}

function duplicateRule(rule: CopyRule) {
  const copy = structuredClone(rule);
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
  await act(async () => { const rule = JSON.parse(await readTextFile(path)) as CopyRule; await api.validateRule(rule); edit(rule); });
}

async function act(action: () => Promise<void>) {
  busy.value = true; message.value = "";
  try { await action(); } catch (error) { message.value = String(error); }
  finally { busy.value = false; }
}

onMounted(async () => {
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
        <div class="actions"><button @click="exportRule(rule)">Export</button><button @click="duplicateRule(rule)">Duplicate</button><button @click="edit(rule)">Edit</button><button @click="removeRule(rule)">Delete</button><button class="primary" :disabled="busy" @click="run(rule)">Run</button></div>
      </article>
    </main>
    <main v-else-if="view === 'editor' && selectedRule">
      <div class="toolbar"><h2>Edit rule</h2><span></span><button @click="view='rules'">Cancel</button><button class="primary" :disabled="busy" @click="persist">Save</button></div>
      <section class="card form-grid">
        <label class="wide">Rule name<input v-model="selectedRule.name" /></label>
        <label class="wide">Source<div class="field-action"><input v-model="selectedRule.source.path" /><button @click="choosePath('source', false)">Choose file</button><button @click="choosePath('source', true)">Choose folder</button></div></label>
        <label class="wide">Destination<div class="field-action"><input v-model="selectedRule.destination.path" /><button @click="choosePath('destination', true)">Browse</button></div></label>
        <label>Compare<select v-model="selectedRule.copy.compare"><option value="size-and-mtime">Size and modified time</option><option value="size-only">Size only</option></select></label>
        <label>Overwrite<select v-model="selectedRule.copy.overwrite"><option value="if-changed">If changed</option><option value="never">Never</option><option value="always">Always</option></select></label>
        <label>Trigger<select :value="selectedRule.trigger.type" @change="changeTrigger"><option value="manual">Manual</option><option value="watch">Watch changes</option><option value="interval">Fixed interval</option><option value="cron">Cron</option></select></label>
        <label>Retries<input v-model.number="selectedRule.copy.retryCount" type="number" min="0" max="10" /></label>
        <label v-if="selectedRule.trigger.type === 'watch'">Debounce (ms)<input v-model.number="selectedRule.trigger.debounceMs" type="number" min="100" /></label>
        <label v-if="selectedRule.trigger.type === 'interval'">Interval (seconds)<input v-model.number="selectedRule.trigger.seconds" type="number" min="1" /></label>
        <label v-if="selectedRule.trigger.type === 'cron'" class="wide">Cron expression<input v-model="selectedRule.trigger.expression" /></label>
      </section>
      <section class="card"><div class="toolbar"><h3>Exclusions</h3><span></span><button @click="addGlob">Add glob</button><button @click="browseSource">Browse source tree</button></div>
        <div v-for="(item,index) in selectedRule.filters.exclude.filter(i => i.kind === 'glob')" :key="index" class="field-action"><input v-model="item.value" /><button @click="selectedRule.filters.exclude.splice(selectedRule.filters.exclude.indexOf(item),1)">Remove</button></div>
        <div class="source-tree"><label v-for="entry in sourceEntries" :key="entry.relativePath"><input type="checkbox" :checked="pathExclusions.has(entry.relativePath)" @change="toggleExcluded(entry.relativePath, ($event.target as HTMLInputElement).checked)" /><span>{{ entry.directory ? '▸' : '·' }} {{ entry.relativePath }}</span></label></div>
      </section>
      <section class="card"><div class="toolbar"><h3>Dry-run preview</h3><span></span><button :disabled="busy" @click="preview">Preview</button></div>
        <p v-if="plan">{{ plan.stats.filesToCopy }} files / {{ plan.stats.bytesToCopy }} bytes to copy; {{ plan.stats.filesToSkip }} skipped; {{ plan.stats.warnings }} warnings.</p>
        <div v-if="plan" class="plan"><div v-for="item in plan.items" :key="item.relativePath"><code>{{ item.action }}</code> {{ item.relativePath }} <small>{{ item.reason }}</small></div></div>
      </section>
    </main>
    <main v-else>
      <div class="toolbar"><h2>Job history</h2><span></span><button @click="refresh">Refresh</button></div>
      <section v-if="progress" class="card"><div class="toolbar"><h3>Current job</h3><span></span><button @click="cancelJob">Cancel</button></div><progress :value="progress.completedFiles" :max="progress.totalFiles || 1"></progress><p>{{ progress.completedFiles }} / {{ progress.totalFiles }} files · {{ progress.relativePath }}</p></section>
      <article v-for="job in jobs" :key="job.jobId" class="card"><div class="toolbar"><strong :class="`status ${job.status}`">{{ job.status }}</strong><span></span><time>{{ new Date(job.finishedAt).toLocaleString() }}</time></div><p>{{ job.copiedFiles }} copied · {{ job.skippedFiles }} skipped · {{ job.failedFiles }} failed · {{ job.copiedBytes }} bytes</p><details @toggle="($event.target as HTMLDetailsElement).open && loadJobEvents(job.jobId)"><summary>Detailed log</summary><div class="plan"><div v-for="event in jobEvents[job.jobId] ?? []" :key="`${event.timestamp}-${event.relativePath}`"><code>{{ event.result }}</code> {{ event.relativePath }} <small>{{ event.message }}</small></div></div></details></article>
    </main>
  </div>
</template>
