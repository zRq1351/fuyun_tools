<template>
  <div class="result-list-container">
    <el-scrollbar max-height="260px">
      <div class="result-list">
        <ResultItem
            v-for="(item, index) in results"
            :key="item.id || index"
            :is-active="index === activeIndex"
            :item="item"
            @click="$emit('select', item)"
            @mouseenter="$emit('hover', index)"
        />
      </div>
    </el-scrollbar>
  </div>
</template>

<script setup>
import ResultItem from './ResultItem.vue'

defineProps({
  results: {
    type: Array,
    required: true
  },
  activeIndex: {
    type: Number,
    default: 0
  }
})

defineEmits(['select', 'hover'])
</script>

<style scoped>
.result-list-container {
  border-top: 1px solid var(--fy-border-light);
}

.result-list {
  padding: 4px 0;
}
</style>
