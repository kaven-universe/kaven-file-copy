<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { t } from "./i18n";
import { formatBytes, formatModified, type SourceTreeNode } from "./source-tree";

const props = withDefaults(defineProps<{
  node: SourceTreeNode;
  includedPaths: ReadonlySet<string>;
  excludedPaths: ReadonlySet<string>;
  renames?: ReadonlyMap<string, string>;
  expandAll?: boolean;
  depth?: number;
  ancestorIncluded?: boolean;
}>(), {
  depth: 0,
  ancestorIncluded: false,
  expandAll: false,
  renames: () => new Map<string, string>(),
});

const emit = defineEmits<{
  toggleIncluded: [path: string, checked: boolean];
}>();

const expanded = ref(props.expandAll);
const hasChildren = computed(() => props.node.directory && props.node.children.length > 0);
const directlyIncluded = computed(() => props.includedPaths.has(props.node.relativePath));
const directlyExcluded = computed(() => props.excludedPaths.has(props.node.relativePath));
const effectivelyIncluded = computed(() => {
  if (directlyExcluded.value) return false;
  if (directlyIncluded.value) return true;
  return props.ancestorIncluded;
});
const hasDescendantOverride = computed(() => {
  const prefix = `${props.node.relativePath}/`;
  const overrides = effectivelyIncluded.value ? props.excludedPaths : props.includedPaths;
  return [...overrides].some((path) => path.startsWith(prefix));
});

watch([() => props.expandAll, () => props.node], ([expandAll]) => {
  expanded.value = expandAll;
});

function toggleExpansion() {
  expanded.value = !expanded.value;
}

function handleInclusion(event: Event) {
  emit("toggleIncluded", props.node.relativePath, (event.target as HTMLInputElement).checked);
}

function forwardInclusion(path: string, checked: boolean) {
  emit("toggleIncluded", path, checked);
}
</script>

<template>
  <li
    class="source-tree-item"
    role="treeitem"
    :aria-expanded="hasChildren ? expanded : undefined"
  >
    <div class="source-tree-row" :class="{ inherited: ancestorIncluded && !directlyIncluded && !directlyExcluded }">
      <button
        v-if="hasChildren"
        class="tree-toggle"
        type="button"
        :aria-label="expanded ? t('tree.collapse', { name: node.name }) : t('tree.expand', { name: node.name })"
        :title="expanded ? t('tree.collapseFolder') : t('tree.expandFolder')"
        @click="toggleExpansion"
      >
        <span class="tree-chevron" :class="{ expanded }" aria-hidden="true"></span>
      </button>
      <span v-else class="tree-spacer" aria-hidden="true"></span>
      <input
        class="tree-checkbox"
        type="checkbox"
        :checked="effectivelyIncluded"
        :indeterminate="hasDescendantOverride"
        :aria-label="t('tree.include', { path: node.relativePath })"
        @change="handleInclusion"
      />
      <span class="source-tree-name" :class="{ directory: node.directory }" :title="node.relativePath">
        {{ node.name }}
      </span>
      <small v-if="!node.directory && renames.has(node.relativePath)" class="source-tree-rename">→ {{ renames.get(node.relativePath) }}</small>
      <small class="source-tree-meta">
        <template v-if="!node.directory">{{ formatBytes(node.size) }} · </template>{{ formatModified(node.modified) }}
      </small>
    </div>
    <ul v-if="hasChildren && expanded" class="source-tree-children" role="group">
      <SourceTreeNode
        v-for="child in node.children"
        :key="child.relativePath"
        :node="child"
        :included-paths="includedPaths"
        :excluded-paths="excludedPaths"
        :renames="renames"
        :expand-all="expandAll"
        :depth="depth + 1"
        :ancestor-included="effectivelyIncluded"
        @toggle-included="forwardInclusion"
      />
    </ul>
  </li>
</template>
