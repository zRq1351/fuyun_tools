<template>
  <div class="controls-page">
    <!-- 上组：窗口控制 -->
    <div
      class="fgroup top"
      :class="{ open: topOpen }"
    >
      <button
        class="fbtn handle"
        :class="{ live: state.connected }"
        :title="topOpen ? '收起窗口控制' : '窗口控制'"
        @click="toggleGroup('top')"
      >
        <svg
          v-if="!topOpen"
          viewBox="0 0 24 24"
        ><path d="M9.5 3.5v4a2 2 0 0 1-2 2h-4M14.5 20.5v-4a2 2 0 0 1 2-2h4M9.5 20.5v-4a2 2 0 0 0-2-2h-4M14.5 3.5v4a2 2 0 0 0 2 2h4" /></svg>
        <svg
          v-else
          viewBox="0 0 24 24"
        ><path d="M6 12h12" /></svg>
      </button>
      <template v-if="topOpen">
        <button
          v-if="!state.isFs"
          class="fbtn"
          title="最小化"
          @click="cmd('win-min')"
        >
          <svg viewBox="0 0 24 24"><path d="M6 12h12" /></svg>
        </button>
        <button
          class="fbtn"
          :title="state.isFs ? '退出全屏' : '铺满屏幕'"
          @click="cmd('win-fs')"
        >
          <svg
            v-if="!state.isFs"
            viewBox="0 0 24 24"
          ><rect
            x="5.5"
            y="5.5"
            width="13"
            height="13"
            rx="1.5"
          /></svg>
          <svg
            v-else
            viewBox="0 0 24 24"
          ><path d="M9.5 3.5v4a2 2 0 0 1-2 2h-4M14.5 20.5v-4a2 2 0 0 1 2-2h4M9.5 20.5v-4a2 2 0 0 0-2-2h-4M14.5 3.5v4a2 2 0 0 0 2 2h4" /></svg>
        </button>
        <button
          class="fbtn close"
          title="关闭投屏"
          @click="cmd('win-close')"
        >
          <svg viewBox="0 0 24 24"><path d="m6 6 12 12M18 6 6 18" /></svg>
        </button>
      </template>
    </div>

    <!-- 下组：手机导航与投屏功能 -->
    <div
      class="fgroup bottom"
      :class="{ open: bottomOpen }"
    >
      <button
        class="fbtn handle"
        :title="bottomOpen ? '收起功能面板' : '投屏功能'"
        @click="toggleGroup('bottom')"
      >
        <svg
          v-if="!bottomOpen"
          viewBox="0 0 24 24"
        ><rect
          x="7.5"
          y="3.5"
          width="9"
          height="17"
          rx="2"
        /><path d="M10.5 17.5h3" /></svg>
        <svg
          v-else
          viewBox="0 0 24 24"
        ><path d="M6 12h12" /></svg>
      </button>
      <template v-if="bottomOpen">
        <button
          class="fbtn"
          :disabled="!state.connected"
          title="返回"
          @click="cmd('nav-back')"
        >
          <svg viewBox="0 0 24 24"><path d="M15.5 4.5 8 12l7.5 7.5" /></svg>
        </button>
        <button
          class="fbtn"
          :disabled="!state.connected"
          title="主页"
          @click="cmd('nav-home')"
        >
          <svg viewBox="0 0 24 24"><circle
            cx="12"
            cy="12"
            r="7.5"
          /></svg>
        </button>
        <button
          class="fbtn"
          :disabled="!state.connected"
          title="最近任务"
          @click="cmd('nav-recents')"
        >
          <svg viewBox="0 0 24 24"><rect
            x="5"
            y="5"
            width="14"
            height="14"
            rx="2.5"
          /></svg>
        </button>
        <button
          class="fbtn"
          :disabled="!state.connected"
          title="旋转屏幕"
          @click="cmd('nav-rotate')"
        >
          <svg viewBox="0 0 24 24"><path d="M4.5 9a8 8 0 0 1 14.3-2.6M19.5 15a8 8 0 0 1-14.3 2.6M18.8 3.2v3.4h-3.4M5.2 20.8v-3.4h3.4" /></svg>
        </button>
        <button
          class="fbtn"
          :class="{ active: state.appMode }"
          :disabled="!state.connected"
          :title="state.appMode ? '退出独立投屏（回到整屏）' : '独立投屏：把手机当前应用投到独立窗口，手机可自由使用'"
          @click="cmd('app-toggle')"
        >
          <svg
            v-if="!state.appMode"
            viewBox="0 0 24 24"
          ><rect
            x="3.5"
            y="3.5"
            width="17"
            height="17"
            rx="2"
          /><rect
            x="9"
            y="9"
            width="9"
            height="9"
            rx="1.5"
            class="fill"
          /></svg>
          <svg
            v-else
            viewBox="0 0 24 24"
          ><rect
            x="3.5"
            y="3.5"
            width="17"
            height="17"
            rx="2"
          /><path d="M9 12h6M12 9l3 3-3 3" /></svg>
        </button>
        <button
          class="fbtn"
          :class="{ active: state.screenOff }"
          :disabled="!state.connected"
          :title="state.screenOff ? '点亮手机屏幕' : '息屏投屏：关闭手机面板，投屏与控制继续'"
          @click="cmd('screen-off')"
        >
          <svg
            v-if="!state.screenOff"
            viewBox="0 0 24 24"
          ><path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z" /></svg>
          <svg
            v-else
            viewBox="0 0 24 24"
          ><circle
            cx="12"
            cy="12"
            r="4"
          /><path d="M12 2.5v2.5M12 19v2.5M2.5 12H5M19 12h2.5M5.2 5.2l1.8 1.8M17 17l1.8 1.8M18.8 5.2 17 7M7 17l-1.8 1.8" /></svg>
        </button>
        <button
          class="fbtn"
          :class="{ active: state.wheelSwipeMode }"
          :disabled="!state.connected"
          :title="state.wheelSwipeMode ? '滚轮翻页已开启（滚轮=快速滑动，短视频直接翻页）；点击恢复平滑滚动' : '滚轮翻页：滚轮模拟快速滑动手势（看短视频用）'"
          @click="cmd('wheel-toggle')"
        >
          <svg viewBox="0 0 24 24"><path d="M12 5v14M7.5 9.5 12 5l4.5 4.5M7.5 14.5 12 19l4.5-4.5" /></svg>
        </button>
        <button
          class="fbtn"
          :disabled="!state.connected"
          title="导出手机最近文件/图片到电脑"
          @click="cmd('export')"
        >
          <svg viewBox="0 0 24 24"><path d="M12 4v10M8 10.5 12 14.5l4-4M5 19h14" /></svg>
        </button>
      </template>
    </div>
  </div>
</template>

<script setup>
import {onMounted, onUnmounted, reactive, ref} from 'vue'
import {emit, listen} from '@tauri-apps/api/event'
import {getCurrentWindow} from '@tauri-apps/api/window'

const topOpen = ref(false)
const bottomOpen = ref(false)
const state = reactive({
  connected: false,
  screenOff: false,
  wheelSwipeMode: false,
  appMode: null,
  isFs: false,
})

let collapseTimer = null
let unlistenState = null

function toggleGroup(which) {
  if (which === 'top') {
    topOpen.value = !topOpen.value
    bottomOpen.value = false
  } else {
    bottomOpen.value = !bottomOpen.value
    topOpen.value = false
  }
  if (topOpen.value || bottomOpen.value) {
    if (collapseTimer) clearTimeout(collapseTimer)
    collapseTimer = setTimeout(() => {
      topOpen.value = false
      bottomOpen.value = false
    }, 5000)
  }
}

function cmd(action) {
  emit('mirror-cmd', {action})
  if (collapseTimer) clearTimeout(collapseTimer)
  collapseTimer = setTimeout(() => {
    topOpen.value = false
    bottomOpen.value = false
  }, 5000)
}

onMounted(async () => {
  unlistenState = await listen('mirror-state', (e) => {
    const p = e.payload || {}
    Object.assign(state, p)
    // 主窗口铺满屏幕时隐藏控制条窗口
    const win = getCurrentWindow()
    if (p.isFs) {
      win.hide().catch(() => {})
    } else {
      win.show().catch(() => {})
    }
  })
  // 向主窗口请求当前状态
  emit('mirror-cmd', {action: 'get-state'})
})

onUnmounted(() => {
  if (collapseTimer) clearTimeout(collapseTimer)
  if (unlistenState) unlistenState()
})
</script>

<style>
.controls-page {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  align-items: center;
  padding: 8px 0;
  background: transparent;
  font-family: 'Segoe UI', 'Microsoft YaHei', sans-serif;
  user-select: none;
}

.fgroup {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 6px;
  background: rgba(16, 20, 24, 0.92);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 12px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.45);
}

.fbtn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 34px;
  background: rgba(255, 255, 255, 0.06);
  color: #cfd6dd;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.fbtn svg {
  width: 16px;
  height: 16px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.fbtn svg .fill {
  fill: currentColor;
  stroke: none;
}

.fbtn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.14);
  color: #fff;
}

.fbtn:disabled {
  opacity: 0.35;
  cursor: default;
}

.fbtn.active {
  background: #35bf6c;
  color: #fff;
  border-color: #35bf6c;
}

.fbtn.close:hover {
  background: rgba(220, 60, 60, 0.4);
  color: #fff;
}

.fbtn.handle.live {
  border-color: #35bf6c;
  box-shadow: 0 0 6px rgba(53, 191, 108, 0.5);
}
</style>
