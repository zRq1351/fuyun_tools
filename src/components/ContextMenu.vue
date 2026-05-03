<template>
  <Teleport to="body">
    <div
        v-if="show"
        ref="menuRef"
        :style="menuStyle"
        class="context-menu"
        @click.stop
        @mouseover="activateItemOnHover"
    >
      <slot/>
    </div>
  </Teleport>
</template>

<script setup>
import {computed, nextTick, onBeforeUnmount, ref, watch} from 'vue'

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
const activeIndex = ref(-1)
let hasListeners = false

const menuStyle = computed(() => ({
  left: adjustedX.value + 'px',
  top: adjustedY.value + 'px',
  zIndex: props.zIndex,
}))

watch(() => props.show, async (visible) => {
  if (visible) {
    activeIndex.value = -1
    adjustedX.value = props.x
    adjustedY.value = props.y
    await nextTick()
    adjustPosition()
    if (!hasListeners) {
      hasListeners = true
      setTimeout(() => {
        document.addEventListener('mousedown', onDocMouseDown)
        document.addEventListener('keydown', onKeydown)
      }, 0)
    }
  } else {
    document.removeEventListener('mousedown', onDocMouseDown)
    document.removeEventListener('keydown', onKeydown)
    activeIndex.value = -1
    hasListeners = false
  }
})

watch([() => props.x, () => props.y], async () => {
  if (props.show) {
    activeIndex.value = -1
    adjustedX.value = props.x
    adjustedY.value = props.y
    await nextTick()
    adjustPosition()
  }
})

function getItems() {
  return menuRef.value?.querySelectorAll(
      '.context-menu-item:not([disabled]):not(.context-menu-item-disabled)'
  ) || []
}

function highlightItem(idx) {
  const items = getItems()
  items.forEach(i => i.classList.remove('context-menu-item-active'))
  if (idx >= 0 && idx < items.length) {
    items[idx].classList.add('context-menu-item-active')
    items[idx].scrollIntoView({block: 'nearest'})
  }
}

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

function onDocMouseDown(e) {
  if (e.button !== 0) return
  if (menuRef.value && !menuRef.value.contains(e.target)) {
    // don't close if clicking inside another context-menu (e.g. submenu)
    if (e.target.closest('.context-menu')) return
    emit('close')
  }
}

function onKeydown(e) {
  const items = getItems()
  if (!items.length) return

  if (e.key === 'ArrowDown') {
    e.preventDefault()
    activeIndex.value = Math.min(activeIndex.value + 1, items.length - 1)
    highlightItem(activeIndex.value)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    activeIndex.value = Math.max(activeIndex.value - 1, 0)
    highlightItem(activeIndex.value)
  } else if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault()
    if (activeIndex.value >= 0 && activeIndex.value < items.length) {
      const item = items[activeIndex.value]
      if (item) {
        const sub = item.querySelector('.context-submenu')
        if (sub) {
          sub.dispatchEvent(new MouseEvent('mouseenter', {bubbles: false}))
        } else {
          item.click()
        }
      }
    }
  } else if (e.key === 'ArrowRight') {
    e.preventDefault()
    if (activeIndex.value >= 0) {
      const item = items[activeIndex.value]
      const sub = item?.querySelector('.context-submenu')
      if (sub) {
        sub.dispatchEvent(new MouseEvent('mouseenter', {bubbles: false}))
      }
    }
  } else if (e.key === 'ArrowLeft') {
    e.preventDefault()
    emit('close')
  } else if (e.key === 'Escape') {
    e.preventDefault()
    emit('close')
  }
}

function activateItemOnHover(e) {
  const items = getItems()
  const target = e.target.closest('.context-menu-item')
  if (!target) return
  const idx = Array.from(items).indexOf(target)
  if (idx >= 0) {
    activeIndex.value = idx
    highlightItem(idx)
  }
}

onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocMouseDown)
  document.removeEventListener('keydown', onKeydown)
})

defineExpose({menuRef})
</script>

<style>
@import "../pages/shared/contextMenu.css";
</style>
