<template>
  <div class="search-box-container">
    <div class="search-icon">
      <el-icon :size="20">
        <Search/>
      </el-icon>
    </div>
    <input
        ref="inputRef"
        :value="modelValue"
        autocomplete="off"
        class="search-input"
        placeholder="搜索应用、文件或输入命令..."
        spellcheck="false"
        type="text"
        @blur="$emit('blur', $event)"
        @focus="$emit('focus', $event)"
        @input="$emit('update:modelValue', $event.target.value); $emit('input', $event)"
        @keydown="$emit('keydown', $event)"
    />
    <div v-if="modelValue" class="clear-button" @click="handleClear">
      <el-icon>
        <Close/>
      </el-icon>
    </div>
  </div>
</template>

<script setup>
import {onMounted, ref, watch} from 'vue'
import {Close, Search} from '@element-plus/icons-vue'

const props = defineProps({
  modelValue: {
    type: String,
    default: ''
  }
})

const emit = defineEmits(['update:modelValue', 'input', 'keydown', 'focus', 'blur'])

const inputRef = ref(null)

const handleClear = () => {
  emit('update:modelValue', '')
  emit('input', {target: {value: ''}})
  inputRef.value?.focus()
}

onMounted(() => {
  inputRef.value?.focus()
})

watch(() => props.modelValue, (newVal) => {
  if (newVal === '') {
    inputRef.value?.focus()
  }
})
</script>

<style scoped>
.search-box-container {
  display: flex;
  align-items: center;
  padding: 12px 0 12px 16px;
  flex: 1;
}

.search-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  margin-right: 12px;
  color: var(--fy-text-muted);
}

.search-input {
  flex: 1;
  height: 32px;
  border: none;
  outline: none;
  background: transparent;
  font-size: 16px;
  color: var(--fy-text-primary);
  caret-color: var(--fy-accent);
}

.search-input::placeholder {
  color: var(--fy-text-muted);
}

.clear-button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  margin-right: 8px;
  border-radius: 4px;
  cursor: pointer;
  color: var(--fy-text-muted);
  transition: all 0.2s;
}

.clear-button:hover {
  background: var(--fy-bg-hover);
  color: var(--fy-text-primary);
}
</style>
