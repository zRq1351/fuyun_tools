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
          <div :title="t('recordingToolbar.dragToolbar')" class="drag-handle" @mousedown.stop.prevent="startWindowDrag">
            <el-icon><GripVertical :size="13" :stroke-width="2.2"/></el-icon>
          </div>
          <button
              :disabled="!canStop"
              class="collapsed-stop-btn no-drag"
              type="button"
              @click.stop="stop"
          >
            <span class="collapsed-stop-icon"></span>
          </button>
          <div
              :data-state="currentRecordingState"
              :class="['collapsed-pill', { 'countdown-pill': countdownActive }]"
              @click.stop="countdownActive ? (countdownCancelled = true) : toggleRecordingState()"
          >
            <span class="collapsed-pill-content">
              <span v-if="countdownActive && !capsuleSettingsVisible" class="collapsed-countdown-num">{{ countdownValue }}</span>
              <template v-else>
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
              </template>
            </span>
          </div>
          <button
              class="collapsed-expand-btn no-drag"
              type="button"
              @click.stop="toggleCapsuleSettings"
          >
            <el-icon class="collapsed-expand-icon">
              <Settings :size="13" :stroke-width="2.2"/>
            </el-icon>
          </button>
          <button
              :class="['collapsed-mic-toggle-btn', 'no-drag', { 'is-muted': isMicMuted || !canToggleMic, 'is-active': !isMicMuted && canToggleMic, 'is-disabled': !canToggleMic || !microphoneDeviceId }]"
              :disabled="!canToggleMic || !microphoneDeviceId || isTogglingMic"
              type="button"
              @click.stop="toggleMicState"
          >
            <el-icon class="collapsed-mic-icon">
              <component :is="isMicMuted || !canToggleMic ? MicOff : Mic" :size="13" :stroke-width="2.2"/>
            </el-icon>
          </button>
          <button
              class="collapsed-close-btn no-drag"
              type="button"
              @click.stop="closeCapsule"
          >
            ×
          </button>
        </div>
        <div class="capsule-settings-panel-wrapper" :class="{ 'is-open': capsuleSettingsVisible }">
          <div class="capsule-settings-panel no-drag">
            <div v-if="inlineNotice" :class="['toolbar-inline-notice', `is-${inlineNoticeType}`]"
                 :title="t('recordingToolbar.clickToDismiss')" @click="clearInlineNotice">
              {{ inlineNotice }}
              <span class="inline-notice-close">×</span>
            </div>
          <div class="toolbar-settings-title-row">
            <div class="toolbar-settings-title">{{ t('recordingToolbar.recordingSettings') }}</div>
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
                {{ t('recordingToolbar.fullscreen') }}
              </button>
              <button
                  :class="['target-mode-btn', { active: recordTargetType === 'window' }]"
                  :disabled="!canEditRecordingConfig"
                  @click="onTargetModeClick('window')"
              >
                {{ t('recordingToolbar.window') }}
              </button>
              <button
                  :class="['target-mode-btn', { active: recordTargetType === 'region' }]"
                  :disabled="!canEditRecordingConfig"
                  @click="onTargetModeClick('region')"
              >
                {{ t('recordingToolbar.region') }}
              </button>
              <button
                  v-if="lastTargetType && lastTargetType !== 'screen'"
                  class="target-mode-btn target-repeat-btn"
                  :disabled="!canEditRecordingConfig"
                  :title="t('recordingToolbar.repeatLastTarget')"
                  @click="onRepeatLastTarget"
              >
                ↻
              </button>
            </div>
          </div>
          <div v-if="recordTargetType === 'window'" class="toolbar-settings-row">
            <span class="toolbar-settings-label">{{ t('recordingToolbar.targetWindow') }}</span>
            <el-select
                v-model="recordTargetWindowId"
                :disabled="!canEditRecordingConfig"
                filterable
                :placeholder="t('recordingToolbar.selectWindow')"
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
            <span class="toolbar-settings-label">{{ t('recordingToolbar.systemAudio') }}</span>
            <el-select
                :model-value="captureSystemAudio ? systemOutputId : ''"
                :placeholder="t('recordingToolbar.selectSystemAudio')"
                popper-class="recording-toolbar-select-popper"
                size="small"
                :disabled="!canEditAudioConfig"
                @visible-change="onSystemAudioDropdownVisibleChange"
                @change="onSystemAudioDeviceChange"
            >
              <el-option :label="t('recordingToolbar.noSystemAudio')" value=""/>
              <el-option
                  v-for="item in systemOutputs"
                  :key="item.id"
                  :label="item.name"
                  :value="item.id"
              />
            </el-select>
          </div>
          <div v-if="captureSystemAudio" class="toolbar-settings-row">
            <span class="toolbar-settings-label">{{ t('recordingToolbar.appAudio') }}</span>
            <el-select
                v-model="systemAudioProcessIds"
                :disabled="!canEditRecordingConfig"
                collapse-tags
                collapse-tags-tooltip
                filterable
                multiple
                :placeholder="t('recordingToolbar.appAudioPlaceholder')"
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
            <span class="toolbar-settings-label">{{ t('recordingToolbar.microphone') }}</span>
            <el-select
                :model-value="captureMicrophone ? microphoneDeviceId : ''"
                :placeholder="t('recordingToolbar.selectMicrophone')"
                popper-class="recording-toolbar-select-popper"
                size="small"
                :disabled="!canEditAudioConfig"
                @visible-change="onMicrophoneDropdownVisibleChange"
                @change="onMicrophoneDeviceChange"
            >
              <el-option :label="t('recordingToolbar.noMicrophone')" value=""/>
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
            {{ t('recordingToolbar.openSaveFolder') }}
          </button>
          <div class="toolbar-settings-switch-row">
            <el-switch
                v-model="captureCursor"
                :active-text="t('recordingToolbar.captureCursor')"
                :disabled="!canEditRecordingConfig"
                @change="onToolbarSettingChange('recordingCaptureCursor', $event)"
            />
              <el-switch
                  v-model="captureToolbar"
                  :active-text="t('recordingToolbar.captureToolbar')"
                  :disabled="!canEditRecordingConfig"
                  @change="onToolbarSettingChange('recordingToolbarContentProtected', $event)"
              />
          </div>
          <div class="toolbar-settings-row">
            <span class="toolbar-settings-label">{{ t('recordingToolbar.qualityPreset') }}</span>
            <el-select
                :model-value="qualityPreset"
                :disabled="!canEditRecordingConfig"
                size="small"
                style="width: 140px"
                @change="onPresetChange"
            >
              <el-option :label="t('recordingToolbar.presetSd')" value="sd" />
              <el-option :label="t('recordingToolbar.presetHd')" value="hd" />
              <el-option :label="t('recordingToolbar.presetFhd')" value="fhd" />
              <el-option :label="t('recordingToolbar.presetCustom')" value="custom" />
            </el-select>
          </div>
          <div class="toolbar-settings-row">
            <span class="toolbar-settings-label">{{ t('recordingToolbar.defaultFps') }}</span>
            <el-input-number
                :controls="false"
                :max="120"
                :min="1"
                :model-value="fps"
                :step="1"
                size="small"
                :disabled="!canEditRecordingConfig || qualityPreset !== 'custom'"
                @change="onToolbarSettingChange('recordingDefaultFps', $event)"
            />
          </div>
          <div class="toolbar-settings-row">
            <span class="toolbar-settings-label">{{ t('recordingToolbar.videoBitrate') }}</span>
            <el-input-number
                :controls="false"
                :max="50000"
                :min="500"
                :model-value="videoBitrateKbps"
                :step="500"
                size="small"
                :disabled="!canEditRecordingConfig || qualityPreset !== 'custom'"
                @change="onToolbarSettingChange('recordingDefaultVideoBitrateKbps', $event)"
            />
          </div>
          <div class="toolbar-settings-row">
            <span class="toolbar-settings-label">{{ t('recordingToolbar.audioBitrate') }}</span>
            <el-input-number
                :controls="false"
                :max="512"
                :min="32"
                :model-value="audioBitrateKbps"
                :step="16"
                size="small"
                :disabled="!canEditRecordingConfig || qualityPreset !== 'custom'"
                @change="onToolbarSettingChange('recordingDefaultAudioBitrateKbps', $event)"
            />
          </div>
          <div v-if="countdownActive && capsuleSettingsVisible" class="countdown-panel-overlay" @click.stop="countdownCancelled = true">
            <span class="countdown-in-panel-number">{{ countdownValue }}</span>
            <div class="countdown-cancel-hint">{{ t('recordingToolbar.pressEscToCancel') }}</div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import {computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch} from "vue";
import {useI18n} from "vue-i18n";
import {useLocale} from "@/composables/useLocale.js";
import {provideGlobalConfig} from "element-plus";
import {listen} from "@tauri-apps/api/event";
import {invoke} from "@tauri-apps/api/core";
import {getCurrentWindow} from "@tauri-apps/api/window";
import {AISettingsService, RecordingService} from "@/services/ipc.js";
import {parseErrorMessage} from "@/utils/errorHandler.js";
import {GripVertical, Mic, MicOff, Settings} from "lucide-vue-next";

const {t} = useI18n()

const {elLocale} = useLocale()

provideGlobalConfig({locale: elLocale});

const loadingAction = ref(null);
const capsuleSettingsVisible = ref(false);
// 后端已保证窗口只在录屏启用时才能打开（ensure_recording_toolbar_window 拦截），
// 因此初始即视为已启用，避免读取设置前短暂显示"录屏已停用"
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
let componentUnmounted = false;
// 进入区域选择前的目标模式，取消选区时回退（#16）
const targetModeBeforeRegionPick = ref('screen');
const inlineNotice = ref("");
const inlineNoticeType = ref("error");
let inlineNoticeTimer = null;

const fps = ref(30);
const videoBitrateKbps = ref(6000);
const audioBitrateKbps = ref(160);
const qualityPreset = ref('hd');
const countdownActive = ref(false);
const countdownValue = ref(3);
let countdownCancelled = false;
let countdownAbortController = null;
const lastTargetType = ref('');
const lastTargetId = ref('');

const PRESET_CONFIG = {
  sd: { fps: 15, videoBitrateKbps: 1000, audioBitrateKbps: 64 },
  hd: { fps: 30, videoBitrateKbps: 3000, audioBitrateKbps: 128 },
  fhd: { fps: 30, videoBitrateKbps: 6000, audioBitrateKbps: 192 },
};

function onPresetChange(val) {
  qualityPreset.value = val;
  if (val === 'custom') {
    // 切到 custom 也要持久化 preset，避免重载后 preset 与参数不一致（#35）
    AISettingsService.saveSettings({ recordingQualityPreset: val }).catch(() => {});
    return;
  }
  const cfg = PRESET_CONFIG[val];
  fps.value = cfg.fps;
  videoBitrateKbps.value = cfg.videoBitrateKbps;
  audioBitrateKbps.value = cfg.audioBitrateKbps;
  AISettingsService.saveSettings({
    recordingDefaultFps: cfg.fps,
    recordingDefaultVideoBitrateKbps: cfg.videoBitrateKbps,
    recordingDefaultAudioBitrateKbps: cfg.audioBitrateKbps,
    recordingQualityPreset: val,
  }).catch(() => {});
}
const captureCursor = ref(true);
const captureToolbar = ref(true);

const state = reactive({state: "idle", sessionId: null, elapsedMs: 0});
let unlistenStateChanged = null;
let unlistenRecordingFinished = null;
let unlistenRecordingError = null;
let unlistenForceCompact = null;
let unlistenRecordingRegionSelected = null;
let unlistenScreenshotReset = null;
let unlistenAudioMerging = null;  // ✅ 新增：监听音频合并事件
let unlistenMicToggled = null;  // ✅ 新增：监听麦克风切换事件
let unlistenMicKeyPressed = null;  // ✅ 新增：监听麦克风按键按下事件
let unlistenMicKeyReleased = null;  // ✅ 新增：监听麦克风按键释放事件
let unlistenVisibility = null;
let keepSettingsOpenUntilTs = 0;
let autoCollapseAfterStartPending = false;
let lastElapsedUiSyncAt = 0;
const isOpeningFolder = ref(false);
let openingFolderTimer = null;
let isUpdatingAudio = false;

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
  if (!recordingFeatureEnabled.value) return t('recordingToolbar.recordingDisabled');
  if (rawRecordingState.value === "recording") return t('recordingToolbar.recording');
  if (rawRecordingState.value === "paused") return t('recordingToolbar.recordingPaused');
  if (rawRecordingState.value === "starting") return t('recordingToolbar.starting');
  if (rawRecordingState.value === "stopping") return t('recordingToolbar.stopping');
  return t('recordingToolbar.startRecording');
});
const collapsedDisplayText = computed(() => {
  if (rawRecordingState.value === "recording" || rawRecordingState.value === "paused") {
    return elapsedText.value;
  }
  return recordingHintText.value;
});
const capsuleTooltipContent = computed(() => {
  if (!recordingFeatureEnabled.value) return t('recordingToolbar.disabledTooltip');
  if (rawRecordingState.value === "recording") return t('recordingToolbar.recordingTooltip');
  if (rawRecordingState.value === "paused") return t('recordingToolbar.pausedTooltip');
  if (rawRecordingState.value === "idle") return t('recordingToolbar.startTooltip');
  return recordingHintText.value;
});
const isBusy = computed(() => loadingAction.value !== null);
const canEditRecordingConfig = computed(() => !isBusy.value && (rawRecordingState.value === "idle" || rawRecordingState.value === "error"));
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
  if (!microphoneDeviceId.value) return t('recordingToolbar.selectMicFirst');
  if (!canToggleMic.value) return t('recordingToolbar.micDuringRecording');
  return isMicMuted.value ? t('recordingToolbar.clickEnableMic') : t('recordingToolbar.clickDisableMic');
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

// 并发互斥：多个 watch 同时触发时串行执行最后一次，避免 resize 竞态抖动（#36）
let capsuleLayoutRunning = false;
let capsuleLayoutQueued = false;

const syncCapsuleLayout = async () => {
  if (capsuleLayoutRunning) {
    capsuleLayoutQueued = true;
    return;
  }
  capsuleLayoutRunning = true;
  try {
    if (capsuleSettingsVisible.value) {
      await nextTick();
      const targetHeight = measureCapsuleContentHeight();
      await RecordingService.resizeToolbar(false, true, true, "capsule", false, targetHeight, null);
    } else {
      // Closing: Two-phase approach
      // Phase 1: Keep window width at 400px and height at targetHeight (keepWidth=true, openOverlay=true)
      // The CSS will animate .bar width and height. The window remains large so no clipping.
      await nextTick();
      const targetHeight = measureCapsuleContentHeight();
      await RecordingService.resizeToolbar(false, true, true, "capsule", false, targetHeight, null, true);

      // Phase 2: Wait for CSS transition to complete (0.18s)
      await new Promise((resolve) => setTimeout(resolve, 200));

      // Phase 3: Now shrink window width to 226px and height to 40px. The .bar is already small so no clipping.
      if (!capsuleSettingsVisible.value) {
        await RecordingService.resizeToolbar(false, false, true, "capsule", false, null, null, false);
      }
    }
  } catch (_e) {
  } finally {
    capsuleLayoutRunning = false;
    if (capsuleLayoutQueued) {
      capsuleLayoutQueued = false;
      void syncCapsuleLayout();
    }
  }
};

const startWindowDrag = () => {
  getCurrentWindow().startDragging().catch((e) => {
    console.error("拖动窗口失败:", e);
  });
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
  if (type === "warning") {
    inlineNoticeTimer = setTimeout(clearInlineNotice, 5000);
  } else if (type === "success") {
    inlineNoticeTimer = setTimeout(clearInlineNotice, 3000);
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
  const parsed = parseErrorMessage(message)
  const text = String(parsed || t('recordingToolbar.recordingError'));
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

let wasHiddenForRegionPick = false

const pickRecordingRegion = async () => {
  if (isPickingRegion) return;
  isPickingRegion = true;

  // 先隐藏录制工具栏窗口，避免遮挡截图编辑器的遮罩层
  wasHiddenForRegionPick = true
  try {
    await getCurrentWindow().hide();
  } catch (_e) {
    // 忽略隐藏失败
  }

  // 自动收起设置面板（注意：由于前面已经 hide，这里的 DOM 变化不会立刻显示在屏幕上，但能保证截图结束后再次 show 时面板是收起状态）
  capsuleSettingsVisible.value = false;

  // 等待一点时间确保窗口完全从屏幕缓冲区隐藏，并等待 vue 收起面板的动画逻辑完成，防止被截图录制进去
  await new Promise(resolve => setTimeout(resolve, 300));

  try {
    await invoke("open_screenshot_editor", {mode: "recording_region"});
  } catch (e) {
    showInlineNotice(t('recordingToolbar.openRegionFailed', {error: String(e)}), "error");
    // 失败时重新显示工具栏
    wasHiddenForRegionPick = false
    try {
      await getCurrentWindow().show();
    } catch (_e) {
    }
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
    targetModeBeforeRegionPick.value = prevMode || 'screen';
    void pickRecordingRegion();
  } else if (mode === "window") {
    void refreshRecordableWindows();
  }
};

const onRepeatLastTarget = async () => {
  if (!lastTargetType.value) return;
  recordTargetType.value = lastTargetType.value;
  if (lastTargetType.value === 'window' && lastTargetId.value) {
    recordTargetWindowId.value = lastTargetId.value;
    await refreshRecordableWindows();
  }
};

const saveLastTarget = () => {
  const type = recordTargetType.value;
  if (type === 'screen') return;
  lastTargetType.value = type;
  lastTargetId.value = type === 'window' ? recordTargetWindowId.value : '';
  try {
    localStorage.setItem('recording_last_target', JSON.stringify({ type, id: lastTargetId.value }));
  } catch (_) {}
};

const loadLastTarget = () => {
  try {
    const raw = localStorage.getItem('recording_last_target');
    if (raw) {
      const { type, id } = JSON.parse(raw);
      // 持久化的目标可能已失效（窗口已关闭等）：缺少必要 id 时视为无效（#53）
      if (type === 'window' && !id) {
        lastTargetType.value = '';
        lastTargetId.value = '';
        return;
      }
      lastTargetType.value = type || '';
      lastTargetId.value = id || '';
    }
  } catch (_) {}
};

const regionCoordinateText = computed(() => {
  if (!regionSelectionReady.value) return t('recordingToolbar.notSelected');
  const x1 = Math.round(recordRegionX.value);
  const y1 = Math.round(recordRegionY.value);
  const x2 = Math.round(recordRegionX.value + recordRegionWidth.value);
  const y2 = Math.round(recordRegionY.value + recordRegionHeight.value);
  return t('recordingToolbar.regionCoordinate', {x1, y1, x2, y2});
});

const formatTargetWindowLabel = (item) => {
  const title = String(item?.title || "").trim();
  const processNameRaw = String(item?.processName || item?.process_name || "").trim();
  const processName = processNameRaw.replace(/\.exe$/i, "");
  if (!title) return processName || t('recordingToolbar.unknownWindow');
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
    if (!recordingFeatureEnabled.value) {
      // 功能停用时点击引导到设置页的录屏标签开启，而不是静默无反应
      try {
        await invoke('open_settings_window', {tab: 'recording', reason: 'enable_recording'});
      } catch (e) {
        console.error('打开录屏设置失败:', e);
      }
      return;
    }
  }
  try {
    const prevRawState = rawRecordingState.value;
    if (rawRecordingState.value === "idle" || rawRecordingState.value === "error") {
      loadingAction.value = "start";
      autoCollapseAfterStartPending = true;
      if (recordTargetType.value === "window" && !recordTargetWindowId.value) {
        autoCollapseAfterStartPending = false;
        showInlineNotice(t('recordingToolbar.selectWindowFirst'), "warning");
        return;
      }
      if (recordTargetType.value === "region" && (!regionSelectionReady.value || recordRegionWidth.value <= 0 || recordRegionHeight.value <= 0)) {
        autoCollapseAfterStartPending = false;
        showInlineNotice(t('recordingToolbar.regionSizeInvalid'), "warning");
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
      countdownActive.value = true;
      const cancel = () => { countdownCancelled = true; };
      const onKey = (e) => { if (e.key === 'Escape') cancel(); };
      countdownAbortController = new AbortController();
      const {signal} = countdownAbortController;
      window.addEventListener('keydown', onKey, {signal});
      countdownCancelled = false;
      for (let i = 3; i >= 1; i--) {
        countdownValue.value = i;
        for (let _ = 0; _ < 50; _++) {
          if (countdownCancelled) break;
          await new Promise((r) => setTimeout(r, 20));
        }
        if (countdownCancelled) break;
      }
      window.removeEventListener('keydown', onKey);
      countdownAbortController = null;
      countdownActive.value = false;
      if (countdownCancelled) {
        autoCollapseAfterStartPending = false;
        isMicMuted.value = false;
        return;
      }
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
      saveLastTarget();
    } else if (rawRecordingState.value === "recording") {
      loadingAction.value = "pause";
      await RecordingService.pause();
    } else if (rawRecordingState.value === "paused") {
      loadingAction.value = "resume";
      await RecordingService.resume();
    }
    await refresh();
    if ((prevRawState === "idle" || prevRawState === "error") && rawRecordingState.value === "recording") {
      capsuleSettingsVisible.value = false;
      void syncCapsuleLayout();
      autoCollapseAfterStartPending = false;
    }
  } catch (e) {
    autoCollapseAfterStartPending = false;
    const msg = String(e || "");
    showBackendErrorInSettings(msg);
    try {
      await refresh();
    } catch (_) {
    }
  } finally {
    loadingAction.value = null;
  }
};

const stop = async () => {
  if (isBusy.value) return;
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
};

const closeCapsule = async () => {
  const isRecording = rawRecordingState.value === "recording" || rawRecordingState.value === "paused" || rawRecordingState.value === "starting" || rawRecordingState.value === "stopping";
  if (isRecording) {
    showInlineNotice(t('recordingToolbar.recordingInProgressCloseHint'), "warning");
    return;
  }
  capsuleSettingsVisible.value = false;
  try {
    await getCurrentWindow().hide();
  } catch (_e) {
  }
};

const isTogglingMic = ref(false);
const toggleMicState = async () => {
  if (!canToggleMic.value || isBusy.value || isTogglingMic.value) return;
  isTogglingMic.value = true;
  try {
    if (!canToggleMic.value) return;
    const newMutedState = !isMicMuted.value;
    await RecordingService.updateAudioCapture({
      captureSystemAudio: captureSystemAudio.value,
      systemAudioDeviceId: systemOutputId.value || "",
      captureMicrophone: !newMutedState,
      microphoneDeviceId: microphoneDeviceId.value || "",
    });
    isMicMuted.value = newMutedState;
    showInlineNotice(newMutedState ? t('recordingToolbar.micTemporarilyOff') : t('recordingToolbar.micReenabled'), "warning");
  } catch (e) {
    showBackendErrorInSettings(t('recordingToolbar.toggleMicFailed', {error: String(e)}));
  } finally {
    isTogglingMic.value = false;
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
    showInlineNotice(t('recordingToolbar.openSaveFolderFailed', {error: String(e)}), "error");
  } finally {
    openingFolderTimer = window.setTimeout(() => {
      isOpeningFolder.value = false;
      openingFolderTimer = null;
    }, 800);
  }
};

const onSystemAudioDeviceChange = async (deviceId) => {
  if (!canEditAudioConfig.value || isUpdatingAudio) return;
  isUpdatingAudio = true;
  try {
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
        // 切换失败可能已停止旧采集线程，尝试恢复原设备，避免后端静默停音
        try {
          await RecordingService.updateAudioCapture({
            captureSystemAudio: prevCapture,
            systemAudioDeviceId: prevId || "",
          });
        } catch (restoreErr) {
          console.error("恢复系统音频采集失败:", restoreErr);
        }
        showBackendErrorInSettings(String(e));
        return;
      }
    }
    try {
      await AISettingsService.saveSettings({
        recordingCaptureSystemAudio: captureSystemAudio.value,
        recordingSystemAudioDeviceId: systemOutputId.value || "",
      });
    } catch (e) {
      showInlineNotice(t('recordingToolbar.saveAudioSettingsFailed', {error: String(e)}), "error");
    }
  } finally {
    isUpdatingAudio = false;
  }
};

const onMicrophoneDeviceChange = async (deviceId) => {
  if (!canEditAudioConfig.value || isUpdatingAudio) return;
  isUpdatingAudio = true;
  try {
    const prevCapture = captureMicrophone.value;
    const prevId = microphoneDeviceId.value;
    const prevMuted = isMicMuted.value;
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
        isMicMuted.value = prevMuted;
        // 切换失败可能已停止旧采集线程，尝试恢复原设备，避免后端静默停音
        try {
          await RecordingService.updateAudioCapture({
            captureMicrophone: prevCapture,
            microphoneDeviceId: prevId || "",
          });
        } catch (restoreErr) {
          console.error("恢复麦克风采集失败:", restoreErr);
        }
        showBackendErrorInSettings(String(e));
        return;
      }
    }
    try {
      await AISettingsService.saveSettings({
        recordingCaptureMicrophone: captureMicrophone.value,
        recordingMicrophoneDeviceId: microphoneDeviceId.value || "",
      });
    } catch (e) {
      showInlineNotice(t('recordingToolbar.saveMicSettingsFailed', {error: String(e)}), "error");
    }
  } finally {
    isUpdatingAudio = false;
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
    recordTargetWindowId.value = "";
    if (recordTargetType.value === "window") {
      showInlineNotice(t('recordingToolbar.windowNoLongerAvailable'), "warning");
    }
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
    await AISettingsService.saveSettings(patch);
  } catch (e) {
    showInlineNotice(t('recordingToolbar.saveRecordingSettingsFailed', {error: String(e)}), "error");
  }
};

onMounted(async () => {
  window.addEventListener("blur", onWindowBlur);
  window.addEventListener("resize", onWindowViewportChanged);

  const listenerResults = await Promise.allSettled([
    listen("recording-state-changed", (event) => {
      const payload = event.payload || {};
      const incomingState = String(payload.state || state.state || "idle");
      const nextState = incomingState;
      const stateChanged = nextState !== state.state;
      state.state = nextState;
      state.sessionId = nextState === "idle" ? null : (payload.sessionId ?? state.sessionId);
      const nextElapsedMs = Number(payload.elapsedMs ?? state.elapsedMs ?? 0);
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
    }),
    listen("recording-finished", async (event) => {
      const payload = event.payload || {};
      const finishedSessionId = payload.sessionId ? String(payload.sessionId) : null;
      // 忽略旧会话/无会话的完成事件：有进行中会话时，无 sessionId 或 sessionId 不匹配都不应清 UI（#31）
      if (state.sessionId && (!finishedSessionId || finishedSessionId !== state.sessionId)) {
        return;
      }
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
    }),
    listen("recording-error", (event) => {
      const payload = event.payload || {};
      const rawMsg = String(payload.message || '');
      const message = parseErrorMessage(rawMsg) || t('recordingToolbar.recordingError');
      const code = String(payload.code || "");
      isMicMuted.value = false;
      // 仅当未在录制中时才切换为 idle，避免非致命错误（如麦克风切换失败）误杀录制状态
      const isRecording = state.state === "recording" || state.state === "paused" || state.state === "starting";
      if (!isRecording) {
        state.state = "idle";
        state.sessionId = null;
      }
      showBackendErrorInSettings(code ? `${code}: ${message}` : message);
    }),
    listen("recording-toolbar-force-compact", () => {
      if (Date.now() < keepSettingsOpenUntilTs) return;
      if (inlineNotice.value) return;
      capsuleSettingsVisible.value = false;
      void syncCapsuleLayout();
    }),
    listen("recording-region-selected", async (event) => {
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

      try {
        await getCurrentWindow().show();
      } catch (_e) {
      }
      await new Promise(resolve => setTimeout(resolve, 50));
      capsuleSettingsVisible.value = true;
      // 由 watch(capsuleSettingsVisible) → syncCapsuleLayout 统一处理布局，避免双重 resize 参数冲突（#32）
    }),
    listen("screenshot-reset", () => {
      if (wasHiddenForRegionPick) {
        wasHiddenForRegionPick = false;
        if (recordTargetType.value === "region" && !regionSelectionReady.value) {
          // 取消选区：回退到进入 region 前的目标模式，避免以默认坐标录制错误区域（#16）
          recordTargetType.value = targetModeBeforeRegionPick.value === 'window' ? 'window' : 'screen';
        }
        try {
          getCurrentWindow().show().catch(() => {
          });
        } catch (_e) {
        }
      }
    }),
    listen("recording-audio-merging", (event) => {
      const payload = event.payload || {};
      const status = String(payload.status || "");
      const message = String(payload.message || "");

      if (status === "started") {
        showInlineNotice(message || t('recordingToolbar.mergingAudioInBackground'), "warning");
      } else if (status === "completed") {
        clearInlineNotice();
      } else if (status === "failed") {
        showInlineNotice(message || t('recordingToolbar.audioMergeFailed'), "error");
      }
    }),
    listen("recording-mic-toggled", (event) => {
      const payload = event.payload || {};
      isMicMuted.value = !payload.enabled;
      const action = payload.enabled ? t('recordingToolbar.micActionEnable') : t('recordingToolbar.micActionDisable');
      showInlineNotice(t('recordingToolbar.micToggledByShortcut', {action}), "warning");
    }),
    listen("recording-mic-key-pressed", () => {
      isMicMuted.value = false;
    }),
    listen("recording-mic-key-released", () => {
      isMicMuted.value = true;
    }),
    listen("recording-exit-blocked", () => {
      // 应用退出被录屏阻止：提示用户先停止录制（#52）
      showInlineNotice(t('recordingToolbar.recordingInProgressCloseHint'), "warning");
    }),
  ]);

  // 倒计时期间窗口被隐藏（快捷键/关闭按钮）时取消开始，避免录制在无 UI 状态下进行
  getCurrentWindow()
      .onVisibilityChanged(({ visible }) => {
        if (!visible && countdownActive.value) {
          countdownCancelled = true;
        }
      })
      .then((fn) => {
        if (componentUnmounted) {
          fn();
          return;
        }
        unlistenVisibility = fn;
      })
      .catch((err) => {
        console.error("注册窗口可见性监听失败:", err);
      });

  const [stateChangedResult, recordingFinishedResult, recordingErrorResult, forceCompactResult, recordingRegionSelectedResult, screenshotResetResult, audioMergingResult, micToggledResult, micKeyPressedResult, micKeyReleasedResult] = listenerResults;

  if (stateChangedResult.status === "fulfilled") unlistenStateChanged = stateChangedResult.value;
  else console.error("注册 recording-state-changed 监听器失败:", stateChangedResult.reason);
  if (recordingFinishedResult.status === "fulfilled") unlistenRecordingFinished = recordingFinishedResult.value;
  else console.error("注册 recording-finished 监听器失败:", recordingFinishedResult.reason);
  if (recordingErrorResult.status === "fulfilled") unlistenRecordingError = recordingErrorResult.value;
  else console.error("注册 recording-error 监听器失败:", recordingErrorResult.reason);
  if (forceCompactResult.status === "fulfilled") unlistenForceCompact = forceCompactResult.value;
  else console.error("注册 recording-toolbar-force-compact 监听器失败:", forceCompactResult.reason);
  if (recordingRegionSelectedResult.status === "fulfilled") unlistenRecordingRegionSelected = recordingRegionSelectedResult.value;
  else console.error("注册 recording-region-selected 监听器失败:", recordingRegionSelectedResult.reason);
  if (screenshotResetResult.status === "fulfilled") unlistenScreenshotReset = screenshotResetResult.value;
  else console.error("注册 screenshot-reset 监听器失败:", screenshotResetResult.reason);
  if (audioMergingResult.status === "fulfilled") unlistenAudioMerging = audioMergingResult.value;
  else console.error("注册 recording-audio-merging 监听器失败:", audioMergingResult.reason);
  if (micToggledResult.status === "fulfilled") unlistenMicToggled = micToggledResult.value;
  else console.error("注册 recording-mic-toggled 监听器失败:", micToggledResult.reason);
  if (micKeyPressedResult.status === "fulfilled") unlistenMicKeyPressed = micKeyPressedResult.value;
  else console.error("注册 recording-mic-key-pressed 监听器失败:", micKeyPressedResult.reason);
  if (micKeyReleasedResult.status === "fulfilled") unlistenMicKeyReleased = micKeyReleasedResult.value;
  else console.error("注册 recording-mic-key-released 监听器失败:", micKeyReleasedResult.reason);
  try {
    const settings = await AISettingsService.getSettings();
    recordingFeatureEnabled.value = settings.recording_enabled === true;
    captureSystemAudio.value = settings.recording_capture_system_audio === true;
    captureMicrophone.value = settings.recording_capture_microphone === true;
    fps.value = Number(settings.recording_default_fps || 30);
    videoBitrateKbps.value = Number(settings.recording_default_video_bitrate_kbps || 6000);
    audioBitrateKbps.value = Number(settings.recording_default_audio_bitrate_kbps || 160);
    qualityPreset.value = settings.recording_quality_preset || 'hd';
    captureCursor.value = settings.recording_capture_cursor !== false;
    captureToolbar.value = settings.recording_toolbar_content_protected !== true;
    microphoneDeviceId.value = settings.recording_microphone_device_id || null;
    systemOutputId.value = settings.recording_system_audio_device_id || null;
    loadLastTarget();
  } catch (_e) {
  }
  await Promise.allSettled([
    refreshRecordableWindows(),
    refreshSystemOutputDevices(),
    refreshMicrophoneDevices(),
    refreshAudioProcesses(),
  ]);
  try {
    await refresh();
  } catch (_e) {
    // getState 失败时不阻塞布局初始化
  }
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
  if (!capsuleSettingsVisible.value) return;
  void syncCapsuleLayout();
});

onBeforeUnmount(() => {
  componentUnmounted = true;
  countdownCancelled = true;
  if (countdownAbortController) {
    countdownAbortController.abort();
    countdownAbortController = null;
  }
  if (inlineNoticeTimer) {
    clearTimeout(inlineNoticeTimer);
    inlineNoticeTimer = null;
  }
  if (openingFolderTimer) {
    clearTimeout(openingFolderTimer);
    openingFolderTimer = null;
  }
  window.removeEventListener("blur", onWindowBlur);
  window.removeEventListener("resize", onWindowViewportChanged);
  if (unlistenStateChanged) unlistenStateChanged();
  if (unlistenRecordingFinished) unlistenRecordingFinished();
  if (unlistenRecordingError) unlistenRecordingError();
  if (unlistenForceCompact) unlistenForceCompact();
  if (unlistenRecordingRegionSelected) unlistenRecordingRegionSelected();
  if (unlistenScreenshotReset) unlistenScreenshotReset();
  if (unlistenAudioMerging) unlistenAudioMerging();
  if (unlistenMicToggled) unlistenMicToggled();
  if (unlistenMicKeyPressed) unlistenMicKeyPressed();
  if (unlistenMicKeyReleased) unlistenMicKeyReleased();
  if (unlistenVisibility) unlistenVisibility();
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
  justify-content: flex-start;
  align-items: flex-start;
}

.recording-toolbar-select-popper.el-popper {
  --el-bg-color-overlay: var(--fy-bg-primary);
  --el-fill-color-light: var(--fy-bg-hover);
  --el-border-color-light: var(--fy-border);
  --el-text-color-primary: var(--fy-text-primary);
  width: 220px !important;
  max-width: 220px !important;
  min-width: 220px !important;
  background: var(--fy-bg-primary) !important;
  border-color: var(--fy-border) !important;
  box-shadow: var(--fy-shadow-lg) !important;
}

.recording-toolbar-select-popper .el-select-dropdown__item {
  color: var(--fy-text-primary);
}

.recording-toolbar-select-popper .el-select-dropdown__item.hover,
.recording-toolbar-select-popper .el-select-dropdown__item:hover,
.recording-toolbar-select-popper .el-select-dropdown__item.is-hovering {
  background: var(--fy-bg-hover);
}

.recording-toolbar-select-popper .el-select-dropdown__item.selected,
.recording-toolbar-select-popper .el-select-dropdown__item.is-selected {
  color: var(--fy-accent);
  font-weight: 600;
  background-color: var(--fy-accent-bg);
}

.recording-toolbar-select-popper .el-select-dropdown__item.is-disabled {
  color: var(--fy-text-muted);
  cursor: not-allowed;
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

.recording-toolbar-select-popper .el-select-dropdown__empty {
  color: var(--fy-text-muted);
}

.recording-toolbar-select-popper .el-scrollbar__bar.is-vertical .el-scrollbar__thumb,
.recording-toolbar-select-popper .el-scrollbar__bar.is-horizontal .el-scrollbar__thumb {
  background: var(--fy-border);
}

.recording-toolbar-select-popper .el-scrollbar__bar.is-vertical .el-scrollbar__thumb:hover,
.recording-toolbar-select-popper .el-scrollbar__bar.is-horizontal .el-scrollbar__thumb:hover {
  background: var(--fy-border-hover);
}

.recording-toolbar-audio-process-popper .el-select-dropdown__wrap {
  max-height: 168px !important;
}

/* Element Plus 深色主题 CSS 变量覆盖 - 所有组件自动生效 */
.capsule-settings-panel {
  position: relative;
  /* 背景色 */
  --el-bg-color: var(--fy-bg-primary);
  --el-bg-color-overlay: var(--fy-bg-overlay);
  --el-bg-color-page: var(--fy-bg-primary);

  /* 文字颜色 */
  --el-text-color-primary: var(--fy-text-primary);
  --el-text-color-regular: var(--fy-text-secondary);
  --el-text-color-secondary: var(--fy-text-muted);
  --el-text-color-placeholder: var(--fy-text-muted);
  --el-text-color-disabled: var(--fy-text-muted);

  /* 边框颜色 */
  --el-border-color: var(--fy-border);
  --el-border-color-light: var(--fy-border-light);
  --el-border-color-lighter: var(--fy-border-light);
  --el-border-color-extra-light: var(--fy-border-light);
  --el-border-color-dark: var(--fy-border);

  /* 填充色 */
  --el-fill-color: var(--fy-bg-surface);
  --el-fill-color-light: var(--fy-bg-hover);
  --el-fill-color-lighter: var(--fy-bg-hover);
  --el-fill-color-extra-light: var(--fy-bg-hover);
  --el-fill-color-dark: var(--fy-bg-primary);
  --el-fill-color-blank: var(--fy-bg-surface);

  /* 禁用状态 */
  --el-disabled-bg-color: var(--fy-bg-surface);
  --el-disabled-text-color: var(--fy-text-muted);
  --el-disabled-border-color: var(--fy-border-light);

  /* 输入框 */
  --el-input-text-color: var(--fy-text-primary);
  --el-input-placeholder-color: var(--fy-text-muted);
  --el-input-border-color: var(--fy-border);
  --el-input-hover-border-color: var(--fy-border-hover);
  --el-input-focus-border-color: var(--fy-border-active);
  --el-input-disabled-text-color: var(--fy-text-muted);
  --el-input-disabled-placeholder-color: var(--fy-text-muted);

  /* Switch */
  --el-switch-off-color: var(--fy-bg-hover);
}

/* 输入框/选择器 wrapper 样式 */
.capsule-settings-panel .el-select__wrapper {
  box-shadow: 0 0 0 1px var(--el-input-border-color) inset !important;
}

.capsule-settings-panel .el-select__wrapper:hover {
  box-shadow: 0 0 0 1px var(--el-input-hover-border-color) inset !important;
}

.capsule-settings-panel .el-select__wrapper.is-focused {
  box-shadow: 0 0 0 1px var(--el-input-focus-border-color) inset !important;
}

/* filterable 搜索输入框 */
.capsule-settings-panel .el-select__input {
  color: var(--fy-text-primary) !important;
  -webkit-text-fill-color: var(--fy-text-primary) !important;
}

/* Switch label */
.capsule-settings-panel .el-switch__label {
  color: var(--fy-text-secondary) !important;
}

/* 输入数字按钮 */
.capsule-settings-panel .el-input-number .el-input-number__decrease,
.capsule-settings-panel .el-input-number .el-input-number__increase {
  color: var(--fy-text-muted) !important;
}

.capsule-settings-panel .el-input-number .el-input-number__decrease:hover,
.capsule-settings-panel .el-input-number .el-input-number__increase:hover {
  color: var(--fy-text-primary) !important;
}

/* 目标模式按钮 */
.target-mode-buttons {
  display: inline-flex;
  gap: 4px;
  width: 100%;
}

.target-mode-btn {
  border: 1px solid var(--fy-border);
  background: var(--fy-bg-surface);
  color: var(--fy-text-secondary);
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
  border-color: var(--fy-border-active);
  background: var(--fy-accent-bg);
  color: var(--fy-accent);
}

.target-region-meta {
  margin-left: 8px;
  color: var(--fy-text-secondary);
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
  background: var(--fy-bg-primary) !important;
  border: 1px solid var(--fy-border-light) !important;
  color: var(--fy-text-primary) !important;
  box-shadow: var(--fy-shadow-lg);
}

.recording-toolbar-tooltip.el-popper .el-popper__arrow::before {
  background: var(--fy-bg-primary) !important;
  border: 1px solid var(--fy-border-light) !important;
}
</style>

<style scoped>
.bar {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--fy-bg-primary);
  border: none;
  border-radius: 10px;
  padding: 8px;
  flex-wrap: nowrap;
  white-space: nowrap;
  overflow: hidden;
  cursor: default;
  -webkit-app-region: no-drag;
  transition: width 0.18s ease-out,
  min-height 0.18s ease-out,
  border-radius 0.18s ease-out,
  padding 0.18s ease-out,
  background 0.15s ease;
}

.bar.bar-collapsed {
  width: 226px;
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
  background: var(--fy-bg-primary);
  border: 1px solid var(--fy-border-light);
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
  background: var(--fy-bg-surface);
  border: 1px solid var(--fy-border);
  user-select: none;
  cursor: pointer;
  background-clip: padding-box;
  clip-path: inset(0 round 999px);
  transition: background 0.2s ease, border-color 0.2s ease;
}

.collapsed-pill:hover {
  background: var(--fy-bg-hover);
}

.collapsed-pill[data-state="disabled"] {
  background: var(--fy-danger);
  border-color: var(--fy-danger);
}

.collapsed-pill[data-state="disabled"]:hover {
  background: var(--fy-danger);
}

.collapsed-pill[data-state="disabled"] .collapsed-pill-content {
  color: var(--fy-text-inverse);
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
  background: var(--fy-danger);
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
  background: var(--fy-warning);
  box-shadow: none;
  flex-shrink: 0;
  vertical-align: middle;
  filter: none;
}

.recording-ready-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--fy-success);
  flex-shrink: 0;
}

.bar.bar-collapsed .collapsed-pill {
  flex: 1;
  width: auto;
  min-width: 0;
  height: 22px;
}

.capsule-settings-panel-wrapper {
  position: relative;
  display: grid;
  grid-template-rows: 0fr;
  transition: grid-template-rows 0.18s ease-out,
  margin-top 0.18s ease-out,
  padding-top 0.18s ease-out,
  border-top-color 0.15s ease-out;
  width: 100%;
  margin-top: 0;
  padding-top: 0;
  border-top: 1px solid transparent;
}

.capsule-settings-panel-wrapper.is-open {
  grid-template-rows: 1fr;
  margin-top: 12px;
  padding-top: 12px;
  border-top-color: var(--fy-border-light);
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
  transition: opacity 0.15s ease-out;
  padding-right: 4px;
  box-sizing: border-box;
  background: transparent;
}

/* 覆盖 Element Plus 组件的默认白色背景 */
.capsule-settings-panel :deep(.el-input__wrapper),
.capsule-settings-panel :deep(.el-textarea__inner),
.capsule-settings-panel :deep(.el-select .el-input__wrapper),
.capsule-settings-panel :deep(.el-select__wrapper) {
  background-color: var(--el-fill-color-blank) !important;
  box-shadow: 0 0 0 1px var(--el-input-border-color) inset !important;
}

.capsule-settings-panel :deep(.el-input__wrapper:hover),
.capsule-settings-panel :deep(.el-textarea__inner:hover),
.capsule-settings-panel :deep(.el-select .el-input__wrapper:hover),
.capsule-settings-panel :deep(.el-select__wrapper:hover) {
  box-shadow: 0 0 0 1px var(--el-input-hover-border-color) inset !important;
}

.capsule-settings-panel :deep(.el-input__wrapper.is-focus),
.capsule-settings-panel :deep(.el-textarea__inner:focus),
.capsule-settings-panel :deep(.el-select .el-input__wrapper.is-focus),
.capsule-settings-panel :deep(.el-select__wrapper.is-focused) {
  box-shadow: 0 0 0 1px var(--el-input-focus-border-color) inset !important;
}

.capsule-settings-panel :deep(.el-input__inner),
.capsule-settings-panel :deep(.el-textarea__inner),
.capsule-settings-panel :deep(.el-select .el-input__inner),
.capsule-settings-panel :deep(.el-select__placeholder) {
  color: var(--el-input-text-color) !important;
}

.capsule-settings-panel :deep(.el-input__inner::placeholder),
.capsule-settings-panel :deep(.el-select .el-input__inner::placeholder),
.capsule-settings-panel :deep(.el-select__placeholder.is-transparent) {
  color: var(--el-input-placeholder-color) !important;
}

.capsule-settings-panel :deep(.el-input__suffix),
.capsule-settings-panel :deep(.el-input__prefix),
.capsule-settings-panel :deep(.el-select__caret),
.capsule-settings-panel :deep(.el-select__suffix) {
  color: var(--el-text-color-secondary) !important;
}

/* filterable 搜索输入框 */
.capsule-settings-panel :deep(.el-select__input) {
  color: var(--el-input-text-color) !important;
  -webkit-text-fill-color: var(--el-input-text-color) !important;
}

.capsule-settings-panel :deep(.el-select__input::placeholder) {
  color: var(--el-input-placeholder-color) !important;
  -webkit-text-fill-color: var(--el-input-placeholder-color) !important;
}

.capsule-settings-panel :deep(.el-select .el-tag) {
  background: var(--fy-accent-bg) !important;
  border-color: var(--fy-accent) !important;
}

.capsule-settings-panel :deep(.el-switch) {
  --el-switch-off-color: var(--fy-bg-hover);
  --el-switch-on-color: var(--fy-accent);
}

.capsule-settings-panel :deep(.el-switch__label) {
  color: var(--fy-text-secondary) !important;
}

.capsule-settings-panel-wrapper.is-open .capsule-settings-panel {
  opacity: 1;
  pointer-events: auto;
  transition: opacity 0.2s ease-out 0.05s;
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
  color: var(--fy-warning);
  background: var(--fy-warning-bg);
  border-color: var(--fy-warning);
  border-opacity: 0.36;
}

.toolbar-inline-notice.is-error {
  color: var(--fy-danger);
  background: var(--fy-danger-bg);
  border-color: var(--fy-danger);
  border-opacity: 0.3;
}

.collapsed-stop-btn,
.collapsed-expand-btn,
.collapsed-close-btn {
  width: 20px;
  height: 20px;
  border: 1px solid var(--fy-border);
  background: var(--fy-bg-surface);
  color: var(--fy-text-primary);
  border-radius: 999px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  cursor: pointer;
  flex-shrink: 0;
}

.collapsed-stop-btn {
  border-color: var(--fy-danger);
  background: var(--fy-danger-bg);
}

.collapsed-stop-btn:hover:not(:disabled) {
  background: var(--fy-danger-bg-hover, var(--fy-danger-bg));
}

.collapsed-stop-btn:active:not(:disabled) {
  background: var(--fy-danger-bg-hover, var(--fy-danger-bg));
}

.collapsed-stop-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.collapsed-stop-icon {
  width: 8px;
  height: 8px;
  border-radius: 2px;
  background: var(--fy-danger);
}

.collapsed-expand-btn {
  border-color: var(--fy-border);
  background: var(--fy-bg-hover);
  color: var(--fy-text-accent);
}

.collapsed-expand-btn:hover {
  background: var(--fy-bg-active);
}

.collapsed-expand-btn:active {
  background: var(--fy-bg-primary);
}

.collapsed-expand-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  line-height: 1;
}

.collapsed-close-btn {
  border-color: var(--fy-border);
  background: var(--fy-bg-hover);
  color: var(--fy-text-primary);
  font-size: 14px;
  line-height: 1;
}

.collapsed-close-btn:hover {
  background: var(--fy-bg-active);
}

.collapsed-close-btn:active {
  background: var(--fy-bg-primary);
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
  color: var(--fy-text-primary);
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
  color: var(--fy-text-primary);
  margin-bottom: 2px;
}

.toolbar-folder-btn {
  width: 100%;
  height: 30px;
  border: 1px solid var(--fy-border);
  background: var(--fy-bg-hover);
  color: var(--fy-text-primary);
  border-radius: 8px;
  padding: 0 12px;
  font-size: 12px;
  line-height: 30px;
  text-align: center;
  cursor: pointer;
}

.toolbar-folder-btn:hover {
  background: var(--fy-bg-active);
}

.toolbar-folder-btn:active {
  background: var(--fy-bg-primary);
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
  color: var(--fy-text-primary);
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
  border: 1px solid var(--fy-border);
  background: var(--fy-bg-surface);
  color: var(--fy-text-primary);
  border-radius: 999px;
  cursor: pointer;
  transition: all 0.15s ease;
  padding: 0;
  flex-shrink: 0;
}

.collapsed-mic-toggle-btn:hover:not(:disabled) {
  background: var(--fy-bg-hover);
}

.collapsed-mic-toggle-btn:active:not(:disabled) {
  background: var(--fy-bg-primary);
  transform: scale(0.96);
}

.collapsed-mic-toggle-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.collapsed-mic-toggle-btn.is-disabled {
  opacity: 0.35;
  cursor: not-allowed;
  background: var(--fy-bg-surface);
  color: var(--fy-text-muted);
}

.collapsed-mic-toggle-btn.is-muted {
  background: var(--fy-danger);
  color: var(--fy-text-inverse);
}

.collapsed-mic-toggle-btn.is-muted:hover:not(:disabled) {
  background: var(--fy-danger);
}

.collapsed-mic-toggle-btn.is-active {
  color: var(--fy-text-primary);
}

.collapsed-mic-icon {
  display: flex;
  align-items: center;
  justify-content: center;
}

.drag-handle {
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: move;
  color: var(--fy-text-muted);
  padding: 0 2px;
  height: 100%;
}

.drag-handle:active {
  cursor: move;
}

.drag-handle:hover {
  color: var(--fy-text-primary);
}

/* 全局交互增强：焦点环 + 过渡 */
button:focus-visible,
[role="button"]:focus-visible,
[tabindex]:focus-visible {
  outline: 2px solid var(--fy-accent);
  outline-offset: 2px;
}

button, [role="button"] {
  transition: transform 0.12s var(--fy-ease-out), filter 0.12s var(--fy-ease-out), opacity 0.15s var(--fy-ease-out);
}

button:active:not(:disabled),
[role="button"]:active:not([aria-disabled="true"]) {
  transform: scale(0.96);
}

.countdown-overlay {
  position: fixed; inset: 0; display: flex; align-items: center; justify-content: center;
  background: rgba(0, 0, 0, 0.5); z-index: 9999;
}
.countdown-number {
  font-size: 120px; font-weight: 700; color: #fff;
  text-shadow: 0 4px 20px rgba(0, 0, 0, 0.4);
  animation: countdown-pop 0.5s ease-out;
}
@keyframes countdown-pop {
  from { transform: scale(1.5); opacity: 0; }
  to { transform: scale(1); opacity: 1; }
}
.target-repeat-btn {
  font-size: 15px;
  width: 36px;
  flex: 0 0 auto;
}
.countdown-pill {
  cursor: default;
}
.collapsed-countdown-num {
  font-size: 20px;
  font-weight: 700;
  animation: countdown-pop 0.4s ease-out;
}
/* 展开时：半透明遮罩覆盖在设置面板上 */
/* 展开时：半透明遮罩覆盖在设置内容上方 */
.countdown-panel-overlay {
  position: absolute; inset: -12px; z-index: 10;
  display: flex; flex-direction: column; align-items: center; justify-content: center;
}
.countdown-in-panel-number {
  display: flex; align-items: center; justify-content: center;
  width: 180px; height: 180px;
  font-size: 96px; font-weight: 700;
  color: var(--fy-text-primary);
  background: var(--fy-glass-bg);
  backdrop-filter: blur(40px) saturate(180%);
  border-radius: 50%;
  animation: countdown-pop 0.5s ease-out;
}
.countdown-cancel-hint {
  margin-top: 12px; font-size: 13px;
  color: var(--fy-text-secondary);
  background: var(--fy-glass-bg);
  backdrop-filter: blur(20px) saturate(180%);
  padding: 4px 16px;
  border-radius: 20px;
}
@keyframes countdown-pop {
  from { transform: scale(1.5); opacity: 0; }
  to { transform: scale(1); opacity: 1; }
}
</style>
