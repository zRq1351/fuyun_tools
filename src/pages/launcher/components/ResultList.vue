<template>
  <div class="result-list-container">
    <el-scrollbar ref="scrollbarRef" class="result-scrollbar">
      <div class="result-list">
        <ResultItem
            v-for="(item, index) in results"
            :key="item.id || index"
            :ref="el => setItemRef(el, index)"
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
import {nextTick, ref, watch} from 'vue'
import ResultItem from './ResultItem.vue'

const props = defineProps({
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

const scrollbarRef = ref(null)
const itemRefs = ref({})

const setItemRef = (el, index) => {
  if (el) {
    itemRefs.value[index] = el
  }
}

watch(() => props.activeIndex, async (newIndex) => {
  await nextTick()
  const activeEl = itemRefs.value[newIndex]
  if (activeEl && activeEl.$el) {
    activeEl.$el.scrollIntoView({block: 'nearest', behavior: 'smooth'})
  }
})
</script>

<style scoped>
.result-list-container {
  border-top: 1px solid var(--fy-border-light);
  height: 100%;
  overflow: hidden;
}

.result-scrollbar {
  height: 100%;
}

.result-scrollbar :deep(.el-scrollbar__wrap) {
  display: flex;
  flex-direction: column;
}

.result-list {
  padding: 4px 0;
}
</style>
