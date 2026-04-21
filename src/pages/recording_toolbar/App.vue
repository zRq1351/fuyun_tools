<template>
  <div
        :class="{
        'bar-collapsed-settings-open': capsuleSettingsVisible,
      }"
        class="bar bar-collapsed"
    >
      <div
          :data-state="rawRecordingState"
          class="collapsed-shell"
      >
        <div class="collapsed-shell-row">
          <el-tooltip
              :offset="10"
              :show-after="300"
              :disabled="!isSettingsPanelOpen"
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
              :disabled="!isSettingsPanelOpen"
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
              :disabled="!isSettingsPanelOpen"
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
              :content="micToggleTooltip"
              :disabled="!isSettingsPanelOpen"
              :offset="10"
              :show-after="300"
              effect="dark"
              placement="bottom"
              popper-class="recording-toolbar-tooltip"
          >
            <button
                :class="['collapsed-mic-toggle-btn', 'no-drag', { 'is-muted': isMicMuted || !canToggleMic, 'is-active': !isMicMuted && canToggleMic, 'is-disabled': !canToggleMic || !microphoneDeviceId }]"
                :disabled="!canToggleMic || !microphoneDeviceId"
                type="button"
                @click.stop="toggleMicState"
            >
              <el-icon class="collapsed-mic-icon">
                <component :is="isMicMuted || !canToggleMic ? MicOff : Mic" :size="13" :stroke-width="2.2"/>
              </el-icon>
            </button>
          </el-tooltip>
          <el-tooltip
              :offset="10"
              :show-after="300"
              :disabled="!isSettingsPanelOpen"
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
        <div class="capsule-settings-panel-wrapper" :class="{ 'is-open': capsuleSettingsVisible }">
          <div class="capsule-settings-panel no-drag">
            <div v-if="inlineNotice" :class="['toolbar-inline-notice', `is-${inlineNoticeType}`]"
                 title="点击关闭提示" @click="clearInlineNotice">
              {{ inlineNotice }}
              <span class="inline-notice-close">×</span>
            </div>
          <div class="toolbar-settings-title-row">
            <div class="toolbar-settings-title">录制设置</div>
            <span v-if="recordTargetType === 'region'" class="target-region-meta">
              {{ regionCoordinateText }}
            </span>
          </div>
          <div class="toolbar-settings-row">
            <div class="target-mode-buttons">
              <button
                  :class="['target-mode-btn', { active: recordTargetType === 'screen' }]"
                  :disabled="!canEditRecordingConfig"
                  @click="onTargetModeClick('screen')"
              >
                全屏
              </button>
              <button
                  :class="['target-mode-btn', { active: recordTargetType === 'window' }]"
                  :disabled="!canEditRecordingConfig"
                  @click="onTargetModeClick('window')"
              >
                窗口
              </button>
              <button
                  :class="['target-mode-btn', { active: recordTargetType === 'region' }]"
                  :disabled="!canEditRecordingConfig"
                  @click="onTargetModeClick('region')"
              >
                区域
              </button>
            </div>
          </div>
          <div v-if="recordTargetType === 'window'" class="toolbar-settings-row">
            <span class="toolbar-settings-label">目标窗口</span>
            <el-select
                v-model="recordTargetWindowId"
                :disabled="!canEditRecordingConfig"
                filterable
                placeholder="选择窗口"
                popper-class="recording-toolbar-select-popper"
                size="small"
                @visible-change="onTargetWindowDropdownVisibleChange"
            >
              <el-option
                  v-for="item in recordableWindows"
                  :key="item.hwnd || item.title"
                  :label="formatTargetWindowLabel(item)"
                  :value="item.hwnd || item.title"
              />
            </el-select>
          </div>
          <div class="toolbar-settings-row">
            <span class="toolbar-settings-label">系统音频</span>
            <el-select
                :model-value="captureSystemAudio ? systemOutputId : ''"
                placeholder="选择系统音频设备"
                popper-class="recording-toolbar-select-popper"
                size="small"
                :disabled="!canEditAudioConfig"
                @visible-change="onSystemAudioDropdownVisibleChange"
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
          <div v-if="captureSystemAudio" class="toolbar-settings-row">
            <span class="toolbar-settings-label">应用音频</span>
            <el-select
                v-model="systemAudioProcessIds"
                :disabled="!canEditRecordingConfig"
                collapse-tags
                collapse-tags-tooltip
                filterable
                multiple
                placeholder="可选：按应用录音（多选）"
                popper-class="recording-toolbar-select-popper recording-toolbar-audio-process-popper"
                size="small"
                @visible-change="onAudioProcessDropdownVisibleChange"
            >
              <el-option
                  v-for="item in audioProcesses"
                  :key="item.pid"
                  :label="`${item.name} (PID ${item.pid})`"
                  :value="item.pid"
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
                :disabled="!canEditAudioConfig"
                @visible-change="onMicrophoneDropdownVisibleChange"
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
              :disabled="isOpeningFolder"
              @click="openRecordingFolder"
          >
            打开录制保存文件夹
          </button>
          <div class="toolbar-settings-switch-row">
            <el-switch
                v-model="captureCursor"
                active-text="捕获鼠标"
                :disabled="!canEditRecordingConfig"
                @change="onToolbarSettingChange('recordingCaptureCursor', $event)"
            />
            <el-tooltip
                content="开启后，录制画面会包含当前悬浮工具栏；关闭后，工具栏会尝试从录制画面中隐藏。"
                effect="dark"
                placement="top"
                popper-class="recording-toolbar-tooltip"
            >
              <el-switch
                  v-model="captureToolbar"
                  active-text="捕获工具栏"
                  :disabled="!canEditRecordingConfig"
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
                :disabled="!canEditRecordingConfig"
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
                :disabled="!canEditRecordingConfig"
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
                :disabled="!canEditRecordingConfig"
                @change="onToolbarSettingChange('recordingDefaultAudioBitrateKbps', $event)"
            />
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import {computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch} from "vue";
import {provideGlobalConfig} from "element-plus";
import zhCn from "element-plus/dist/locale/zh-cn";
import {listen} from "@tauri-apps/api/event";
import {invoke} from "@tauri-apps/api/core";
import {getCurrentWindow} from "@tauri-apps/api/window";
import {AISettingsService, RecordingService} from "@/services/ipc.js";
import {Mic, MicOff, Settings} from "lucide-vue-next";

provideGlobalConfig({locale: zhCn});

const loadingAction = ref(null);
const capsuleSettingsVisible = ref(false);
const recordingFeatureEnabled = ref(true);

const captureSystemAudio = ref(false);
const captureMicrophone = ref(false);
const isMicMuted = ref(false); // 麦克风临时静音状态
const systemOutputId = ref(null);
const microphoneDeviceId = ref(null);
const systemOutputs = ref([]);
const microphones = ref([]);
const audioProcesses = ref([]);
const systemAudioProcessIds = ref([]);
const recordTargetType = ref("screen");
const recordTargetWindowId = ref("");
const recordableWindows = ref([]);
const recordRegionX = ref(0);
const recordRegionY = ref(0);
const recordRegionWidth = ref(1280);
const recordRegionHeight = ref(720);
const regionSelectionReady = ref(false);
let isPickingRegion = false;
const inlineNotice = ref("");
const inlineNoticeType = ref("error");
let inlineNoticeTimer = null;

const fps = ref(30);
const videoBitrateKbps = ref(6000);
const audioBitrateKbps = ref(160);
const captureCursor = ref(true);
const captureToolbar = ref(true);

const state = reactive({state: "idle", sessionId: null, elapsedMs: 0});
let unlistenStateChanged = null;
let unlistenRecordingFinished = null;
let unlistenRecordingError = null;
let unlistenForceCompact = null;
let unlistenRecordingRegionSelected = null;
let unlistenAudioMerging = null;  // ✅ 新增：监听音频合并事件
let unlistenMicToggled = null;  // ✅ 新增：监听麦克风切换事件
let unlistenMicKeyPressed = null;  // ✅ 新增：监听麦克风按键按下事件
let unlistenMicKeyReleased = null;  // ✅ 新增：监听麦克风按键释放事件
let keepSettingsOpenUntilTs = 0;
let autoCollapseAfterStartPending = false;
let lastElapsedUiSyncAt = 0;
const isOpeningFolder = ref(false);

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
const canEditRecordingConfig = computed(() => rawRecordingState.value === "idle" || rawRecordingState.value === "error");
const canEditAudioConfig = computed(() => {
  const s = rawRecordingState.value;
  return s === "idle" || s === "error" || s === "recording" || s === "paused";
});
const canStop = computed(
    () =>
        !isBusy.value &&
        (currentRecordingState.value === "recording" || currentRecordingState.value === "paused"),
);
const canToggleMic = computed(() => {
  const s = rawRecordingState.value;
  return microphoneDeviceId.value && (s === "recording" || s === "paused");
});
const micToggleTooltip = computed(() => {
  if (!microphoneDeviceId.value) return "请先在设置中选择麦克风设备";
  if (!canToggleMic.value) return "录制过程中才能切换麦克风状态";
  return isMicMuted.value ? "点击开启麦克风（Ctrl+Space）" : "点击关闭麦克风（Ctrl+Space）";
});

const measureCapsuleContentHeight = () => {
  const panelEl = document.querySelector(".capsule-settings-panel");
  const shellRowEl = document.querySelector(".collapsed-shell-row");
  if (!panelEl || !shellRowEl) return 730; // fallback
  
  // .bar padding and border when open: 12px padding + 1px border top/bottom = 26px
  const barPadding = 26;
  // shell row height
  const shellH = shellRowEl.scrollHeight || 20;
  // wrapper margin-top (12) + padding-top (12) = 24px
  const wrapperGap = 24;
  // wrapper border
  const wrapperBorder = 1;
  // panel actual height
  const panelH = panelEl.scrollHeight || 0;
  
  return Math.ceil(barPadding + shellH + wrapperGap + wrapperBorder + panelH + 4);
};

const syncCapsuleLayout = async () => {
  try {
    if (capsuleSettingsVisible.value) {
      await nextTick(); // Ensure any internal v-if (like inlineNotice) has rendered
      const targetHeight = measureCapsuleContentHeight();
      // Opening: resize Tauri window first so the CSS animation has space to play
      await RecordingService.resizeToolbar(false, true, true, "capsule", true, targetHeight, null);
    } else {
      // Closing: wait for CSS animation to finish before shrinking Tauri window
      await new Promise((resolve) => setTimeout(resolve, 400));
      // Double check it's still closed after the delay
      if (!capsuleSettingsVisible.value) {
        await RecordingService.resizeToolbar(false, false, true, "capsule", true, null, null);
      }
    }
  } catch (_e) {
  }
};


const refresh = async () => {
  const data = await RecordingService.getState();
  state.state = data.state || state.state || "idle";
  state.sessionId = data.sessionId || null;
  state.elapsedMs = Number(data.elapsedMs || 0);
};

const showInlineNotice = (message, type = "error") => {
  inlineNotice.value = String(message || "");
  inlineNoticeType.value = type;
  if (inlineNoticeTimer) {
    clearTimeout(inlineNoticeTimer);
    inlineNoticeTimer = null;
  }
  
};

const clearInlineNotice = () => {
  inlineNotice.value = "";
  if (inlineNoticeTimer) {
    clearTimeout(inlineNoticeTimer);
    inlineNoticeTimer = null;
  }
};

const showBackendErrorInSettings = async (message) => {
  const text = String(message || "录屏异常");
  keepSettingsOpenUntilTs = Date.now() + 3000;
  capsuleSettingsVisible.value = true;
  
  if (inlineNotice.value && inlineNoticeType.value === "error" && inlineNotice.value !== text) {
    if (!inlineNotice.value.includes(text)) {
      showInlineNotice(`${inlineNotice.value} | 连锁异常: ${text}`, "error");
    }
  } else {
    showInlineNotice(text, "error");
  }
  void syncCapsuleLayout();
  try {
    const win = getCurrentWindow();
    if (await win.isVisible() === false) {
      await win.show();
      await win.setFocus();
    }
  } catch (e) {
    console.error("唤醒控制台窗口失败:", e);
  }
};

const pickRecordingRegion = async () => {
  if (isPickingRegion) return;
  isPickingRegion = true;
  try {
    await invoke("open_screenshot_editor", {mode: "recording_region"});
  } catch (e) {
    showInlineNotice(`打开区域框选失败: ${String(e)}`, "error");
  } finally {
    window.setTimeout(() => {
      isPickingRegion = false;
    }, 200);
  }
};

const onTargetModeClick = (mode) => {
  if (!canEditRecordingConfig.value) return;
  const prevMode = recordTargetType.value;
  recordTargetType.value = mode;
  if (prevMode !== mode) {
    clearInlineNotice();
  }
  if (mode === "region") {
    void pickRecordingRegion();
  } else if (mode === "window") {
    void refreshRecordableWindows();
  }
};

const regionCoordinateText = computed(() => {
  if (!regionSelectionReady.value) return "未选择";
  const x1 = Math.round(recordRegionX.value);
  const y1 = Math.round(recordRegionY.value);
  const x2 = Math.round(recordRegionX.value + recordRegionWidth.value);
  const y2 = Math.round(recordRegionY.value + recordRegionHeight.value);
  return `左上(${x1}, ${y1}) 右下(${x2}, ${y2})`;
});

const formatTargetWindowLabel = (item) => {
  const title = String(item?.title || "").trim();
  const processNameRaw = String(item?.processName || item?.process_name || "").trim();
  const processName = processNameRaw.replace(/\.exe$/i, "");
  if (!title) return processName || "未知窗口";
  if (!processName) return title;
  return `${processName} - ${title}`;
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
    const prevRawState = rawRecordingState.value;
    if (rawRecordingState.value === "idle" || rawRecordingState.value === "error") {
      loadingAction.value = "start";
      autoCollapseAfterStartPending = true;
      if (recordTargetType.value === "window" && !recordTargetWindowId.value) {
        showInlineNotice("请先选择录制窗口", "warning");
        return;
      }
      if (recordTargetType.value === "region" && (recordRegionWidth.value <= 0 || recordRegionHeight.value <= 0)) {
        showInlineNotice("录制区域宽高必须大于 0", "warning");
        return;
      }
      const targetId = recordTargetType.value === "window"
          ? recordTargetWindowId.value
          : recordTargetType.value === "region"
              ? `${Math.round(recordRegionX.value)},${Math.round(recordRegionY.value)},${Math.max(1, Math.round(recordRegionWidth.value))},${Math.max(1, Math.round(recordRegionHeight.value))}`
              : "";
      const selectedWindow =
          recordTargetType.value === "window"
              ? recordableWindows.value.find((w) => (w.hwnd || w.title) === recordTargetWindowId.value) || null
              : null;
      
      isMicMuted.value = true;
      await RecordingService.start({
        targetType: recordTargetType.value,
        targetId,
        targetX: selectedWindow ? Number(selectedWindow.x || 0) : null,
        targetY: selectedWindow ? Number(selectedWindow.y || 0) : null,
        targetWidth: selectedWindow ? Number(selectedWindow.width || 0) : null,
        targetHeight: selectedWindow ? Number(selectedWindow.height || 0) : null,
        captureSystemAudio: captureSystemAudio.value,
        systemAudioDeviceId: systemOutputId.value,
        systemAudioProcessIds: captureSystemAudio.value ? systemAudioProcessIds.value : [],
        captureMicrophone: false, 
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
    if ((prevRawState === "idle" || prevRawState === "error") && currentRecordingState.value === "recording") {
      capsuleSettingsVisible.value = false;
      void syncCapsuleLayout();
      autoCollapseAfterStartPending = false;
    }
  } catch (e) {
    autoCollapseAfterStartPending = false;
    const msg = String(e || "");
    showBackendErrorInSettings(msg);
  } finally {
    loadingAction.value = null;
  }
};

const stop = async () => {
  loadingAction.value = "stop";
  try {
    await RecordingService.stop(state.sessionId);
    await refresh();
    clearInlineNotice();
  } catch (e) {
    showBackendErrorInSettings(String(e));
  } finally {
    loadingAction.value = null;
  }
};

const toggleCapsuleSettings = () => {
  capsuleSettingsVisible.value = !capsuleSettingsVisible.value;
  if (capsuleSettingsVisible.value) {
    void refreshAllDropdownOptions();
  }
};

const closeCapsule = async () => {
  capsuleSettingsVisible.value = false;
  try {
    await getCurrentWindow().hide();
  } catch (_e) {
  }
};

const toggleMicState = async () => {
  if (!canToggleMic.value || isBusy.value) return;
  try {
    const newMutedState = !isMicMuted.value;
    await RecordingService.updateAudioCapture({
      captureSystemAudio: captureSystemAudio.value,
      systemAudioDeviceId: systemOutputId.value || "",
      captureMicrophone: !newMutedState, 
      microphoneDeviceId: microphoneDeviceId.value || "",
    });
    isMicMuted.value = newMutedState;
    showInlineNotice(newMutedState ? "麦克风已临时关闭" : "麦克风已重新开启", "warning");
  } catch (e) {
    showBackendErrorInSettings(`切换麦克风状态失败: ${String(e)}`);
  }
};

const onWindowBlur = () => {
  if (!capsuleSettingsVisible.value) return;
  if (Date.now() < keepSettingsOpenUntilTs) return;
  
  if (inlineNotice.value) return;
  capsuleSettingsVisible.value = false;
};

const onWindowViewportChanged = () => {
  if (!capsuleSettingsVisible.value) return;
  void syncCapsuleLayout();
};

const openRecordingFolder = async () => {
  if (isOpeningFolder.value) return;
  isOpeningFolder.value = true;
  try {
    await RecordingService.openFolder();
  } catch (e) {
    showInlineNotice(`打开录制保存文件夹失败: ${String(e)}`, "error");
  } finally {
    window.setTimeout(() => {
      isOpeningFolder.value = false;
    }, 800);
  }
};

const onSystemAudioDeviceChange = async (deviceId) => {
  if (!canEditAudioConfig.value) return;
  const prevCapture = captureSystemAudio.value;
  const prevId = systemOutputId.value;
  const id = String(deviceId || "");
  const nextCapture = id.length > 0;
  const nextId = nextCapture ? id : null;
  captureSystemAudio.value = id.length > 0;
  systemOutputId.value = id.length > 0 ? id : null;
  if (rawRecordingState.value === "recording" || rawRecordingState.value === "paused") {
    try {
      if (prevCapture && nextCapture && prevId && nextId && prevId !== nextId) {
        
        await RecordingService.updateAudioCapture({
          captureSystemAudio: false,
          systemAudioDeviceId: prevId || "",
        });
        await RecordingService.updateAudioCapture({
          captureSystemAudio: true,
          systemAudioDeviceId: nextId || "",
        });
      } else {
        await RecordingService.updateAudioCapture({
          captureSystemAudio: captureSystemAudio.value,
          systemAudioDeviceId: systemOutputId.value || "",
        });
      }
    } catch (e) {
      captureSystemAudio.value = prevCapture;
      systemOutputId.value = prevId;
      showBackendErrorInSettings(String(e));
      return;
    }
  }
  try {
    await AISettingsService.savePartialSettings({
      recordingCaptureSystemAudio: captureSystemAudio.value,
    });
  } catch (e) {
    showInlineNotice(`保存系统音频设置失败: ${String(e)}`, "error");
  }
};

const onMicrophoneDeviceChange = async (deviceId) => {
  if (!canEditAudioConfig.value) return;
  const prevCapture = captureMicrophone.value;
  const prevId = microphoneDeviceId.value;
  const id = String(deviceId || "");
  const nextCapture = id.length > 0;
  const nextId = nextCapture ? id : null;
  captureMicrophone.value = id.length > 0;
  microphoneDeviceId.value = id.length > 0 ? id : null;
  
  isMicMuted.value = false;
  if (rawRecordingState.value === "recording" || rawRecordingState.value === "paused") {
    try {
      if (prevCapture && nextCapture && prevId && nextId && prevId !== nextId) {
        
        await RecordingService.updateAudioCapture({
          captureMicrophone: false,
          microphoneDeviceId: prevId || "",
        });
        await RecordingService.updateAudioCapture({
          captureMicrophone: true,
          microphoneDeviceId: nextId || "",
        });
      } else {
        await RecordingService.updateAudioCapture({
          captureMicrophone: captureMicrophone.value,
          microphoneDeviceId: microphoneDeviceId.value || "",
        });
      }
    } catch (e) {
      captureMicrophone.value = prevCapture;
      microphoneDeviceId.value = prevId;
      showBackendErrorInSettings(String(e));
      return;
    }
  }
  try {
    await AISettingsService.savePartialSettings({
      recordingCaptureMicrophone: captureMicrophone.value,
      recordingMicrophoneDeviceId: microphoneDeviceId.value || "",
    });
  } catch (e) {
    showInlineNotice(`保存麦克风设置失败: ${String(e)}`, "error");
  }
};

const refreshSystemOutputDevices = async () => {
  const outs = await RecordingService.listSystemOutputs();
  systemOutputs.value = Array.isArray(outs) ? outs : [];
  const def = systemOutputs.value.find((it) => it.isDefault);
  if (captureSystemAudio.value) {
    const exists = systemOutputs.value.some((it) => it.id === systemOutputId.value);
    if (!exists) {
      systemOutputId.value = def ? def.id : (systemOutputs.value[0]?.id ?? null);
    }
  } else {
    systemOutputId.value = null;
  }
};

const refreshMicrophoneDevices = async () => {
  const mics = await RecordingService.listAudioDevices();
  microphones.value = Array.isArray(mics) ? mics : [];
  const def = microphones.value.find((it) => it.isDefault);
  if (captureMicrophone.value) {
    const exists = microphones.value.some((it) => it.id === microphoneDeviceId.value);
    if (!exists) {
      microphoneDeviceId.value = def ? def.id : (microphones.value[0]?.id ?? null);
    }
  } else {
    microphoneDeviceId.value = null;
  }
};

const refreshAudioProcesses = async () => {
  const procs = await RecordingService.listAudioProcesses();
  audioProcesses.value = Array.isArray(procs) ? procs : [];
  const pidSet = new Set(audioProcesses.value.map((p) => Number(p.pid || 0)));
  systemAudioProcessIds.value = systemAudioProcessIds.value
      .map((v) => Number(v))
      .filter((v) => pidSet.has(v));
};

const refreshRecordableWindows = async () => {
  const windowRes = await RecordingService.listWindows();
  if (!windowRes?.success || !Array.isArray(windowRes.windows)) {
    return;
  }
  const nextWindows = windowRes.windows.filter((w) => String(w?.title || "").trim().length > 0);
  recordableWindows.value = nextWindows;
  if (nextWindows.length === 0) {
    recordTargetWindowId.value = "";
    return;
  }
  const exists = nextWindows.some(
      (w) => (w.hwnd || w.title) === recordTargetWindowId.value,
  );
  if (!exists) {
    recordTargetWindowId.value = nextWindows[0].hwnd || nextWindows[0].title;
  }
};

const refreshAllDropdownOptions = async () => {
  await Promise.allSettled([
    refreshRecordableWindows(),
    refreshSystemOutputDevices(),
    refreshMicrophoneDevices(),
    refreshAudioProcesses(),
  ]);
};

const onSystemAudioDropdownVisibleChange = async (visible) => {
  if (!visible) return;
  try {
    await refreshSystemOutputDevices();
  } catch (_e) {
  }
};

const onMicrophoneDropdownVisibleChange = async (visible) => {
  if (!visible) return;
  try {
    await refreshMicrophoneDevices();
  } catch (_e) {
  }
};

const onAudioProcessDropdownVisibleChange = async (visible) => {
  if (!visible) return;
  try {
    await refreshAudioProcesses();
  } catch (_e) {
  }
};

const onTargetWindowDropdownVisibleChange = async (visible) => {
  if (!visible) return;
  try {
    await refreshRecordableWindows();
  } catch (_e) {
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
    captureToolbar.value = v;
    
    patch.recordingToolbarContentProtected = !v;
  } else {
    return;
  }
  try {
    await AISettingsService.savePartialSettings(patch);
  } catch (e) {
    showInlineNotice(`保存录制设置失败: ${String(e)}`, "error");
  }
};

onMounted(async () => {
  window.addEventListener("blur", onWindowBlur);
  window.addEventListener("resize", onWindowViewportChanged);
  unlistenStateChanged = await listen("recording-state-changed", (event) => {
    const payload = event.payload || {};
    const incomingState = String(payload.state || state.state || "idle");
    const nextState = incomingState === "error" ? "idle" : incomingState;
    const stateChanged = nextState !== state.state;
    state.state = nextState;
    state.sessionId = nextState === "idle" ? null : (payload.sessionId ?? state.sessionId);
    const nextElapsedMs = Number(payload.elapsedMs || state.elapsedMs || 0);
    const now = Date.now();
    if (stateChanged || now - lastElapsedUiSyncAt >= 1000) {
      state.elapsedMs = nextElapsedMs;
      lastElapsedUiSyncAt = now;
    }
    
    if (nextState === "idle" || nextState === "error") {
      isMicMuted.value = false;
    }
    if (nextState === "recording" && capsuleSettingsVisible.value && autoCollapseAfterStartPending) {
      capsuleSettingsVisible.value = false;
      void syncCapsuleLayout();
      autoCollapseAfterStartPending = false;
    }
    if (nextState !== "starting" && nextState !== "recording") {
      autoCollapseAfterStartPending = false;
    }
  });
  unlistenRecordingFinished = await listen("recording-finished", async () => {
    state.state = "idle";
    state.sessionId = null;

    
    if (!inlineNotice.value) {
      clearInlineNotice();
      capsuleSettingsVisible.value = false;
      void syncCapsuleLayout();
    }

    try {
      const win = getCurrentWindow();
      if (await win.isVisible() === false) {
        await win.show();
        await win.setFocus();
      }
    } catch (e) {
      console.error("唤醒控制台窗口失败:", e);
    }
  });
  unlistenRecordingError = await listen("recording-error", (event) => {
    const payload = event.payload || {};
    const message = String(payload.message || "录屏异常");
    const code = String(payload.code || "");
    state.state = "idle";
    state.sessionId = null;
    showBackendErrorInSettings(code ? `${code}: ${message}` : message);
  });
  unlistenForceCompact = await listen("recording-toolbar-force-compact", () => {
    if (Date.now() < keepSettingsOpenUntilTs) {
      return;
    }
    
    if (inlineNotice.value) return;
    capsuleSettingsVisible.value = false;
    void syncCapsuleLayout();
  });
  unlistenRecordingRegionSelected = await listen("recording-region-selected", (event) => {
    const payload = event.payload || {};
    const x = Number(payload.x || 0);
    const y = Number(payload.y || 0);
    const width = Math.max(1, Number(payload.width || 1));
    const height = Math.max(1, Number(payload.height || 1));
    recordTargetType.value = "region";
    recordRegionX.value = x;
    recordRegionY.value = y;
    recordRegionWidth.value = width;
    recordRegionHeight.value = height;
    regionSelectionReady.value = true;
    keepSettingsOpenUntilTs = Date.now() + 1500;
    capsuleSettingsVisible.value = true;
    void syncCapsuleLayout();
  });

  
  unlistenAudioMerging = await listen("recording-audio-merging", (event) => {
    const payload = event.payload || {};
    const status = String(payload.status || "");
    const message = String(payload.message || "");
    const progress = payload.progress;

    if (status === "started") {
      
      showInlineNotice(message || "正在后台合并音频...", "warning");
    } else if (status === "completed") {
      
      clearInlineNotice();
    } else if (status === "failed") {
      
      showInlineNotice(message || "音频合并失败，视频文件已保存", "error");
    }
  });

  
  unlistenMicToggled = await listen("recording-mic-toggled", (event) => {
    const payload = event.payload || {};
    isMicMuted.value = !payload.enabled;
    const action = payload.enabled ? "开启" : "关闭";
    showInlineNotice(`麦克风已${action}（快捷键）`, "warning");
  });

  
  unlistenMicKeyPressed = await listen("recording-mic-key-pressed", () => {
    
    isMicMuted.value = false;
  });

  
  unlistenMicKeyReleased = await listen("recording-mic-key-released", () => {
    
    isMicMuted.value = true;
  });
  try {
    await refreshRecordableWindows();
  } catch (_e) {
  }
  try {
    const settings = await AISettingsService.getSettings();
    recordingFeatureEnabled.value = settings.recording_enabled === true;
    captureSystemAudio.value = settings.recording_capture_system_audio === true;
    captureMicrophone.value = settings.recording_capture_microphone === true;
    fps.value = Number(settings.recording_default_fps || 30);
    videoBitrateKbps.value = Number(settings.recording_default_video_bitrate_kbps || 6000);
    audioBitrateKbps.value = Number(settings.recording_default_audio_bitrate_kbps || 160);
    captureCursor.value = settings.recording_capture_cursor !== false;
    captureToolbar.value = settings.recording_toolbar_content_protected !== true;
    microphoneDeviceId.value = settings.recording_microphone_device_id || null;
  } catch (_e) {
  }
  try {
    await refreshSystemOutputDevices();
  } catch (_e) {
  }
  try {
    await refreshMicrophoneDevices();
  } catch (_e) {
  }
  try {
    await refreshAudioProcesses();
  } catch (_e) {
  }
  await refresh();
  await syncCapsuleLayout();
});

watch(capsuleSettingsVisible, () => {
  if (capsuleSettingsVisible.value) {
    void refreshAllDropdownOptions();
  }
  void syncCapsuleLayout();
});

watch(
    () => [recordTargetType.value, captureSystemAudio.value, inlineNotice.value],
    () => {
      if (!capsuleSettingsVisible.value) return;
      void syncCapsuleLayout();
    },
);

watch(currentRecordingState, (next) => {
  void syncCapsuleLayout();
});

onBeforeUnmount(() => {
  if (inlineNoticeTimer) {
    clearTimeout(inlineNoticeTimer);
    inlineNoticeTimer = null;
  }
  window.removeEventListener("blur", onWindowBlur);
  window.removeEventListener("resize", onWindowViewportChanged);
  if (unlistenStateChanged) unlistenStateChanged();
  if (unlistenRecordingFinished) unlistenRecordingFinished();
  if (unlistenRecordingError) unlistenRecordingError();
  if (unlistenForceCompact) unlistenForceCompact();
  if (unlistenRecordingRegionSelected) unlistenRecordingRegionSelected();
  if (unlistenAudioMerging) unlistenAudioMerging();  
  if (unlistenMicToggled) unlistenMicToggled();
  if (unlistenMicKeyPressed) unlistenMicKeyPressed();
  if (unlistenMicKeyReleased) unlistenMicKeyReleased();
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
body {
  height: 100%;
  background: transparent;
}

#app {
  height: 100%;
  background: transparent;
  display: flex;
  justify-content: center;
  align-items: flex-start;
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

.recording-toolbar-audio-process-popper .el-select-dropdown__wrap {
  max-height: 168px !important;
}

.target-mode-buttons {
  display: inline-flex;
  gap: 4px;
  width: 100%;
}

.target-mode-btn {
  border: 1px solid rgba(255, 255, 255, 0.22);
  background: rgba(30, 35, 48, 0.72);
  color: #d6ddec;
  border-radius: 7px;
  padding: 4px 10px;
  font-size: 12px;
  line-height: 1.2;
  cursor: pointer;
  flex: 1 1 0;
  min-width: 0;
}

.target-mode-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.target-mode-btn.active {
  border-color: rgba(114, 183, 255, 0.82);
  background: rgba(72, 157, 255, 0.24);
  color: #ffffff;
}

.target-region-meta {
  margin-left: 8px;
  color: #c8d1e6;
  font-size: 11px;
  opacity: 0.86;
  white-space: nowrap;
  max-width: 210px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.toolbar-settings-title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
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
  transition: width 0.35s cubic-bezier(0.34, 1.56, 0.64, 1),
              min-height 0.35s cubic-bezier(0.34, 1.56, 0.64, 1),
              border-radius 0.35s cubic-bezier(0.34, 1.56, 0.64, 1),
              padding 0.35s cubic-bezier(0.34, 1.56, 0.64, 1),
              background 0.35s ease;
}

.bar.bar-collapsed {
  width: 210px;
  min-height: 40px;
  height: auto;
  padding: 9px 6px;
  gap: 0;
  justify-content: flex-start;
  align-items: stretch;
  border-radius: 999px;
  background: transparent;
  border: 1px solid transparent;
  overflow: hidden;
  box-sizing: border-box;
  cursor: default;
  -webkit-app-region: no-drag;
  clip-path: inset(0 round 999px);
}

.bar.bar-collapsed.bar-collapsed-settings-open {
  justify-content: flex-start;
  align-items: stretch;
  padding: 12px;
  width: 400px;
  border-radius: 12px;
  background: rgba(17, 22, 32, 0.92);
  border: 1px solid rgba(255, 255, 255, 0.12);
  clip-path: none;
}

.bar.bar-collapsed.bar-collapsed-settings-open .collapsed-shell {
  height: auto;
}

.collapsed-shell {
  display: flex;
  align-items: center;
  gap: 0;
  width: 100%;
  height: auto;
  min-height: 0;
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
  background-clip: padding-box;
  clip-path: inset(0 round 999px);
  transition: background 0.2s ease, border-color 0.2s ease;
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

.capsule-settings-panel-wrapper {
  display: grid;
  grid-template-rows: 0fr;
  transition: grid-template-rows 0.35s cubic-bezier(0.34, 1.56, 0.64, 1),
              margin-top 0.35s cubic-bezier(0.34, 1.56, 0.64, 1),
              padding-top 0.35s cubic-bezier(0.34, 1.56, 0.64, 1),
              border-top-color 0.35s ease;
  width: 100%;
  margin-top: 0;
  padding-top: 0;
  border-top: 1px solid transparent;
}

.capsule-settings-panel-wrapper.is-open {
  grid-template-rows: 1fr;
  margin-top: 12px;
  padding-top: 12px;
  border-top-color: rgba(255, 255, 255, 0.1);
}

.capsule-settings-panel {
  width: 376px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
  overflow: hidden;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.25s ease;
  padding-right: 4px;
  box-sizing: border-box;
}

.capsule-settings-panel-wrapper.is-open .capsule-settings-panel {
  opacity: 1;
  pointer-events: auto;
  transition: opacity 0.25s ease 0.15s;
}


.toolbar-inline-notice {
  margin-bottom: 4px;
  padding: 6px 24px 6px 8px;
  border-radius: 8px;
  font-size: 12px;
  line-height: 1.35;
  border: 1px solid transparent;
  max-width: 100%;
  overflow: visible;
  white-space: pre-wrap;
  word-break: break-word;
  overflow-wrap: anywhere;
  box-sizing: border-box;
  position: relative;
  cursor: pointer;
  transition: opacity 0.2s ease;
}

.toolbar-inline-notice:hover {
  opacity: 0.8;
}

.inline-notice-close {
  position: absolute;
  right: 8px;
  top: 50%;
  transform: translateY(-50%);
  font-size: 14px;
  font-weight: bold;
  opacity: 0.6;
}

.toolbar-inline-notice.is-warning {
  color: #c27803;
  background: rgba(255, 182, 39, 0.14);
  border-color: rgba(255, 182, 39, 0.36);
}

.toolbar-inline-notice.is-error {
  color: #ff6b6b;
  background: rgba(255, 107, 107, 0.12);
  border-color: rgba(255, 107, 107, 0.3);
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
  width: 100%;
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
  width: 100%;
  min-width: 0;
}

.toolbar-settings-row > :not(.toolbar-settings-label) {
  flex: 1 1 0;
  min-width: 0;
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
  width: 100% !important;
  flex: 1 1 auto;
  min-width: 112px;
}

.toolbar-settings-row :deep(.el-select) {
  width: 100% !important;
  flex: 1 1 auto;
  min-width: 0;
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

.collapsed-mic-toggle-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: 1px solid #344055;
  background: #202937;
  color: #dfebfc;
  border-radius: 999px;
  cursor: pointer;
  transition: all 0.15s ease;
  padding: 0;
  flex-shrink: 0;
}

.collapsed-mic-toggle-btn:hover:not(:disabled) {
  background: rgba(45, 62, 88, 0.95);
}

.collapsed-mic-toggle-btn:active:not(:disabled) {
  background: rgba(25, 35, 50, 0.95);
  transform: scale(0.96);
}

.collapsed-mic-toggle-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.collapsed-mic-toggle-btn.is-disabled {
  opacity: 0.35;
  cursor: not-allowed;
  background: rgba(60, 70, 85, 0.6);
  color: #8a95a5;
}

.collapsed-mic-toggle-btn.is-muted {
  background: rgba(180, 60, 60, 0.85);
  color: #ffd6d6;
}

.collapsed-mic-toggle-btn.is-muted:hover:not(:disabled) {
  background: rgba(200, 70, 70, 0.95);
}

.collapsed-mic-toggle-btn.is-active {
  color: #dfebfc;
}

.collapsed-mic-icon {
  display: flex;
  align-items: center;
  justify-content: center;
}
</style>
