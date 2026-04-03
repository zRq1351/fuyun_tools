<template>
  <el-config-provider :locale="zhCn">
    <div
        :class="{'bar-collapsed': isToolbarCollapsed}"
        :data-tauri-drag-region="isToolbarCollapsed ? null : ''"
        class="bar"
        @mouseenter="onBarMouseEnter"
        @mouseleave="onBarMouseLeave"
    >
      <div
          v-if="isToolbarCollapsed"
          :data-state="rawRecordingState"
          :title="collapsedPillText"
          class="collapsed-pill"
      ></div>

      <div v-else class="expanded-content">
        <div class="time">{{ elapsedText }}</div>
      <span class="no-drag" @mouseenter="onButtonHoverChange(true)" @mouseleave="onButtonHoverChange(false)">
        <el-popover
            v-model:visible="microphonePopoverVisible"
            placement="bottom-start"
            popper-class="recording-toolbar-select-popper"
            trigger="manual"
            @hide="onSelectVisibleChange(false)"
            @show="onSelectVisibleChange(true)"
        >
          <div class="device-list">
            <div
                v-for="item in microphones"
                :key="item.id"
                :data-active="item.id === microphoneDeviceId"
                class="device-item"
                @click="selectMicrophone(item.id)"
            >
              {{ item.name }}
            </div>
            <div v-if="microphones.length === 0" class="device-empty">暂无麦克风设备</div>
          </div>
          <template #reference>
            <el-button
                circle
                class="icon-btn"
                size="small"
                @click="toggleMicrophone"
            >
              <el-icon v-if="captureMicrophone"><Mic :size="18" :stroke-width="2.2"/></el-icon>
              <el-icon v-else><MicOff :size="18" :stroke-width="2.2"/></el-icon>
            </el-button>
          </template>
        </el-popover>
      </span>

      <span class="no-drag" @mouseenter="onButtonHoverChange(true)" @mouseleave="onButtonHoverChange(false)">
        <el-popover
            v-model:visible="systemAudioPopoverVisible"
            :width="systemOutputListWidth"
            placement="bottom-start"
            popper-class="recording-toolbar-select-popper"
            trigger="manual"
            @hide="onSelectVisibleChange(false)"
            @show="onSelectVisibleChange(true)"
        >
          <div class="device-list">
            <div
                v-for="item in systemOutputs"
                :key="item.id"
                :data-active="item.id === systemOutputId"
                class="device-item"
                @click="selectSystemOutput(item.id)"
            >
              {{ item.name }}
            </div>
            <div v-if="systemOutputs.length === 0" class="device-empty">暂无系统音频设备</div>
          </div>
          <template #reference>
            <el-button
                circle
                class="icon-btn"
                size="small"
                @click="toggleSystemAudio"
            >
              <el-icon v-if="captureSystemAudio"><Volume2 :size="18" :stroke-width="2.2"/></el-icon>
              <el-icon v-else><VolumeOff :size="18" :stroke-width="2.2"/></el-icon>
            </el-button>
          </template>
        </el-popover>
      </span>

      <span class="no-drag" @mouseenter="onButtonHoverChange(true)" @mouseleave="onButtonHoverChange(false)">
        <el-tooltip content="开始录制" effect="dark" placement="bottom" @visible-change="onTooltipVisibleChange">
          <el-button
              :loading="loading"
              :disabled="!canStart"
              circle
              class="icon-btn"
              size="small"
              @click="start"
          >
            <el-icon><VideoPlay/></el-icon>
          </el-button>
        </el-tooltip>
      </span>
      <span class="no-drag" @mouseenter="onButtonHoverChange(true)" @mouseleave="onButtonHoverChange(false)">
        <el-tooltip content="暂停录制" effect="dark" placement="bottom" @visible-change="onTooltipVisibleChange">
          <el-button
              :loading="loading"
              :disabled="!canPause"
              circle
              class="icon-btn"
              size="small"
              @click="pause"
          >
            <el-icon><VideoPause/></el-icon>
          </el-button>
        </el-tooltip>
      </span>
      <span class="no-drag" @mouseenter="onButtonHoverChange(true)" @mouseleave="onButtonHoverChange(false)">
        <el-tooltip content="恢复录制" effect="dark" placement="bottom" @visible-change="onTooltipVisibleChange">
          <el-button
              :loading="loading"
              :disabled="!canResume"
              circle
              class="icon-btn"
              size="small"
              @click="resume"
          >
            <el-icon><RefreshRight/></el-icon>
          </el-button>
        </el-tooltip>
      </span>
      <span class="no-drag" @mouseenter="onButtonHoverChange(true)" @mouseleave="onButtonHoverChange(false)">
        <el-tooltip content="停止录制" effect="dark" placement="bottom" @visible-change="onTooltipVisibleChange">
          <el-button
              :loading="loading"
              :disabled="!canStop"
              circle
              class="icon-btn"
              size="small"
              @click="stop"
          >
            <el-icon><SwitchButton/></el-icon>
          </el-button>
        </el-tooltip>
      </span>
      <span class="no-drag" @mouseenter="onButtonHoverChange(true)" @mouseleave="onButtonHoverChange(false)">
        <el-tooltip content="打开视频目录" effect="dark" placement="bottom" @visible-change="onTooltipVisibleChange">
          <el-button
              :loading="loading"
              circle
              class="icon-btn"
              size="small"
              @click="openFolder"
          >
            <el-icon><FolderOpen :size="18" :stroke-width="2.2"/></el-icon>
          </el-button>
        </el-tooltip>
      </span>
      <span class="no-drag" @mouseenter="onButtonHoverChange(true)" @mouseleave="onButtonHoverChange(false)">
        <el-tooltip :content="closeTooltipText" effect="dark" placement="bottom"
                    @visible-change="onTooltipVisibleChange">
          <el-button
              circle
              class="icon-btn"
              size="small"
              @click="closeBar"
          >
            <el-icon><Close/></el-icon>
          </el-button>
        </el-tooltip>
      </span>
      </div>
    </div>
  </el-config-provider>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, reactive, ref, watch} from 'vue'
import zhCn from 'element-plus/dist/locale/zh-cn'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWindow} from '@tauri-apps/api/window'
import {AISettingsService, RecordingService} from '@/services/ipc.js'
import {ElMessage} from 'element-plus'
import {Close} from "@element-plus/icons-vue";
import {FolderOpen, Mic, MicOff, Volume2, VolumeOff} from 'lucide-vue-next'

const loading = ref(false)
const captureSystemAudio = ref(false)
const captureMicrophone = ref(true)
const microphonePopoverVisible = ref(false)
const systemAudioPopoverVisible = ref(false)
const systemOutputs = ref([])
const systemOutputId = ref(null)
const microphones = ref([])
const microphoneDeviceId = ref(null)
const fps = ref(30)
const state = reactive({state: 'idle', sessionId: null, elapsedMs: 0})
let unlistenStateChanged = null
let openSelectCount = 0
let openTooltipCount = 0
let openButtonHoverCount = 0
let tooltipHideResizeTimer = null
let toolbarCollapseTimer = null
const TOOLTIP_HIDE_GRACE_MS = 450
const TOOLBAR_COLLAPSE_GRACE_MS = 220
const isToolbarCollapsed = ref(false)

let _measureCanvas = null
const ensureMeasureCtx = () => {
  if (!_measureCanvas) {
    _measureCanvas = document.createElement('canvas')
  }
  const ctx = _measureCanvas.getContext('2d')
  ctx.font = '14px -apple-system, BlinkMacSystemFont, Segoe UI, Roboto, Helvetica, Arial, \"Microsoft Yahei\", sans-serif'
  return ctx
}
const computeListWidth = (items, labelSelector) => {
  const names = Array.isArray(items) ? items.map(labelSelector) : []
  if (names.length === 0) return 220
  const ctx = ensureMeasureCtx()
  let max = 0
  for (const s of names) {
    max = Math.max(max, ctx.measureText(String(s || '')).width)
  }
  // 左右内边距与滚动条余量
  const padding = 20 + 16
  const raw = Math.ceil(max + padding)
  const minW = 220
  const maxW = 900
  return Math.max(minW, Math.min(maxW, raw))
}
const microphoneListWidth = computed(() => computeListWidth(microphones.value, it => it.name))
const systemOutputListWidth = computed(() => computeListWidth(systemOutputs.value, it => it.name))

const elapsedText = computed(() => `${Math.floor(state.elapsedMs / 1000)}s`)
const rawRecordingState = computed(() => String(state.state || 'idle').toLowerCase())
const currentRecordingState = computed(() => {
  const normalized = rawRecordingState.value
  if (
      normalized === 'idle'
      || normalized === 'recording'
      || normalized === 'paused'
      || normalized === 'starting'
      || normalized === 'stopping'
      || normalized === 'error'
  ) {
    return normalized
  }
  return state.sessionId ? 'recording' : 'idle'
})
const recordingHintText = computed(() => {
  if (rawRecordingState.value === 'recording') return '正在录屏'
  if (rawRecordingState.value === 'paused') return '录屏已暂停'
  if (rawRecordingState.value === 'starting') return '录屏启动中'
  if (rawRecordingState.value === 'stopping') return '录屏停止中'
  if (rawRecordingState.value === 'error') return '录屏异常'
  return '录屏处理中'
})
const collapsedPillText = computed(() => {
  if (rawRecordingState.value === 'recording' || rawRecordingState.value === 'paused') {
    return `${recordingHintText.value} · ${elapsedText.value}`
  }
  return recordingHintText.value
})
const phaseActionRules = computed(() => {
  switch (currentRecordingState.value) {
    case 'recording':
      return {start: false, pause: true, resume: false, stop: true}
    case 'paused':
      return {start: false, pause: false, resume: true, stop: true}
    case 'starting':
    case 'stopping':
      return {start: false, pause: false, resume: false, stop: false}
    case 'error':
      return {start: true, pause: false, resume: false, stop: false}
    default:
      return {start: true, pause: false, resume: false, stop: false}
  }
})
const canStart = computed(() => !loading.value && phaseActionRules.value.start)
const canPause = computed(() => !loading.value && phaseActionRules.value.pause)
const canResume = computed(() => !loading.value && phaseActionRules.value.resume)
const canStop = computed(() => !loading.value && phaseActionRules.value.stop)
const isRecordingSessionActive = computed(() =>
    ['starting', 'recording', 'paused', 'stopping'].includes(currentRecordingState.value)
)
const closeTooltipText = computed(() => {
  if (currentRecordingState.value === 'recording' || currentRecordingState.value === 'paused') {
    return '隐藏工具栏（录制继续）'
  }
  return '关闭工具栏'
})

const refresh = async () => {
  const data = await RecordingService.getState()
  state.state = data.state || state.state || 'idle'
  state.sessionId = data.sessionId || null
  state.elapsedMs = Number(data.elapsedMs || 0)
}

const start = async () => {
  loading.value = true
  try {
    await RecordingService.start({
      captureSystemAudio: captureSystemAudio.value,
      systemAudioDeviceId: systemOutputId.value,
      captureMicrophone: captureMicrophone.value,
      microphoneDeviceId: microphoneDeviceId.value,
      captureCursor: true,
      fps: fps.value
    })
    await refresh()
  } catch (e) {
    ElMessage.error(String(e))
  } finally {
    loading.value = false
  }
}

const pause = async () => {
  loading.value = true
  try {
    await RecordingService.pause();
    await refresh()
  } catch (e) {
    ElMessage.error(String(e))
  } finally {
    loading.value = false
  }
}
const resume = async () => {
  loading.value = true
  try {
    await RecordingService.resume();
    await refresh()
  } catch (e) {
    ElMessage.error(String(e))
  } finally {
    loading.value = false
  }
}
const stop = async () => {
  loading.value = true
  try {
    await RecordingService.stop(state.sessionId);
    await refresh()
  } catch (e) {
    ElMessage.error(String(e))
  } finally {
    loading.value = false
  }
}

const openFolder = async () => {
  loading.value = true
  try {
    await RecordingService.openFolder()
  } catch (e) {
    ElMessage.error(String(e))
  } finally {
    loading.value = false
  }
}

const closeBar = async () => {
  try {
    await getCurrentWindow().hide()
  } catch (_e) {
  }
}

const applyToolbarWindowSize = async () => {
  try {
    await RecordingService.resizeToolbar(
        openSelectCount > 0,
        openTooltipCount > 0 || openButtonHoverCount > 0,
        isToolbarCollapsed.value
    )
  } catch (_e) {
  }
}

const hasExpandedOverlay = () => openSelectCount > 0 || openTooltipCount > 0 || openButtonHoverCount > 0

const clearToolbarCollapseTimer = () => {
  if (!toolbarCollapseTimer) return
  clearTimeout(toolbarCollapseTimer)
  toolbarCollapseTimer = null
}

const setToolbarCollapsed = async (collapsed) => {
  const next = collapsed && isRecordingSessionActive.value
  if (isToolbarCollapsed.value === next) return
  isToolbarCollapsed.value = next
  await applyToolbarWindowSize()
}

const scheduleToolbarCollapse = () => {
  clearToolbarCollapseTimer()
  if (!isRecordingSessionActive.value || hasExpandedOverlay()) return
  toolbarCollapseTimer = setTimeout(() => {
    toolbarCollapseTimer = null
    setToolbarCollapsed(true)
  }, TOOLBAR_COLLAPSE_GRACE_MS)
}

const onBarMouseEnter = async () => {
  clearToolbarCollapseTimer()
  await setToolbarCollapsed(false)
}

const onBarMouseLeave = () => {
  scheduleToolbarCollapse()
}

const scheduleShrinkToolbarWindowSize = () => {
  if (tooltipHideResizeTimer) {
    clearTimeout(tooltipHideResizeTimer)
  }
  tooltipHideResizeTimer = setTimeout(() => {
    tooltipHideResizeTimer = null
    if (!hasExpandedOverlay()) {
      if (isRecordingSessionActive.value) {
        scheduleToolbarCollapse()
      } else {
        setToolbarCollapsed(false)
      }
    }
  }, TOOLTIP_HIDE_GRACE_MS)
}

const onSelectVisibleChange = async (visible) => {
  if (visible && tooltipHideResizeTimer) {
    clearTimeout(tooltipHideResizeTimer)
    tooltipHideResizeTimer = null
  }
  openSelectCount += visible ? 1 : -1
  if (openSelectCount < 0) openSelectCount = 0
  if (hasExpandedOverlay()) {
    await setToolbarCollapsed(false)
    await applyToolbarWindowSize()
  } else {
    scheduleShrinkToolbarWindowSize()
  }
}

const onTooltipVisibleChange = async (visible) => {
  if (visible) {
    if (tooltipHideResizeTimer) {
      clearTimeout(tooltipHideResizeTimer)
      tooltipHideResizeTimer = null
    }
    openTooltipCount += 1
    await setToolbarCollapsed(false)
    await applyToolbarWindowSize()
    return
  }

  openTooltipCount -= 1
  if (openTooltipCount < 0) openTooltipCount = 0
  if (hasExpandedOverlay()) {
    await applyToolbarWindowSize()
    return
  }
  scheduleShrinkToolbarWindowSize()
}

const onButtonHoverChange = async (visible) => {
  if (visible && tooltipHideResizeTimer) {
    clearTimeout(tooltipHideResizeTimer)
    tooltipHideResizeTimer = null
  }
  openButtonHoverCount += visible ? 1 : -1
  if (openButtonHoverCount < 0) openButtonHoverCount = 0
  if (hasExpandedOverlay()) {
    await setToolbarCollapsed(false)
    await applyToolbarWindowSize()
  } else {
    scheduleShrinkToolbarWindowSize()
  }
}

const toggleMicrophone = async () => {
  if (captureMicrophone.value) {
    captureMicrophone.value = false
    microphonePopoverVisible.value = false
    return
  }
  captureMicrophone.value = true
  if (!microphoneDeviceId.value && microphones.value.length > 0) {
    const def = microphones.value.find(it => it.isDefault)
    microphoneDeviceId.value = def ? def.id : microphones.value[0].id
  }
  microphonePopoverVisible.value = true
}

const selectMicrophone = async (deviceId) => {
  microphoneDeviceId.value = deviceId
  microphonePopoverVisible.value = false
}

const toggleSystemAudio = async () => {
  if (captureSystemAudio.value) {
    captureSystemAudio.value = false
    systemAudioPopoverVisible.value = false
    return
  }
  captureSystemAudio.value = true
  if (!systemOutputId.value && systemOutputs.value.length > 0) {
    const def = systemOutputs.value.find(it => it.isDefault)
    systemOutputId.value = def ? def.id : systemOutputs.value[0].id
  }
  systemAudioPopoverVisible.value = true
}

const selectSystemOutput = async (deviceId) => {
  systemOutputId.value = deviceId
  systemAudioPopoverVisible.value = false
}

onMounted(async () => {
  unlistenStateChanged = await listen('recording-state-changed', (event) => {
    const payload = event.payload || {}
    state.state = payload.state || state.state
    state.sessionId = payload.sessionId ?? state.sessionId
    state.elapsedMs = Number(payload.elapsedMs || state.elapsedMs || 0)
  })
  try {
    const settings = await AISettingsService.getSettings()
    captureSystemAudio.value = settings.recording_capture_system_audio === true
    captureMicrophone.value = settings.recording_capture_microphone !== false
    fps.value = Number(settings.recording_default_fps || 30)
  } catch (_e) {
  }
  try {
    const outs = await RecordingService.listSystemOutputs()
    systemOutputs.value = Array.isArray(outs) ? outs : []
    const def = systemOutputs.value.find(it => it.isDefault)
    systemOutputId.value = def ? def.id : (systemOutputs.value[0]?.id ?? null)
  } catch (e) {
    systemOutputs.value = []
    systemOutputId.value = null
    ElMessage.error(`加载系统音频设备失败: ${String(e)}`)
  }
  try {
    const mics = await RecordingService.listAudioDevices()
    microphones.value = Array.isArray(mics) ? mics : []
    const def = microphones.value.find(it => it.isDefault)
    microphoneDeviceId.value = def ? def.id : (microphones.value[0]?.id ?? null)
  } catch (e) {
    microphones.value = []
    microphoneDeviceId.value = null
    ElMessage.error(`加载麦克风设备失败: ${String(e)}`)
  }
  await refresh()
  if (isRecordingSessionActive.value) {
    scheduleToolbarCollapse()
  } else {
    await setToolbarCollapsed(false)
  }
})

watch(currentRecordingState, async (next) => {
  if (next === 'idle' || next === 'error') {
    clearToolbarCollapseTimer()
    await setToolbarCollapsed(false)
    return
  }
  if (isRecordingSessionActive.value) {
    scheduleToolbarCollapse()
  }
})

onBeforeUnmount(() => {
  if (tooltipHideResizeTimer) {
    clearTimeout(tooltipHideResizeTimer)
    tooltipHideResizeTimer = null
  }
  clearToolbarCollapseTimer()
  if (unlistenStateChanged) unlistenStateChanged()
})
</script>

<style>
body {
  margin: 0;
  padding: 0;
  overflow: hidden;
  background: transparent;
}

html, body, #app {
  height: 100%;
  background: transparent;
}

.recording-toolbar-select-popper.el-popper {
  --el-bg-color-overlay: #171b24;
  --el-fill-color-light: #252b38;
  --el-border-color-light: rgba(255, 255, 255, 0.16);
  --el-text-color-primary: #e9eefc;
}

.recording-toolbar-select-popper .el-select-dropdown__item {
  color: #e9eefc;
}

.recording-toolbar-select-popper .el-select-dropdown__item.hover,
.recording-toolbar-select-popper .el-select-dropdown__item:hover {
  background: rgba(114, 183, 255, 0.18);
}

.recording-toolbar-select-popper .el-select-dropdown__item.selected {
  color: #7bb8ff;
  font-weight: 600;
}

.recording-toolbar-select-popper.el-popper {
  max-width: 940px;
  overflow: hidden;
}

.recording-toolbar-select-popper .device-list {
  width: auto;
  max-width: 900px;
  max-height: 260px;
  overflow-y: auto;
  overflow-x: hidden;
  box-sizing: border-box;
}

.recording-toolbar-select-popper .device-item {
  display: block;
  line-height: 32px;
  padding: 0 10px;
  color: #e9eefc;
  cursor: pointer;
  border-radius: 6px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

.recording-toolbar-select-popper .device-item:hover {
  background: rgba(114, 183, 255, 0.18);
}

.recording-toolbar-select-popper .device-item[data-active="true"] {
  color: #7bb8ff;
  font-weight: 600;
}

.recording-toolbar-select-popper .device-empty {
  line-height: 32px;
  padding: 0 10px;
  color: rgba(233, 238, 252, 0.72);
}
</style>

<style scoped>
.bar {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  background: rgba(17, 22, 32, 0.92);
  border: none;
  border-radius: 10px;
  padding: 8px;
  flex-wrap: nowrap;
  white-space: nowrap;
  overflow: hidden;
  cursor: move;
  -webkit-app-region: drag;
}

.bar.bar-collapsed {
  width: 100%;
  height: 100%;
  padding: 2px;
  gap: 0;
  justify-content: center;
  align-items: center;
  border-radius: 999px;
  background: transparent;
  border: none;
  overflow: visible;
  box-sizing: border-box;
  cursor: default;
  -webkit-app-region: no-drag;
}

.expanded-content {
  display: flex;
  align-items: center;
  gap: 8px;
}

.time {
  min-width: 54px;
  color: #fff;
  font-size: 13px;
  font-weight: 600;
  text-align: center;
  user-select: none;
}

.collapsed-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 1;
  width: auto;
  min-width: 0;
  height: calc(100% - 1px);
  padding: 0;
  box-sizing: border-box;
  border-radius: 999px;
  background: linear-gradient(90deg, #2f9dfb 0%, #5fc0ff 100%);
  border: 1px solid rgba(118, 196, 255, 0.92);
  box-shadow: inset 0 0 0 1px rgba(20, 62, 92, 0.22);
  user-select: none;
  pointer-events: none;
}

.bar:not(.bar-collapsed) .collapsed-pill {
  flex: none;
  width: 166px;
  height: 22px;
}

.collapsed-pill[data-state="paused"] {
  background: linear-gradient(90deg, #e4b22c 0%, #ffd96f 100%);
  border-color: rgba(255, 219, 116, 0.95);
  box-shadow: inset 0 0 0 1px rgba(107, 79, 16, 0.2);
}

.collapsed-pill[data-state="stopping"],
.collapsed-pill[data-state="starting"] {
  background: linear-gradient(90deg, #6ea7d3 0%, #8ac0ea 100%);
  border-color: rgba(163, 208, 238, 0.9);
}

.collapsed-pill[data-state="error"] {
  background: linear-gradient(90deg, #d55b5b 0%, #ef8b8b 100%);
  border-color: rgba(244, 159, 159, 0.9);
  box-shadow: inset 0 0 0 1px rgba(116, 37, 37, 0.24);
}

.no-drag {
  -webkit-app-region: no-drag;
}

.no-drag :deep(.el-select),
.no-drag :deep(.el-button),
.no-drag :deep(.el-switch) {
  cursor: default;
}

.icon-btn:deep(.el-button) {
  background: transparent !important;
  border-color: transparent !important;
  color: #e9eefc !important;
}

.icon-btn:deep(.el-button:hover) {
  background: rgba(255, 255, 255, 0.08) !important;
  border-color: rgba(255, 255, 255, 0.12) !important;
}

.icon-btn:deep(.el-icon) {
  font-size: 18px;
}
</style>
