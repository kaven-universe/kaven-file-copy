<script setup lang="ts">
import { computed, ref } from "vue";
import type { SourceTreeNode } from "./source-tree";

const props = withDefaults(defineProps<{
  node: SourceTreeNode;
  excludedPaths: ReadonlySet<string>;
  includedPaths: ReadonlySet<string>;
  depth?: number;
  ancestorExcluded?: boolean;
}>(), {
  depth: 0,
  ancestorExcluded: false,
});

const emit = defineEmits<{
  toggleExcluded: [path: string, checked: boolean];
}>();

const expanded = ref(false);
const hasChildren = computed(() => props.node.directory && props.node.children.length > 0);
const directlyExcluded = computed(() => props.excludedPaths.has(props.node.relativePath));
const directlyIncluded = computed(() => props.includedPaths.has(props.node.relativePath));
const effectivelyExcluded = computed(() => {
  if (directlyIncluded.value) return false;
  if (directlyExcluded.value) return true;
  return props.ancestorExcluded;
});
const hasDescendantOverride = computed(() => {
  const prefix = `${props.node.relativePath}/`;
  const overrides = effectivelyExcluded.value ? props.includedPaths : props.excludedPaths;
  return [...overrides].some((path) => path.startsWith(prefix));
});

function toggleExpansion() {
  expanded.value = !expanded.value;
}

function handleExclusion(event: Event) {
  emit("toggleExcluded", props.node.relativePath, (event.target as HTMLInputElement).checked);
}

function forwardExclusion(path: string, checked: boolean) {
  emit("toggleExcluded", path, checked);
}
</script>

<template>
  <li
    class="source-tree-item"
    role="treeitem"
    :aria-expanded="hasChildren ? expanded : undefined"
  >
    <div class="source-tree-row" :class="{ inherited: ancestorExcluded && !directlyIncluded && !directlyExcluded }">
      <button
        v-if="hasChildren"
        class="tree-toggle"
        type="button"
        :aria-label="expanded ? `Collapse ${node.name}` : `Expand ${node.name}`"
        :title="expanded ? 'Collapse folder' : 'Expand folder'"
        @click="toggleExpansion"
      >
        <span class="tree-chevron" :class="{ expanded }" aria-hidden="true"></span>
      </button>
      <span v-else class="tree-spacer" aria-hidden="true"></span>
      <input
        class="tree-checkbox"
        type="checkbox"
        :checked="effectivelyExcluded"
        :indeterminate="hasDescendantOverride"
        :aria-label="`Exclude ${node.relativePath}`"
        @change="handleExclusion"
      />
      <span class="source-tree-name" :class="{ directory: node.directory }" :title="node.relativePath">
        {{ node.name }}
      </span>
    </div>
    <ul v-if="hasChildren && expanded" class="source-tree-children" role="group">
      <SourceTreeNode
        v-for="child in node.children"
        :key="child.relativePath"
        :node="child"
        :excluded-paths="excludedPaths"
        :included-paths="includedPaths"
        :depth="depth + 1"
        :ancestor-excluded="effectivelyExcluded"
        @toggle-excluded="forwardExclusion"
      />
    </ul>
  </li>
</template>