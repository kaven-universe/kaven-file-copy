<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { PreviewTreeNode as PreviewTreeNodeModel } from "./preview-filter";

const props = defineProps<{
  node: PreviewTreeNodeModel;
  expandAll: boolean;
}>();

const expanded = ref(props.expandAll);
const hasChildren = computed(() => props.node.children.length > 0);
const renamed = computed(() => {
  const destination = props.node.item?.destinationPath;
  return destination !== undefined
    && destination !== null
    && destination !== props.node.item?.relativePath;
});

watch([() => props.expandAll, () => props.node], ([expandAll]) => {
  expanded.value = expandAll;
});

function displayedName() {
  if (!renamed.value || !props.node.item?.destinationPath) return props.node.name;
  const path = props.node.item.destinationPath.replaceAll("\\", "/");
  const index = path.lastIndexOf("/");
  return index === -1 ? path : path.slice(index + 1);
}
</script>

<template>
  <li class="preview-tree-item" role="treeitem" :aria-expanded="hasChildren ? expanded : undefined">
    <div class="preview-tree-row">
      <button
        v-if="hasChildren"
        class="tree-toggle"
        type="button"
        :aria-label="expanded ? `Collapse ${node.name}` : `Expand ${node.name}`"
        :title="expanded ? 'Collapse folder' : 'Expand folder'"
        @click="expanded = !expanded"
      >
        <span class="tree-chevron" :class="{ expanded }" aria-hidden="true"></span>
      </button>
      <span v-else class="tree-spacer" aria-hidden="true"></span>
      <span class="preview-tree-name" :class="{ directory: node.directory, renamed }" :title="node.relativePath">{{ displayedName() }}</span>
      <small v-if="renamed" class="preview-source">from {{ node.relativePath }}</small>
      <code v-if="node.item">{{ node.item.action }}</code>
      <small v-if="node.item">{{ node.item.reason }}</small>
    </div>
    <ul v-if="hasChildren && expanded" class="preview-tree-children" role="group">
      <PreviewTreeNode
        v-for="child in node.children"
        :key="child.relativePath"
        :node="child"
        :expand-all="expandAll"
      />
    </ul>
  </li>
</template>