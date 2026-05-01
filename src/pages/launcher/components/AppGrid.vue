<template>
  <div class="app-grid-container">
    <div v-for="category in categories" :key="category.name" class="app-category">
      <div class="category-name">{{ category.name }}</div>
      <div class="app-grid">
        <div
            v-for="app in category.apps"
            :key="app.id"
            class="app-item"
            @click="$emit('select', app)"
        >
          <div class="app-icon">
            <img v-if="app.icon_base64" :src="app.icon_base64" class="icon-img"/>
            <el-icon v-else :size="24">
              <Monitor/>
            </el-icon>
          </div>
          <div class="app-name">{{ app.title }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import {Monitor} from '@element-plus/icons-vue'

defineProps({
  categories: {
    type: Array,
    required: true
  }
})

defineEmits(['select'])
</script>

<style scoped>
.app-grid-container {
  flex: 1;
  overflow-y: auto;
  padding: 8px 12px;
}

.app-category {
  margin-bottom: 12px;
}

.category-name {
  font-size: 12px;
  font-weight: 600;
  color: var(--fy-text-muted);
  padding: 4px 4px 8px;
  user-select: none;
}

.app-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(76px, 1fr));
  gap: 4px;
}

.app-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 10px 4px;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.15s;
}

.app-item:hover {
  background: var(--fy-bg-hover);
}

.app-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  border-radius: 10px;
  overflow: hidden;
}

.icon-img {
  width: 40px;
  height: 40px;
  object-fit: contain;
}

.app-name {
  font-size: 11px;
  color: var(--fy-text-primary);
  text-align: center;
  width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  line-height: 1.3;
}
</style>
