<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { formatBytes, formatModified, type GroupTreeNode } from "./source-tree";

const props = withDefaults(defineProps<{
  group: GroupTreeNode;
  expandAll?: boolean;
}>(), {
  expandAll: false,
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
  emit("toggleGroup", props.group.name, (event.target as HTMLInputElement).checked);
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
        :aria-label="expanded ? `Collapse ${group.name}` : `Expand ${group.name}`"
        :title="expanded ? 'Collapse group' : 'Expand group'"
        @click="toggleExpansion"
      >
        <span class="tree-chevron" :class="{ expanded }" aria-hidden="true"></span>
      </button>
      <span v-else class="tree-spacer" aria-hidden="true"></span>
      <input
        v-if="group.pattern"
        class="tree-checkbox"
        type="checkbox"
        :checked="group.selected"
        :aria-label="`Copy group ${group.name}`"
        @change="handleToggle"
      />
      <span v-else class="tree-spacer" aria-hidden="true"></span>
      <span class="group-tree-name" :title="group.pattern || undefined">{{ group.name }}</span>
      <small class="group-tree-meta">{{ group.matched.length }} files</small>
    </div>
    <ul v-if="hasFiles && expanded" class="group-tree-children" role="group">
      <li v-for="file in group.matched" :key="file.relativePath" class="group-file-row">
        <span class="tree-spacer" aria-hidden="true"></span>
        <span class="group-file-name" :title="file.relativePath">{{ file.relativePath }}</span>
        <small class="group-file-meta">{{ formatBytes(file.size) }} · {{ formatModified(file.modified) }}</small>
      </li>
    </ul>
  </li>
</template>
