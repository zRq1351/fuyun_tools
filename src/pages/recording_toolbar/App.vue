<template>
  <el-config-provider :locale="zhCn">
    <div
        :class="{
        'bar-collapsed': isToolbarCollapsed,
        'bar-collapsed-settings-open':
          isToolbarCollapsed && capsuleSettingsVisible,
      }"
        :data-tauri-drag-region="isToolbarCollapsed ? null : ''"
        class="bar"
    >
      <div
          v-if="isToolbarCollapsed"
          :data-state="rawRecordingState"
          class="collapsed-shell"
      >
        <div class="collapsed-shell-row">
          <el-tooltip
              :offset="10"
              :show-after="300"
              content="停止录制"
              effect="dark"
              placement="bottom"
              popper-class="recording-toolbar-tooltip"
          >
            <button
                :disabled="!canStop"
                class="collapsed-stop-btn no-drag"
                type="button"
                @click.stop="stop"
            >
              <span class="collapsed-stop-icon"></span>
            </button>
          </el-tooltip>
          <el-tooltip
              :content="capsuleTooltipContent"
              :offset="10"
              :show-after="300"
              effect="dark"
              placement="bottom"
              popper-class="recording-toolbar-tooltip"
          >
            <div
                :data-state="currentRecordingState"
                class="collapsed-pill"
                @click.stop="toggleRecordingState"
            >
              <span class="collapsed-pill-content">
                <span
                    v-if="currentRecordingState === 'recording'"
                    class="recording-dot"
                ></span>
                <span
                    v-else-if="currentRecordingState === 'paused'"
                    class="recording-pause-square"
                ></span>
                <span
                    v-else-if="currentRecordingState === 'idle'"
                    class="recording-ready-dot"
                ></span>
                <span class="collapsed-pill-text">{{ collapsedDisplayText }}</span>
              </span>
            </div>
          </el-tooltip>
          <el-tooltip
              :offset="10"
              :show-after="300"
              content="设置"
              effect="dark"
              placement="bottom"
              popper-class="recording-toolbar-tooltip"
          >
            <button
                class="collapsed-expand-btn no-drag"
                type="button"
                @click.stop="toggleCapsuleSettings"
            >
              <el-icon class="collapsed-expand-icon">
                <Settings :size="13" :stroke-width="2.2"/>
              </el-icon>
            </button>
          </el-tooltip>
          <el-tooltip
              :offset="10"
              :show-after="300"
              content="关闭"
              effect="dark"
              placement="bottom"
              popper-class="recording-toolbar-tooltip"
          >
            <button
                class="collapsed-close-btn no-drag"
                type="button"
                @click.stop="closeCapsule"
            >
              ×
            </button>
          </el-tooltip>
        </div>
        <div v-if="capsuleSettingsVisible" class="capsule-settings-panel no-drag">
          <div class="toolbar-settings-title">录制设置</div>
          <div class="toolbar-settings-row">
            <span class="toolbar-settings-label">系统音频</span>
            <el-select
                :model-value="captureSystemAudio ? systemOutputId : ''"
                placeholder="选择系统音频设备"
                popper-class="recording-toolbar-select-popper"
                size="small"
                @change="onSystemAudioDeviceChange"
            >
              <el-option label="不捕获系统音频" value=""/>
              <el-option
                  v-for="item in systemOutputs"
                  :key="item.id"
                  :label="item.name"
                  :value="item.id"
              />
            </el-select>
          </div>
          <div class="toolbar-settings-row">
            <span class="toolbar-settings-label">麦克风</span>
            <el-select
                :model-value="captureMicrophone ? microphoneDeviceId : ''"
                placeholder="选择麦克风设备"
                popper-class="recording-toolbar-select-popper"
                size="small"
                @change="onMicrophoneDeviceChange"
            >
              <el-option label="不捕获麦克风" value=""/>
              <el-option
                  v-for="item in microphones"
                  :key="item.id"
                  :label="item.name"
                  :value="item.id"
              />
            </el-select>
          </div>
          <button
              class="toolbar-folder-btn no-drag"
              type="button"
              @click="openRecordingFolder"
          >
            打开录制保存文件夹
          </button>
          <div class="toolbar-settings-switch-row">
            <el-switch
                v-model="captureCursor"
                active-text="捕获鼠标"
                @change="onToolbarSettingChange('recordingCaptureCursor', $event)"
            />
            <el-tooltip
                content="开启后，录制画面会包含当前悬浮工具栏；关闭后，工具栏不会被录进去。"
                effect="dark"
                placement="top"
                popper-class="recording-toolbar-tooltip"
            >
              <el-switch
                  v-model="toolbarContentProtected"
                  active-text="捕获工具栏"
                  @change="onToolbarSettingChange('recordingToolbarContentProtected', $event)"
              />
            </el-tooltip>
          </div>
          <div class="toolbar-settings-row">
            <span class="toolbar-settings-label">默认帧率</span>
            <el-input-number
                :controls="false"
                :max="120"
                :min="1"
                :model-value="fps"
                :step="1"
                size="small"
                @change="onToolbarSettingChange('recordingDefaultFps', $event)"
            />
          </div>
          <div class="toolbar-settings-row">
            <span class="toolbar-settings-label">视频码率 (kbps)</span>
            <el-input-number
                :controls="false"
                :max="50000"
                :min="500"
                :model-value="videoBitrateKbps"
                :step="500"
                size="small"
                @change="onToolbarSettingChange('recordingDefaultVideoBitrateKbps', $event)"
            />
          </div>
          <div class="toolbar-settings-row">
            <span class="toolbar-settings-label">音频码率 (kbps)</span>
            <el-input-number
                :controls="false"
                :max="512"
                :min="32"
                :model-value="audioBitrateKbps"
                :step="16"
                size="small"
                @change="onToolbarSettingChange('recordingDefaultAudioBitrateKbps', $event)"
            />
          </div>
        </div>
      </div>

    </div>
  </el-config-provider>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, reactive, ref, watch} from "vue";
import zhCn from "element-plus/dist/locale/zh-cn";
import {listen} from "@tauri-apps/api/event";
import {getCurrentWindow} from "@tauri-apps/api/window";
import {AISettingsService, RecordingService} from "@/services/ipc.js";
import {ElMessage} from "element-plus";
import {Settings} from "lucide-vue-next";

const loadingAction = ref(null);
const capsuleSettingsVisible = ref(false);
const isToolbarCollapsed = ref(true);
const recordingFeatureEnabled = ref(true);

const captureSystemAudio = ref(false);
const captureMicrophone = ref(false);
const systemOutputId = ref(null);
const microphoneDeviceId = ref(null);
const systemOutputs = ref([]);
const microphones = ref([]);

const fps = ref(30);
const videoBitrateKbps = ref(6000);
const audioBitrateKbps = ref(160);
const captureCursor = ref(true);
const toolbarContentProtected = ref(false);

const state = reactive({state: "idle", sessionId: null, elapsedMs: 0});
let unlistenStateChanged = null;
let unlistenRecordingFinished = null;
let unlistenRecordingError = null;
let unlistenForceCompact = null;
let lastElapsedUiSyncAt = 0;

const formatElapsedText = (ms) => {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
};
const elapsedText = computed(() => formatElapsedText(state.elapsedMs));
const rawRecordingState = computed(() => String(state.state || "idle").toLowerCase());
const currentRecordingState = computed(() => {
  if (!recordingFeatureEnabled.value) return "disabled";
  const normalized = rawRecordingState.value;
  if (
      normalized === "idle" ||
      normalized === "recording" ||
      normalized === "paused" ||
      normalized === "starting" ||
      normalized === "stopping" ||
      normalized === "error" ||
      normalized === "disabled"
  ) {
    return normalized;
  }
  return state.sessionId ? "recording" : "idle";
});
const recordingHintText = computed(() => {
  if (!recordingFeatureEnabled.value) return "录屏已停用";
  if (rawRecordingState.value === "recording") return "正在录屏";
  if (rawRecordingState.value === "paused") return "录屏已暂停";
  if (rawRecordingState.value === "starting") return "录屏启动中";
  if (rawRecordingState.value === "stopping") return "录屏停止中";
  if (rawRecordingState.value === "error") return "录屏异常";
  return "开始录制";
});
const collapsedDisplayText = computed(() => {
  if (rawRecordingState.value === "recording" || rawRecordingState.value === "paused") {
    return elapsedText.value;
  }
  return recordingHintText.value;
});
const capsuleTooltipContent = computed(() => {
  if (!recordingFeatureEnabled.value) return "录屏功能已停用";
  if (rawRecordingState.value === "recording") return "正在录制，点击暂停";
  if (rawRecordingState.value === "paused") return "已暂停，点击恢复录制";
  if (rawRecordingState.value === "idle") return "开始录制，点击开始";
  return recordingHintText.value;
});
const isBusy = computed(() => loadingAction.value !== null);
const canStop = computed(
    () =>
        !isBusy.value &&
        (currentRecordingState.value === "recording" || currentRecordingState.value === "paused"),
);

const syncCapsuleLayout = async () => {
  try {
    isToolbarCollapsed.value = true;
    await RecordingService.resizeToolbar(false, capsuleSettingsVisible.value, true);
  } catch (_e) {
  }
};

const refresh = async () => {
  const data = await RecordingService.getState();
  state.state = data.state || state.state || "idle";
  state.sessionId = data.sessionId || null;
  state.elapsedMs = Number(data.elapsedMs || 0);
};

const toggleRecordingState = async () => {
  if (isBusy.value) return;
  if (!recordingFeatureEnabled.value) {
    try {
      const settings = await AISettingsService.getSettings();
      recordingFeatureEnabled.value = settings.recording_enabled === true;
    } catch (_e) {
    }
    if (!recordingFeatureEnabled.value) return;
  }
  try {
    if (rawRecordingState.value === "idle") {
      loadingAction.value = "start";
      await RecordingService.start({
        captureSystemAudio: captureSystemAudio.value,
        systemAudioDeviceId: systemOutputId.value,
        captureMicrophone: captureMicrophone.value,
        microphoneDeviceId: microphoneDeviceId.value,
        captureCursor: captureCursor.value,
        fps: fps.value,
        videoBitrateKbps: videoBitrateKbps.value,
        audioBitrateKbps: audioBitrateKbps.value,
      });
    } else if (rawRecordingState.value === "recording") {
      loadingAction.value = "pause";
      await RecordingService.pause();
    } else if (rawRecordingState.value === "paused") {
      loadingAction.value = "resume";
      await RecordingService.resume();
    }
    await refresh();
  } catch (e) {
    const msg = String(e || "");
    if (msg.includes("录屏功能已停用")) {
      recordingFeatureEnabled.value = false;
      return;
    }
    ElMessage.error(msg);
  } finally {
    loadingAction.value = null;
  }
};

const stop = async () => {
  loadingAction.value = "stop";
  try {
    await RecordingService.stop(state.sessionId);
    await refresh();
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loadingAction.value = null;
  }
};

const toggleCapsuleSettings = () => {
  capsuleSettingsVisible.value = !capsuleSettingsVisible.value;
};

const closeCapsule = async () => {
  capsuleSettingsVisible.value = false;
  await syncCapsuleLayout();
  try {
    await getCurrentWindow().hide();
  } catch (_e) {
  }
};

const onWindowBlur = () => {
  if (!capsuleSettingsVisible.value) return;
  capsuleSettingsVisible.value = false;
};

const openRecordingFolder = async () => {
  try {
    await RecordingService.openFolder();
  } catch (e) {
    ElMessage.error(`打开录制保存文件夹失败: ${String(e)}`);
  }
};

const onSystemAudioDeviceChange = async (deviceId) => {
  const id = String(deviceId || "");
  captureSystemAudio.value = id.length > 0;
  systemOutputId.value = id.length > 0 ? id : null;
  try {
    await AISettingsService.savePartialSettings({
      recordingCaptureSystemAudio: captureSystemAudio.value,
    });
  } catch (e) {
    ElMessage.error(`保存系统音频设置失败: ${String(e)}`);
  }
};

const onMicrophoneDeviceChange = async (deviceId) => {
  const id = String(deviceId || "");
  captureMicrophone.value = id.length > 0;
  microphoneDeviceId.value = id.length > 0 ? id : null;
  try {
    await AISettingsService.savePartialSettings({
      recordingCaptureMicrophone: captureMicrophone.value,
      recordingMicrophoneDeviceId: microphoneDeviceId.value || "",
    });
  } catch (e) {
    ElMessage.error(`保存麦克风设置失败: ${String(e)}`);
  }
};

const onToolbarSettingChange = async (key, rawValue) => {
  const n = Number(rawValue);
  const patch = {};
  if (key === "recordingDefaultFps") {
    const v = Math.max(1, Math.min(120, Number.isFinite(n) ? n : fps.value));
    fps.value = v;
    patch.recordingDefaultFps = v;
  } else if (key === "recordingDefaultVideoBitrateKbps") {
    const v = Math.max(500, Math.min(50000, Number.isFinite(n) ? n : videoBitrateKbps.value));
    videoBitrateKbps.value = v;
    patch.recordingDefaultVideoBitrateKbps = v;
  } else if (key === "recordingDefaultAudioBitrateKbps") {
    const v = Math.max(32, Math.min(512, Number.isFinite(n) ? n : audioBitrateKbps.value));
    audioBitrateKbps.value = v;
    patch.recordingDefaultAudioBitrateKbps = v;
  } else if (key === "recordingCaptureCursor") {
    const v = rawValue === true;
    captureCursor.value = v;
    patch.recordingCaptureCursor = v;
  } else if (key === "recordingToolbarContentProtected") {
    const v = rawValue === true;
    toolbarContentProtected.value = v;
    patch.recordingToolbarContentProtected = v;
  } else {
    return;
  }
  try {
    await AISettingsService.savePartialSettings(patch);
  } catch (e) {
    ElMessage.error(`保存录制设置失败: ${String(e)}`);
  }
};

onMounted(async () => {
  window.addEventListener("blur", onWindowBlur);
  unlistenStateChanged = await listen("recording-state-changed", (event) => {
    const payload = event.payload || {};
    const nextState = payload.state || state.state;
    const stateChanged = nextState !== state.state;
    state.state = nextState;
    state.sessionId = payload.sessionId ?? state.sessionId;
    const nextElapsedMs = Number(payload.elapsedMs || state.elapsedMs || 0);
    const now = Date.now();
    if (stateChanged || now - lastElapsedUiSyncAt >= 1000) {
      state.elapsedMs = nextElapsedMs;
      lastElapsedUiSyncAt = now;
    }
  });
  unlistenRecordingFinished = await listen("recording-finished", () => {
    state.state = "idle";
    state.sessionId = null;
    capsuleSettingsVisible.value = false;
    void syncCapsuleLayout();
  });
  unlistenRecordingError = await listen("recording-error", () => {
    state.state = "error";
    capsuleSettingsVisible.value = false;
    void syncCapsuleLayout();
  });
  unlistenForceCompact = await listen("recording-toolbar-force-compact", () => {
    capsuleSettingsVisible.value = false;
    void syncCapsuleLayout();
  });
  try {
    const settings = await AISettingsService.getSettings();
    recordingFeatureEnabled.value = settings.recording_enabled === true;
    captureSystemAudio.value = settings.recording_capture_system_audio === true;
    captureMicrophone.value = settings.recording_capture_microphone === true;
    fps.value = Number(settings.recording_default_fps || 30);
    videoBitrateKbps.value = Number(settings.recording_default_video_bitrate_kbps || 6000);
    audioBitrateKbps.value = Number(settings.recording_default_audio_bitrate_kbps || 160);
    captureCursor.value = settings.recording_capture_cursor !== false;
    toolbarContentProtected.value = settings.recording_toolbar_content_protected === true;
    microphoneDeviceId.value = settings.recording_microphone_device_id || null;
  } catch (_e) {
  }
  try {
    const outs = await RecordingService.listSystemOutputs();
    systemOutputs.value = Array.isArray(outs) ? outs : [];
    const def = systemOutputs.value.find((it) => it.isDefault);
    if (!systemOutputId.value) {
      systemOutputId.value = def ? def.id : (systemOutputs.value[0]?.id ?? null);
    }
    if (captureSystemAudio.value && !systemOutputId.value && systemOutputs.value.length > 0) {
      systemOutputId.value = def ? def.id : systemOutputs.value[0].id;
    }
    if (!captureSystemAudio.value) {
      systemOutputId.value = null;
    }
  } catch (_e) {
  }
  try {
    const mics = await RecordingService.listAudioDevices();
    microphones.value = Array.isArray(mics) ? mics : [];
    const def = microphones.value.find((it) => it.isDefault);
    if (!microphoneDeviceId.value && captureMicrophone.value) {
      microphoneDeviceId.value = def ? def.id : (microphones.value[0]?.id ?? null);
    }
    if (!captureMicrophone.value) {
      microphoneDeviceId.value = null;
    }
  } catch (_e) {
  }
  await refresh();
  await syncCapsuleLayout();
});

watch(capsuleSettingsVisible, () => {
  void syncCapsuleLayout();
});

watch(currentRecordingState, (next) => {
  if (next === "idle" || next === "error") {
    capsuleSettingsVisible.value = false;
  }
  void syncCapsuleLayout();
});

onBeforeUnmount(() => {
  window.removeEventListener("blur", onWindowBlur);
  if (unlistenStateChanged) unlistenStateChanged();
  if (unlistenRecordingFinished) unlistenRecordingFinished();
  if (unlistenRecordingError) unlistenRecordingError();
  if (unlistenForceCompact) unlistenForceCompact();
});
</script>

<style>
body {
  margin: 0;
  padding: 0;
  overflow: hidden;
  background: transparent;
}

html,
body,
#app {
  height: 100%;
  background: transparent;
}

.recording-toolbar-select-popper.el-popper {
  --el-bg-color-overlay: #171b24;
  --el-fill-color-light: #252b38;
  --el-border-color-light: rgba(255, 255, 255, 0.16);
  --el-text-color-primary: #e9eefc;
  width: 220px !important;
  max-width: 220px !important;
  min-width: 220px !important;
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
  max-width: 220px;
  overflow: hidden;
}

.recording-toolbar-select-popper .el-select-dropdown__item {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.recording-toolbar-tooltip.el-popper {
  background: rgba(17, 22, 32, 0.92) !important;
  border: 1px solid rgba(255, 255, 255, 0.12) !important;
  color: #e9eefc !important;
  box-shadow: 0 6px 14px rgba(0, 0, 0, 0.32);
}

.recording-toolbar-tooltip.el-popper .el-popper__arrow::before {
  background: rgba(17, 22, 32, 0.92) !important;
  border: 1px solid rgba(255, 255, 255, 0.12) !important;
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
  transition: none !important;
  animation: none !important;
}

.bar.bar-collapsed {
  width: 100%;
  height: 100%;
  padding: 2px 6px;
  gap: 0;
  justify-content: center;
  align-items: center;
  border-radius: 999px;
  background: transparent;
  border: none;
  overflow: hidden;
  box-sizing: border-box;
  cursor: default;
  -webkit-app-region: no-drag;
  transition: none !important;
  animation: none !important;
  clip-path: inset(0 round 999px);
}

.bar.bar-collapsed.bar-collapsed-settings-open {
  justify-content: flex-start;
  align-items: stretch;
  padding: 12px;
  border-radius: 12px;
  background: rgba(17, 22, 32, 0.92);
  border: 1px solid rgba(255, 255, 255, 0.12);
  clip-path: none;
}

.collapsed-shell {
  display: flex;
  align-items: center;
  gap: 5px;
  width: 100%;
  height: 100%;
  flex-direction: column;
  justify-content: flex-start;
}

.collapsed-shell-row {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 6px;
}

.collapsed-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 1;
  width: auto;
  min-width: 0;
  height: calc(100% - 1px);
  padding: 0 8px;
  box-sizing: border-box;
  border-radius: 999px;
  background: #202937;
  border: 1px solid #344055;
  user-select: none;
  cursor: pointer;
  transition: none !important;
  animation: none !important;
  background-clip: padding-box;
  clip-path: inset(0 round 999px);
}

.collapsed-pill:hover {
  background: #273345;
}

.collapsed-pill[data-state="disabled"] {
  background: #d64242;
  border-color: #f07979;
}

.collapsed-pill[data-state="disabled"]:hover {
  background: #c73939;
}

.collapsed-pill[data-state="disabled"] .collapsed-pill-content {
  color: #ffffff;
}

.recording-dot {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 13px;
  height: 13px;
  margin-right: 0;
  flex-shrink: 0;
  vertical-align: middle;
  box-shadow: none;
  filter: none;
}

.recording-dot::before {
  content: "";
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: #ff4d4d;
  animation: recording-dot-pulse 1.05s ease-in-out infinite;
}

@keyframes recording-dot-pulse {
  0% {
    width: 9px;
    height: 9px;
  }
  50% {
    width: 13px;
    height: 13px;
  }
  100% {
    width: 9px;
    height: 9px;
  }
}

.recording-pause-square {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 10px;
  height: 10px;
  margin-right: 0;
  border-radius: 2px;
  background: #e8bf4f;
  box-shadow: none;
  flex-shrink: 0;
  vertical-align: middle;
  filter: none;
}

.recording-ready-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #1fb76a;
  flex-shrink: 0;
}

.bar.bar-collapsed .collapsed-pill {
  flex: 1;
  width: auto;
  min-width: 0;
  height: 22px;
}

.capsule-settings-panel {
  width: 100%;
  margin-top: 12px;
  border-top: 1px solid rgba(255, 255, 255, 0.1);
  padding-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-height: calc(100% - 44px);
  overflow-y: auto;
  padding-right: 4px;
  box-sizing: border-box;
}

.collapsed-stop-btn,
.collapsed-expand-btn,
.collapsed-close-btn {
  width: 20px;
  height: 20px;
  border: 1px solid #344055;
  background: #202937;
  color: #dce7f8;
  border-radius: 999px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  cursor: pointer;
  flex-shrink: 0;
}

.collapsed-stop-btn {
  border-color: #845364;
  background: #2f2229;
}

.collapsed-stop-btn:hover:not(:disabled) {
  background: #3a2932;
}

.collapsed-stop-btn:active:not(:disabled) {
  background: #261b21;
}

.collapsed-stop-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.collapsed-stop-icon {
  width: 8px;
  height: 8px;
  border-radius: 2px;
  background: #f08b8b;
}

.collapsed-expand-btn {
  border-color: #3d4f6b;
  background: #232f41;
  color: #d3e4ff;
}

.collapsed-expand-btn:hover {
  background: #2b3950;
}

.collapsed-expand-btn:active {
  background: #1e2a3b;
}

.collapsed-expand-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  line-height: 1;
}

.collapsed-close-btn {
  border-color: #475265;
  background: #2a3445;
  color: #e3e9f3;
  font-size: 14px;
  line-height: 1;
}

.collapsed-close-btn:hover {
  background: #333f53;
}

.collapsed-close-btn:active {
  background: #232c3b;
}

.collapsed-pill-content {
  display: inline-flex;
  flex-direction: row;
  align-items: center;
  justify-content: center;
  gap: 6px;
  white-space: nowrap;
  overflow: visible;
  font-size: 12.5px;
  color: #dfebfc;
  font-weight: 600;
  letter-spacing: 0.1px;
  text-shadow: none;
  line-height: 1;
  width: 100%;
  height: 100%;
  padding: 0;
  text-align: center;
  box-sizing: border-box;
  -webkit-font-smoothing: antialiased;
}

.collapsed-pill-text {
  display: inline-block;
  vertical-align: middle;
  font-variant-numeric: tabular-nums;
  letter-spacing: 0;
}

.bar:not(.bar-collapsed) .collapsed-pill {
  display: none;
}

.toolbar-settings-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 2px 2px 0;
}

.toolbar-settings-title {
  font-size: 14px;
  font-weight: 700;
  color: #e9eefc;
  margin-bottom: 2px;
}

.toolbar-folder-btn {
  height: 30px;
  border: 1px solid #3a4a63;
  background: #233144;
  color: #e6f0ff;
  border-radius: 8px;
  padding: 0 12px;
  font-size: 12px;
  line-height: 30px;
  text-align: center;
  cursor: pointer;
}

.toolbar-folder-btn:hover {
  background: #2b3d54;
}

.toolbar-folder-btn:active {
  background: #1f2c3d;
}

.toolbar-settings-row {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 12px;
  min-height: 34px;
}

.toolbar-settings-label {
  font-size: 13px;
  color: #dce5f8;
  white-space: nowrap;
  width: 118px;
  flex: 0 0 118px;
}

.toolbar-settings-switch-row {
  display: flex;
  justify-content: flex-start;
  min-height: 34px;
  align-items: center;
  gap: 12px;
  flex-wrap: nowrap;
}

.toolbar-settings-switch-row > * {
  flex-shrink: 0;
}

.toolbar-settings-row :deep(.el-input-number) {
  width: 80px;
  flex: 0 0 80px;
}

.toolbar-settings-row :deep(.el-select) {
  width: 100px;
  flex: 0 0 100px;
}

.toolbar-settings-panel :deep(.el-input-number .el-input__inner) {
  text-align: left;
}

.no-drag {
  -webkit-app-region: no-drag;
}

.no-drag :deep(.el-select),
.no-drag :deep(.el-switch) {
  cursor: default;
}
</style>
