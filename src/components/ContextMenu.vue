<template>
  <Teleport to="body">
    <div
        v-if="show"
        ref="menuRef"
        :style="menuStyle"
        class="context-menu"
        @click.stop
    >
      <slot/>
    </div>
  </Teleport>
</template>

<script setup>
import {ref, watch, computed, onBeforeUnmount, nextTick} from 'vue'

const props = defineProps({
  show: Boolean,
  x: {type: Number, default: 0},
  y: {type: Number, default: 0},
  zIndex: {type: Number, default: 10000},
})

const emit = defineEmits(['close'])

const menuRef = ref(null)
const adjustedX = ref(0)
const adjustedY = ref(0)

const menuStyle = computed(() => ({
  left: adjustedX.value + 'px',
  top: adjustedY.value + 'px',
  zIndex: props.zIndex,
}))

watch(() => props.show, async (visible) => {
  if (visible) {
    adjustedX.value = props.x
    adjustedY.value = props.y
    await nextTick()
    adjustPosition()
    setTimeout(() => {
      document.addEventListener('click', onDocClick)
      document.addEventListener('keydown', onKeydown)
    }, 0)
  } else {
    document.removeEventListener('click', onDocClick)
    document.removeEventListener('keydown', onKeydown)
  }
})

function adjustPosition() {
  const el = menuRef.value
  if (!el) return
  const w = el.offsetWidth
  const h = el.offsetHeight
  const vw = window.innerWidth
  const vh = window.innerHeight
  let x = props.x
  let y = props.y
  if (x + w > vw - 8) x = Math.max(8, vw - w - 8)
  if (y + h > vh - 8) y = Math.max(8, vh - h - 8)
  adjustedX.value = x
  adjustedY.value = y
}

function onDocClick(e) {
  if (menuRef.value && !menuRef.value.contains(e.target)) {
    emit('close')
  }
}

function onKeydown(e) {
  if (e.key === 'Escape') {
    emit('close')
  }
}

onBeforeUnmount(() => {
  document.removeEventListener('click', onDocClick)
  document.removeEventListener('keydown', onKeydown)
})
</script>

<style>
@import "../pages/shared/contextMenu.css";
</style>
