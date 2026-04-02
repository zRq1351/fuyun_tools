<template>
  <el-config-provider :locale="zhCn">
    <div class="recording-page">
      <el-card class="panel" shadow="never">
        <template #header>
          <div class="title">录屏控制台</div>
        </template>

        <el-space direction="vertical" fill>
          <el-descriptions :column="1" border>
            <el-descriptions-item label="状态">{{ state.state }}</el-descriptions-item>
            <el-descriptions-item label="会话">{{ state.sessionId || '-' }}</el-descriptions-item>
            <el-descriptions-item label="已录制">{{ elapsedText }}</el-descriptions-item>
            <el-descriptions-item label="剩余时长">{{ remainingText }}</el-descriptions-item>
            <el-descriptions-item label="帧率">{{ state.fps }} fps</el-descriptions-item>
            <el-descriptions-item label="视频码率">{{ state.videoBitrateKbps }} kbps</el-descriptions-item>
            <el-descriptions-item label="音频码率">{{ state.audioBitrateKbps }} kbps</el-descriptions-item>
            <el-descriptions-item label="丢帧">{{ state.droppedVideoFrames }}</el-descriptions-item>
          </el-descriptions>
          <el-space wrap>
            <el-switch v-model="captureSystemAudio" active-text="系统音频"/>
            <el-switch v-model="captureMicrophone" active-text="麦克风"/>
            <el-switch v-model="captureCursor" active-text="鼠标光标"/>
          </el-space>
          <el-select
              v-if="captureSystemAudio"
              v-model="systemOutputId"
              placeholder="选择系统音频输出设备"
              style="width: 360px"
          >
            <el-option
                v-for="item in systemOutputs"
                :key="item.id"
                :label="item.name"
                :value="item.id"
            />
          </el-select>
          <el-select
              v-if="captureMicrophone"
              v-model="microphoneDeviceId"
              placeholder="选择麦克风设备"
              style="width: 360px"
          >
            <el-option
                v-for="item in microphones"
                :key="item.id"
                :label="item.name"
                :value="item.id"
            />
          </el-select>

          <el-space wrap>
            <el-button :loading="loading" type="primary" @click="start">开始录制</el-button>
            <el-button :loading="loading" @click="pause">暂停</el-button>
            <el-button :loading="loading" @click="resume">恢复</el-button>
            <el-button :loading="loading" @click="stop">停止录制</el-button>
            <el-button :loading="loading" @click="refresh">刷新状态</el-button>
            <el-button :loading="loading" @click="openFolder">打开目录</el-button>
            <el-button :loading="loading" type="success" @click="runRegression">运行回归自测</el-button>
          </el-space>
          <el-alert
              v-if="regressionResult"
              :description="regressionResult"
              :title="regressionTitle"
              :type="regressionOk ? 'success' : 'error'"
              show-icon
          />
        </el-space>
      </el-card>
    </div>
  </el-config-provider>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, reactive, ref} from 'vue'
import {ElMessage} from 'element-plus'
import zhCn from 'element-plus/dist/locale/zh-cn'
import {listen} from '@tauri-apps/api/event'
import {AISettingsService, RecordingService} from '../../services/ipc'

const loading = ref(false)
const captureSystemAudio = ref(false)
const captureMicrophone = ref(true)
const captureCursor = ref(true)
const systemOutputs = ref([])
const systemOutputId = ref(null)
const microphones = ref([])
const microphoneDeviceId = ref(null)
const maxDurationMinutes = ref(180)
const regressionResult = ref('')
const regressionTitle = ref('')
const regressionOk = ref(false)
let unlistenStateChanged = null
let unlistenStatsUpdated = null
let unlistenError = null
let unlistenFinished = null
const state = reactive({
  state: 'idle',
  sessionId: null,
  elapsedMs: 0,
  fps: 0,
  videoBitrateKbps: 0,
  audioBitrateKbps: 0,
  droppedVideoFrames: 0
})

const elapsedText = computed(() => `${Math.floor(state.elapsedMs / 1000)} 秒`)
const remainingText = computed(() => {
  const remainMs = Math.max(0, maxDurationMinutes.value * 60 * 1000 - state.elapsedMs)
  return `${Math.floor(remainMs / 1000)} 秒`
})

const refresh = async () => {
  const data = await RecordingService.getState()
  state.state = data.state || 'idle'
  state.sessionId = data.sessionId || null
  state.elapsedMs = Number(data.elapsedMs || 0)
  state.droppedVideoFrames = Number(data.droppedVideoFrames || 0)
}


const start = async () => {
  loading.value = true
  try {
    await RecordingService.start({
      captureSystemAudio: captureSystemAudio.value,
      systemAudioDeviceId: systemOutputId.value,
      captureMicrophone: captureMicrophone.value,
      microphoneDeviceId: microphoneDeviceId.value,
      captureCursor: captureCursor.value
    })
    await refresh()
    ElMessage.success('录制已开始')
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    loading.value = false
  }
}


const pause = async () => {
  loading.value = true
  try {
    await RecordingService.pause()
    await refresh()
    ElMessage.success('录制已暂停')
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    loading.value = false
  }
}

const resume = async () => {
  loading.value = true
  try {
    await RecordingService.resume()
    await refresh()
    ElMessage.success('录制已恢复')
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    loading.value = false
  }
}

const stop = async () => {
  loading.value = true
  try {
    await RecordingService.stop(state.sessionId)
    await refresh()
    ElMessage.success('录制已停止')
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    loading.value = false
  }
}

const openFolder = async () => {
  loading.value = true
  try {
    await RecordingService.openFolder()
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    loading.value = false
  }
}

const runRegression = async () => {
  loading.value = true
  regressionResult.value = ''
  regressionTitle.value = ''
  regressionOk.value = false
  try {
    const report = await RecordingService.runRegression()
    const steps = Array.isArray(report.steps) ? report.steps.join(' -> ') : ''
    regressionTitle.value = report.message || '回归自测完成'
    regressionOk.value = report.success === true
    regressionResult.value = `产物: ${report.outputPath || '-'}；时长: ${report.durationMs || 0}ms；大小: ${report.fileSizeBytes || 0} bytes；步骤: ${steps}`
    ElMessage.success('回归自测通过')
    await refresh()
  } catch (error) {
    regressionTitle.value = '回归自测失败'
    regressionOk.value = false
    regressionResult.value = String(error)
    ElMessage.error(String(error))
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  unlistenStateChanged = await listen('recording-state-changed', (event) => {
    const payload = event.payload || {}
    state.state = payload.state || state.state
    state.sessionId = payload.sessionId ?? state.sessionId
    state.elapsedMs = Number(payload.elapsedMs || state.elapsedMs || 0)
  })
  unlistenStatsUpdated = await listen('recording-stats-updated', (event) => {
    const payload = event.payload || {}
    state.fps = Number(payload.fps || 0)
    state.videoBitrateKbps = Number(payload.videoBitrateKbps || 0)
    state.audioBitrateKbps = Number(payload.audioBitrateKbps || 0)
    state.droppedVideoFrames = Number(payload.droppedVideoFrames || 0)
  })
  unlistenError = await listen('recording-error', (event) => {
    const payload = event.payload || {}
    if (payload.message) {
      if (payload.code === 'MAX_DURATION_REACHED') {
        ElMessage.warning(String(payload.message))
      } else {
        ElMessage.error(String(payload.message))
      }
    }
  })
  unlistenFinished = await listen('recording-finished', (event) => {
    const payload = event.payload || {}
    if (payload.outputPath) {
      ElMessage.success(`录制文件已生成: ${payload.outputPath}`)
    } else {
      ElMessage.success('录制文件已生成')
    }
  })
  try {
    const settings = await AISettingsService.getSettings()
    maxDurationMinutes.value = Number(settings.recording_max_duration_minutes || 180)
    captureSystemAudio.value = settings.recording_capture_system_audio === true
    captureMicrophone.value = settings.recording_capture_microphone !== false
    captureCursor.value = settings.recording_capture_cursor !== false
  } catch (_error) {
  }
  try {
    const outs = await RecordingService.listSystemOutputs()
    systemOutputs.value = Array.isArray(outs) ? outs : []
    const def = systemOutputs.value.find(it => it.isDefault)
    systemOutputId.value = def ? def.id : (systemOutputs.value[0]?.id ?? null)
  } catch (_e) {
    systemOutputs.value = []
    systemOutputId.value = null
  }
  try {
    const mics = await RecordingService.listAudioDevices()
    microphones.value = Array.isArray(mics) ? mics : []
    const defMic = microphones.value.find(it => it.isDefault)
    microphoneDeviceId.value = defMic ? defMic.id : (microphones.value[0]?.id ?? null)
  } catch (_e) {
    microphones.value = []
    microphoneDeviceId.value = null
  }

  await refresh()
})

onBeforeUnmount(() => {
  if (unlistenStateChanged) {
    unlistenStateChanged()
    unlistenStateChanged = null
  }
  if (unlistenStatsUpdated) {
    unlistenStatsUpdated()
    unlistenStatsUpdated = null
  }
  if (unlistenError) {
    unlistenError()
    unlistenError = null
  }
  if (unlistenFinished) {
    unlistenFinished()
    unlistenFinished = null
  }
})
</script>

<style scoped>
.recording-page {
  padding: 16px;
}

.panel {
  max-width: 680px;
}

.title {
  font-size: 16px;
  font-weight: 600;
}

.mb-12 {
  margin-bottom: 12px;
}

.mb-16 {
  margin-bottom: 16px;
}
</style>
