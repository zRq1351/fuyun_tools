<template>
  <div
      ref="itemRef"
      class="context-menu-item context-menu-item-sub"
      @mouseenter="openSub"
      @mouseleave="onLeave"
  >
    <span class="context-menu-item-label">{{ label }}</span>
    <span class="context-menu-item-arrow">▶</span>
    <ContextMenu
        ref="subMenuRef"
        :show="subOpen"
        :x="subX"
        :y="subY"
        :z-index="zIndex + 1"
        @close="subOpen = false"
    >
      <slot/>
    </ContextMenu>
  </div>
</template>

<script setup>
import {onBeforeUnmount, ref, watch} from 'vue'
import ContextMenu from './ContextMenu.vue'

defineProps({
  label: String,
  zIndex: {type: Number, default: 10000},
})

const itemRef = ref(null)
const subMenuRef = ref(null)
const subOpen = ref(false)
const subX = ref(0)
const subY = ref(0)
let closeTimer = null

async function openSub() {
  clearTimeout(closeTimer)
  if (!itemRef.value) return
  const rect = itemRef.value.getBoundingClientRect()
  subX.value = rect.right + 4
  subY.value = rect.top - 4
  subOpen.value = true
}

function onLeave() {
  tryClose()
}

function tryClose() {
  closeTimer = setTimeout(() => {
    subOpen.value = false
  }, 150)
}

function onDocMouseOver(e) {
  const subEl = subMenuRef.value?.menuRef
  const inSub = subEl && subEl.contains(e.target)
  const inParent = itemRef.value && itemRef.value.contains(e.target)
  if (inSub || inParent) {
    clearTimeout(closeTimer)
  } else {
    tryClose()
  }
}

watch(subOpen, (open) => {
  if (open) {
    document.addEventListener('mouseover', onDocMouseOver)
  } else {
    document.removeEventListener('mouseover', onDocMouseOver)
  }
})

onBeforeUnmount(() => {
  clearTimeout(closeTimer)
  subOpen.value = false
  document.removeEventListener('mouseover', onDocMouseOver)
})
</script>
