<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { PreviewTreeNode as PreviewTreeNodeModel } from "./preview-filter";

const props = defineProps<{
  node: PreviewTreeNodeModel;
  expandAll: boolean;
}>();

const expanded = ref(props.expandAll);
const hasChildren = computed(() => props.node.children.length > 0);

watch([() => props.expandAll, () => props.node], ([expandAll]) => {
  expanded.value = expandAll;
});
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
      <span class="preview-tree-name" :class="{ directory: node.directory }" :title="node.relativePath">{{ node.name }}</span>
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