<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { formatBytes, formatModified, type GroupTreeNode } from "./source-tree";

const props = withDefaults(defineProps<{
  group: GroupTreeNode;
  renames?: ReadonlyMap<string, string>;
  expandAll?: boolean;
}>(), {
  expandAll: false,
  renames: () => new Map<string, string>(),
});

const emit = defineEmits<{
  toggleGroup: [name: string, checked: boolean];
}>();

const expanded = ref(props.expandAll);
const hasFiles = computed(() => props.group.matched.length > 0);

watch(() => props.expandAll, (value) => {
  expanded.value = value;
});

function toggleExpansion() {
  expanded.value = !expanded.value;
}

function handleToggle(event: Event) {
  emit("toggleGroup", props.group.key, (event.target as HTMLInputElement).checked);
}

function displayName() {
  return props.group.key
    ? (props.group.label ? `${props.group.label} ${props.group.key}` : props.group.key)
    : props.group.label;
}
</script>

<template>
  <li
    class="group-tree-item"
    role="treeitem"
    :aria-expanded="hasFiles ? expanded : undefined"
  >
    <div class="group-tree-row">
      <button
        v-if="hasFiles"
        class="tree-toggle"
        type="button"
        :aria-label="expanded ? `Collapse ${group.key || group.label}` : `Expand ${group.key || group.label}`"
        :title="expanded ? 'Collapse group' : 'Expand group'"
        @click="toggleExpansion"
      >
        <span class="tree-chevron" :class="{ expanded }" aria-hidden="true"></span>
      </button>
      <span v-else class="tree-spacer" aria-hidden="true"></span>
      <input
        v-if="group.key"
        class="tree-checkbox"
        type="checkbox"
        :checked="group.selected"
        :aria-label="`Copy ${displayName()}`"
        @change="handleToggle"
      />
      <span v-else class="tree-spacer" aria-hidden="true"></span>
      <span class="group-tree-name" :title="group.key || undefined">{{ displayName() }}</span>
      <small class="group-tree-meta">{{ group.matched.length }} files</small>
    </div>
    <ul v-if="hasFiles && expanded" class="group-tree-children" role="group">
      <li v-for="file in group.matched" :key="file.relativePath" class="group-file-row">
        <span class="tree-spacer" aria-hidden="true"></span>
        <span class="group-file-name" :title="file.relativePath">{{ file.relativePath }}</span>
        <small v-if="renames.has(file.relativePath)" class="source-list-rename">→ {{ renames.get(file.relativePath) }}</small>
        <small class="group-file-meta">{{ formatBytes(file.size) }} · {{ formatModified(file.modified) }}</small>
      </li>
    </ul>
  </li>
</template>
