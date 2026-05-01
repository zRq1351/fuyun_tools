<template>
  <div
      :class="{ 'is-active': isActive }"
      class="result-item"
      @click="$emit('click', item)"
      @mouseenter="$emit('mouseenter')"
  >
    <div class="item-icon">
      <el-icon :size="20">
        <component :is="iconComponent"/>
      </el-icon>
    </div>
    <div class="item-content">
      <div class="item-title">{{ item.title }}</div>
      <div v-if="item.description" class="item-description">{{ item.description }}</div>
    </div>
    <div v-if="item.shortcut" class="item-shortcut">
      <span class="shortcut-key">{{ item.shortcut }}</span>
    </div>
    <div class="item-type">
      <span class="type-badge">{{ item.type }}</span>
    </div>
  </div>
</template>

<script setup>
import {computed} from 'vue'
import {
  CopyDocument,
  DataLine,
  Document,
  Files,
  Folder,
  Monitor,
  Operation,
  Search,
  Setting
} from '@element-plus/icons-vue'

const props = defineProps({
  item: {
    type: Object,
    required: true
  },
  isActive: {
    type: Boolean,
    default: false
  }
})

defineEmits(['click', 'mouseenter'])

const iconMap = {
  app: Monitor,
  file: Document,
  clipboard: CopyDocument,
  setting: Setting,
  command: Operation,
  calculator: DataLine,
  folder: Folder,
  search: Search,
  default: Files
}

const iconComponent = computed(() => {
  return iconMap[props.item.icon] || iconMap[props.item.type] || iconMap.default
})
</script>

<style scoped>
.result-item {
  display: flex;
  align-items: center;
  padding: 10px 16px;
  cursor: pointer;
  transition: background-color 0.15s;
}

.result-item:hover,
.result-item.is-active {
  background: var(--fy-bg-hover);
}

.item-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  margin-right: 12px;
  border-radius: 8px;
  background: var(--fy-bg-card);
  color: var(--fy-accent);
}

.item-content {
  flex: 1;
  min-width: 0;
}

.item-title {
  font-size: 14px;
  color: var(--fy-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.item-description {
  font-size: 12px;
  color: var(--fy-text-muted);
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.item-shortcut {
  margin-left: 12px;
}

.shortcut-key {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 24px;
  height: 20px;
  padding: 0 6px;
  background: var(--fy-bg-hover);
  border: 1px solid var(--fy-border-light);
  border-radius: 4px;
  font-size: 11px;
  font-family: monospace;
  color: var(--fy-text-secondary);
}

.item-type {
  margin-left: 8px;
}

.type-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 20px;
  padding: 0 8px;
  border-radius: 10px;
  font-size: 11px;
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
}
</style>
