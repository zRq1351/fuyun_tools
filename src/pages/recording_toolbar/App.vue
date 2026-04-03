<template>
  <el-config-provider :locale="zhCn">
    <div class="bar" data-tauri-drag-region>
      <div class="time">{{ elapsedText }}</div>

      <span class="no-drag">
        <el-popover
            v-model:visible="fpsPopoverVisible"
            placement="bottom-start"
            popper-class="recording-toolbar-select-popper"
            trigger="click"
            @hide="onSelectVisibleChange(false)"
            @show="onSelectVisibleChange(true)"
        >
          <div class="fps-list">
            <div
                v-for="v in fpsOptions"
                :key="v"
                :data-active="v === fps"
                class="fps-item"
                @click="selectFps(v)"
            >
              {{ v }}fps
            </div>
          </div>
          <template #reference>
            <el-button class="fps-btn" size="small" style="width: 90px">{{ fps }}fps</el-button>
          </template>
        </el-popover>
      </span>

      <span class="no-drag" @mouseenter="onButtonHoverChange(true)" @mouseleave="onButtonHoverChange(false)">
        <el-popover
            v-model:visible="microphonePopoverVisible"
            :width="microphoneListWidth"
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
              <el-icon v-if="captureMicrophone"><Microphone/></el-icon>
              <el-icon v-else>
                <svg fill="none" viewBox="0 0 24 24">
                  <path d="M4 9.5H8L13.2 5.5V18.5L8 14.5H4V9.5Z" stroke="currentColor" stroke-linejoin="round"
                        stroke-width="1.8"/>
                  <path d="M15.2 9.1L20.2 15.1" stroke="currentColor" stroke-linecap="round" stroke-width="1.8"/>
                </svg>
              </el-icon>
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
              <el-icon v-if="captureSystemAudio">
                <svg fill="none" viewBox="0 0 24 24">
                  <path d="M4 9.5H8L13.2 5.5V18.5L8 14.5H4V9.5Z" stroke="currentColor" stroke-linejoin="round"
                        stroke-width="1.8"/>
                  <path d="M16 10.2C17.1 11.2 17.1 12.8 16 13.8" stroke="currentColor" stroke-linecap="round"
                        stroke-width="1.8"/>
                  <path d="M18.4 8.2C20.5 10.3 20.5 13.7 18.4 15.8" stroke="currentColor" stroke-linecap="round"
                        stroke-width="1.8"/>
                </svg>
              </el-icon>
              <el-icon v-else><Mute/></el-icon>
            </el-button>
          </template>
        </el-popover>
      </span>

      <span class="no-drag" @mouseenter="onButtonHoverChange(true)" @mouseleave="onButtonHoverChange(false)">
        <el-tooltip content="开始录制" effect="dark" placement="bottom" @visible-change="onTooltipVisibleChange">
          <el-button
              :loading="loading"
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
          <el-button
              circle
              class="icon-btn"
              size="small"
              @click="closeBar"
          >
            <el-icon><Close/></el-icon>
          </el-button>
      </span>
    </div>
  </el-config-provider>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, reactive, ref} from 'vue'
import zhCn from 'element-plus/dist/locale/zh-cn'
import {listen} from '@tauri-apps/api/event'
import {getCurrentWindow} from '@tauri-apps/api/window'
import {AISettingsService, RecordingService} from '@/services/ipc.js'
import {ElMessage} from 'element-plus'
import {Close} from "@element-plus/icons-vue";

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
const fpsPopoverVisible = ref(false)
const fpsOptions = [15, 20, 24, 30, 45, 60]
const state = reactive({sessionId: null, elapsedMs: 0})
let unlistenStateChanged = null
let openSelectCount = 0
let openTooltipCount = 0
let openButtonHoverCount = 0
let tooltipHideResizeTimer = null
const TOOLTIP_HIDE_GRACE_MS = 450

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

const refresh = async () => {
  const data = await RecordingService.getState()
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

const closeBar = async () => {
  try {
    await getCurrentWindow().hide()
  } catch (_e) {
  }
}

const applyToolbarWindowSize = async () => {
  try {
    await RecordingService.resizeToolbar(openSelectCount > 0, openTooltipCount > 0 || openButtonHoverCount > 0)
  } catch (_e) {
  }
}

const hasExpandedOverlay = () => openSelectCount > 0 || openTooltipCount > 0 || openButtonHoverCount > 0

const scheduleShrinkToolbarWindowSize = () => {
  if (tooltipHideResizeTimer) {
    clearTimeout(tooltipHideResizeTimer)
  }
  tooltipHideResizeTimer = setTimeout(() => {
    tooltipHideResizeTimer = null
    if (!hasExpandedOverlay()) {
      applyToolbarWindowSize()
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
    await applyToolbarWindowSize()
  } else {
    scheduleShrinkToolbarWindowSize()
  }
}

const selectFps = async (v) => {
  fps.value = v
  fpsPopoverVisible.value = false
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
})

onBeforeUnmount(() => {
  if (tooltipHideResizeTimer) {
    clearTimeout(tooltipHideResizeTimer)
    tooltipHideResizeTimer = null
  }
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

.recording-toolbar-select-popper .fps-list {
  min-width: 96px;
}

.recording-toolbar-select-popper .fps-item {
  line-height: 32px;
  padding: 0 10px;
  color: #e9eefc;
  cursor: pointer;
  border-radius: 6px;
}

.recording-toolbar-select-popper .fps-item:hover {
  background: rgba(114, 183, 255, 0.18);
}

.recording-toolbar-select-popper .fps-item[data-active="true"] {
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

.time {
  min-width: 54px;
  color: #fff;
  font-size: 13px;
  font-weight: 600;
  text-align: center;
  user-select: none;
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
