<template>
  <el-config-provider :locale="zhCn">
    <div
        ref="barRef"
        :class="{ 'bar-collapsed': isToolbarCollapsed }"
        :data-tauri-drag-region="isToolbarCollapsed ? null : ''"
        class="bar"
        @click="onBarClick"
        @mouseenter="onBarMouseEnter"
        @mouseleave="onBarMouseLeave"
    >
      <div
          v-if="isToolbarCollapsed"
          :data-state="rawRecordingState"
          class="collapsed-shell"
      >
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
              :data-state="rawRecordingState"
              class="collapsed-pill"
              @click.stop="toggleRecordingState"
          >
            <span class="collapsed-pill-content">
              <span
                  v-if="rawRecordingState === 'recording'"
                  class="recording-dot"
              ></span>
              <span
                  v-else-if="rawRecordingState === 'paused'"
                  class="recording-pause-square"
              ></span>
              <span
                  v-else-if="rawRecordingState === 'idle'"
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
              @click.stop="expandFromCapsule"
          >
            <el-icon class="collapsed-expand-icon"
            >
              <Settings :size="13" :stroke-width="2.2"
              />
            </el-icon>
          </button>
        </el-tooltip>
      </div>

      <div v-else class="expanded-content">
        <div class="time">{{ elapsedText }}</div>
        <span
            class="no-drag"
            @mouseenter="onButtonHoverChange('microphone', true)"
            @mouseleave="onButtonHoverChange('microphone', false)"
        >
          <el-popover
            v-model:visible="microphonePopoverVisible"
            placement="bottom-start"
            popper-class="recording-toolbar-select-popper"
            trigger="manual"
            @hide="onSelectVisibleChange('microphone', false)"
            @show="onSelectVisibleChange('microphone', true)"
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
              <div v-if="microphones.length === 0" class="device-empty">
                暂无麦克风设备
              </div>
            </div>
            <template #reference>
              <el-button
                circle
                class="icon-btn"
                size="small"
                @click="toggleMicrophone"
              >
                <el-icon v-if="captureMicrophone"
                ><Mic :size="18" :stroke-width="2.2"
                /></el-icon>
                <el-icon v-else
                ><MicOff :size="18" :stroke-width="2.2"
                /></el-icon>
              </el-button>
            </template>
          </el-popover>
        </span>

        <span
            class="no-drag"
            @mouseenter="onButtonHoverChange('systemAudio', true)"
            @mouseleave="onButtonHoverChange('systemAudio', false)"
        >
          <el-popover
            v-model:visible="systemAudioPopoverVisible"
            :width="systemOutputListWidth"
            placement="bottom-start"
            popper-class="recording-toolbar-select-popper"
            trigger="manual"
            @hide="onSelectVisibleChange('systemAudio', false)"
            @show="onSelectVisibleChange('systemAudio', true)"
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
              <div v-if="systemOutputs.length === 0" class="device-empty">
                暂无系统音频设备
              </div>
            </div>
            <template #reference>
              <el-button
                circle
                class="icon-btn"
                size="small"
                @click="toggleSystemAudio"
              >
                <el-icon v-if="captureSystemAudio"
                ><Volume2 :size="18" :stroke-width="2.2"
                /></el-icon>
                <el-icon v-else
                ><VolumeOff :size="18" :stroke-width="2.2"
                /></el-icon>
              </el-button>
            </template>
          </el-popover>
        </span>

        <span
            class="no-drag"
            @mouseenter="onButtonHoverChange('openFolder', true)"
            @mouseleave="onButtonHoverChange('openFolder', false)"
        >
          <el-tooltip
              :offset="14"
              content="打开视频目录"
              effect="dark"
              placement="bottom"
              popper-class="recording-toolbar-tooltip"
              @visible-change="
              (visible) => onTooltipVisibleChange('openFolder', visible)
            "
          >
            <el-button
              :loading="false"
              circle
              class="icon-btn"
              size="small"
              @click="openFolder"
            >
              <span class="action-icon-slot">
                <el-icon
                    :class="{
                    'action-icon-hidden': loadingAction === 'openFolder',
                  }"
                    class="action-icon"
                ><FolderOpen :size="18" :stroke-width="2.2"
                /></el-icon>
                <el-icon
                    :class="{
                    'action-icon-hidden': loadingAction !== 'openFolder',
                  }"
                    class="action-icon action-loading-icon"
                ><Loading
                /></el-icon>
              </span>
            </el-button>
          </el-tooltip>
        </span>
        <span
            class="no-drag"
            @mouseenter="onButtonHoverChange('close', true)"
            @mouseleave="onButtonHoverChange('close', false)"
        >
          <el-tooltip
              :content="closeTooltipText"
              :offset="14"
              effect="dark"
              placement="bottom"
              popper-class="recording-toolbar-tooltip"
              @visible-change="
              (visible) => onTooltipVisibleChange('close', visible)
            "
          >
            <el-button circle class="icon-btn" size="small" @click="closeBar">
              <el-icon><Close/></el-icon>
            </el-button>
          </el-tooltip>
        </span>
      </div>
    </div>
  </el-config-provider>
</template>

<script setup>
import {computed, onBeforeUnmount, onMounted, reactive, ref, watch,} from "vue";
import zhCn from "element-plus/dist/locale/zh-cn";
import {listen} from "@tauri-apps/api/event";
import {getCurrentWindow} from "@tauri-apps/api/window";
import {AISettingsService, RecordingService} from "@/services/ipc.js";
import {ElMessage} from "element-plus";
import {Close, Loading} from "@element-plus/icons-vue";
import {FolderOpen, Mic, MicOff, Settings, Volume2, VolumeOff,} from "lucide-vue-next";

const loadingAction = ref(null);
const captureSystemAudio = ref(false);
const captureMicrophone = ref(true);
const microphonePopoverVisible = ref(false);
const systemAudioPopoverVisible = ref(false);
const systemOutputs = ref([]);
const systemOutputId = ref(null);
const microphones = ref([]);
const microphoneDeviceId = ref(null);
const fps = ref(30);
const state = reactive({state: "idle", sessionId: null, elapsedMs: 0});
let unlistenStateChanged = null;
let unlistenRecordingFinished = null;
let unlistenRecordingError = null;
let unlistenForceCompact = null;
let lastElapsedUiSyncAt = 0;
const barRef = ref(null);
const openSelectIds = ref(new Set()); // 麦克风/系统音频下拉层
const openTooltipIds = ref(new Set()); // tooltip 可见
const hoveredControlIds = ref(new Set()); // 按钮 hover（用于预留 tooltip 空间）
const isPointerOverBar = ref(false);
const forceCompactMode = ref(false);
let collapseTimer = null;
const isToolbarCollapsed = ref(false);
let pendingToolbarResizePayload = null;
let lastAppliedToolbarResizePayload = null;
let isApplyingToolbarResize = false;
let suppressLayoutSync = false;
const COLLAPSE_DELAY_MS = 0;

let _measureCanvas = null;
const ensureMeasureCtx = () => {
  if (!_measureCanvas) {
    _measureCanvas = document.createElement("canvas");
  }
  const ctx = _measureCanvas.getContext("2d");
  ctx.font =
      '14px -apple-system, BlinkMacSystemFont, Segoe UI, Roboto, Helvetica, Arial, \"Microsoft Yahei\", sans-serif';
  return ctx;
};
const computeListWidth = (items, labelSelector) => {
  const names = Array.isArray(items) ? items.map(labelSelector) : [];
  if (names.length === 0) return 220;
  const ctx = ensureMeasureCtx();
  let max = 0;
  for (const s of names) {
    max = Math.max(max, ctx.measureText(String(s || "")).width);
  }
  // 左右内边距与滚动条余量
  const padding = 20 + 16;
  const raw = Math.ceil(max + padding);
  const minW = 220;
  const maxW = 900;
  return Math.max(minW, Math.min(maxW, raw));
};
const microphoneListWidth = computed(() =>
    computeListWidth(microphones.value, (it) => it.name),
);
const systemOutputListWidth = computed(() =>
    computeListWidth(systemOutputs.value, (it) => it.name),
);

const formatElapsedText = (ms) => {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
};
const elapsedText = computed(() => formatElapsedText(state.elapsedMs));
const rawRecordingState = computed(() =>
    String(state.state || "idle").toLowerCase(),
);
const currentRecordingState = computed(() => {
  const normalized = rawRecordingState.value;
  if (
      normalized === "idle" ||
      normalized === "recording" ||
      normalized === "paused" ||
      normalized === "starting" ||
      normalized === "stopping" ||
      normalized === "error"
  ) {
    return normalized;
  }
  return state.sessionId ? "recording" : "idle";
});
const recordingHintText = computed(() => {
  if (rawRecordingState.value === "recording") return "正在录屏";
  if (rawRecordingState.value === "paused") return "录屏已暂停";
  if (rawRecordingState.value === "starting") return "录屏启动中";
  if (rawRecordingState.value === "stopping") return "录屏停止中";
  if (rawRecordingState.value === "error") return "录屏异常";
  return "开始录制";
});
const collapsedDisplayText = computed(() => {
  // 只显示时间，去掉"已录制/已暂停"前缀
  if (
      rawRecordingState.value === "recording" ||
      rawRecordingState.value === "paused"
  ) {
    return elapsedText.value;
  }
  return recordingHintText.value;
});
const capsuleTooltipContent = computed(() => {
  if (rawRecordingState.value === "recording") return "正在录制，点击暂停";
  if (rawRecordingState.value === "paused") return "已暂停，点击恢复录制";
  if (rawRecordingState.value === "idle") return "开始录制，点击开始";
  return recordingHintText.value;
});
const isBusy = computed(() => loadingAction.value !== null);
const canStop = computed(
    () =>
        !isBusy.value &&
        (currentRecordingState.value === "recording" ||
            currentRecordingState.value === "paused"),
);
const isRecordingSessionActive = computed(() =>
    ["starting", "recording", "paused", "stopping"].includes(
        currentRecordingState.value,
    ),
);
const closeTooltipText = computed(() => {
  if (
      currentRecordingState.value === "recording" ||
      currentRecordingState.value === "paused"
  ) {
    return "隐藏工具栏（录制继续）";
  }
  return "关闭工具栏";
});

const toggleRecordingState = async () => {
  if (isBusy.value) return;
  try {
    if (rawRecordingState.value === "idle") {
      loadingAction.value = "start";
      await RecordingService.start({
        captureSystemAudio: captureSystemAudio.value,
        systemAudioDeviceId: systemOutputId.value,
        captureMicrophone: captureMicrophone.value,
        microphoneDeviceId: microphoneDeviceId.value,
        captureCursor: true,
        fps: fps.value,
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
    ElMessage.error(String(e));
  } finally {
    loadingAction.value = null;
  }
};

const refresh = async () => {
  const data = await RecordingService.getState();
  state.state = data.state || state.state || "idle";
  state.sessionId = data.sessionId || null;
  state.elapsedMs = Number(data.elapsedMs || 0);
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

const openFolder = async () => {
  loadingAction.value = "openFolder";
  try {
    await RecordingService.openFolder();
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loadingAction.value = null;
  }
};

const closeBar = async () => {
  suppressLayoutSync = true;
  clearCollapseTimer();
  resetInteractionState();
  try {
    await getCurrentWindow().hide();
  } catch (_e) {
  } finally {
    // 避免关闭瞬间触发的状态变更再次触发布局抖动
    setTimeout(() => {
      suppressLayoutSync = false;
    }, 0);
  }
};

const mutateIdSet = (setRef, id, visible) => {
  const next = new Set(setRef.value);
  if (visible) {
    next.add(id);
  } else {
    next.delete(id);
  }
  setRef.value = next;
};

const clearTransientOverlayState = () => {
  openTooltipIds.value = new Set();
  hoveredControlIds.value = new Set();
};

const hasOpenSelectOverlay = () => openSelectIds.value.size > 0;
const hasTransientOverlay = () =>
    openTooltipIds.value.size > 0 || hoveredControlIds.value.size > 0;

const reconcileSelectOverlayState = () => {
  const next = new Set();
  if (microphonePopoverVisible.value) next.add("microphone");
  if (systemAudioPopoverVisible.value) next.add("systemAudio");
  openSelectIds.value = next;
};

const buildToolbarSnapshot = () => ({
  recordingActive: isRecordingSessionActive.value,
  pointerOverBar: isPointerOverBar.value,
  openSelect: hasOpenSelectOverlay(),
  openOverlay: hasTransientOverlay(),
  forceCompact: forceCompactMode.value,
});

const deriveCollapsedFromSnapshot = (snapshot) =>
    snapshot.forceCompact ||
    (snapshot.recordingActive && !snapshot.pointerOverBar && !snapshot.openSelect);

const deriveResizePayloadFromSnapshot = (snapshot, compactMode) => ({
  openSelect: snapshot.openSelect,
  // 仅 tooltip 可见时增高窗口，不再由按钮 hover 触发
  openOverlay: snapshot.openOverlay,
  compactMode,
});

const isSameToolbarResizePayload = (a, b) => {
  if (!a || !b) return false;
  return (
      a.openSelect === b.openSelect &&
      a.openOverlay === b.openOverlay &&
      a.compactMode === b.compactMode
  );
};

const applyToolbarWindowSize = async (payload) => {
  pendingToolbarResizePayload = payload;
  if (isApplyingToolbarResize) return;

  isApplyingToolbarResize = true;
  try {
    while (pendingToolbarResizePayload) {
      const current = pendingToolbarResizePayload;
      pendingToolbarResizePayload = null;
      if (isSameToolbarResizePayload(current, lastAppliedToolbarResizePayload))
        continue;
      try {
        await RecordingService.resizeToolbar(
            current.openSelect,
            current.openOverlay,
            current.compactMode,
        );
        lastAppliedToolbarResizePayload = current;
      } catch (_e) {
      }
    }
  } finally {
    isApplyingToolbarResize = false;
  }
};

const clearCollapseTimer = () => {
  if (!collapseTimer) return;
  clearTimeout(collapseTimer);
  collapseTimer = null;
};

const commitToolbarLayout = async (snapshot = null) => {
  let currentSnapshot = snapshot || buildToolbarSnapshot();
  const nextCollapsed = deriveCollapsedFromSnapshot(currentSnapshot);
  if (isToolbarCollapsed.value !== nextCollapsed) {
    isToolbarCollapsed.value = nextCollapsed;
    if (nextCollapsed) {
      clearTransientOverlayState();
      currentSnapshot = buildToolbarSnapshot();
    }
  }
  const payload = deriveResizePayloadFromSnapshot(
      currentSnapshot,
      isToolbarCollapsed.value,
  );
  await applyToolbarWindowSize(payload);
};

const scheduleToolbarTransition = (snapshot) => {
  clearCollapseTimer();
  if (!deriveCollapsedFromSnapshot(snapshot)) {
    void commitToolbarLayout(snapshot);
    return;
  }
  collapseTimer = setTimeout(() => {
    collapseTimer = null;
    reconcileSelectOverlayState();
    void commitToolbarLayout(buildToolbarSnapshot());
  }, COLLAPSE_DELAY_MS);
};

const requestToolbarLayoutSync = (immediate = false) => {
  if (suppressLayoutSync) return;
  reconcileSelectOverlayState();
  const snapshot = buildToolbarSnapshot();
  if (immediate) {
    clearCollapseTimer();
    void commitToolbarLayout(snapshot);
    return;
  }
  scheduleToolbarTransition(snapshot);
};

const onBarMouseEnter = () => {
  if (isToolbarCollapsed.value) return;
  isPointerOverBar.value = true;
  requestToolbarLayoutSync(true);
};

const onBarMouseLeave = () => {
  isPointerOverBar.value = false;
  clearTransientOverlayState();
};

const onBarClick = () => {
  if (!isToolbarCollapsed.value) return;
  expandFromCapsule();
};

const expandFromCapsule = () => {
  if (!isToolbarCollapsed.value) return;
  forceCompactMode.value = false;
  isPointerOverBar.value = true;
  requestToolbarLayoutSync(true);
};

const onWindowBlur = () => {
  resetInteractionState();
  requestToolbarLayoutSync(true);
};

const onSelectVisibleChange = (id, visible) => {
  mutateIdSet(openSelectIds, id, visible);
  requestToolbarLayoutSync(true);
};

const onTooltipVisibleChange = (id, visible) => {
  mutateIdSet(openTooltipIds, id, visible);
  requestToolbarLayoutSync(true);
};

const onButtonHoverChange = (id, visible) => {
  mutateIdSet(hoveredControlIds, id, visible);
  requestToolbarLayoutSync(true);
};

const toggleMicrophone = async () => {
  if (captureMicrophone.value) {
    captureMicrophone.value = false;
    microphonePopoverVisible.value = false;
    return;
  }
  captureMicrophone.value = true;
  if (!microphoneDeviceId.value && microphones.value.length > 0) {
    const def = microphones.value.find((it) => it.isDefault);
    microphoneDeviceId.value = def ? def.id : microphones.value[0].id;
  }
  microphonePopoverVisible.value = true;
};

const selectMicrophone = async (deviceId) => {
  microphoneDeviceId.value = deviceId;
  microphonePopoverVisible.value = false;
};

const toggleSystemAudio = async () => {
  if (captureSystemAudio.value) {
    captureSystemAudio.value = false;
    systemAudioPopoverVisible.value = false;
    return;
  }
  captureSystemAudio.value = true;
  if (!systemOutputId.value && systemOutputs.value.length > 0) {
    const def = systemOutputs.value.find((it) => it.isDefault);
    systemOutputId.value = def ? def.id : systemOutputs.value[0].id;
  }
  systemAudioPopoverVisible.value = true;
};

const selectSystemOutput = async (deviceId) => {
  systemOutputId.value = deviceId;
  systemAudioPopoverVisible.value = false;
};

const resetInteractionState = () => {
  isPointerOverBar.value = false;
  clearTransientOverlayState();
  openSelectIds.value = new Set();
  microphonePopoverVisible.value = false;
  systemAudioPopoverVisible.value = false;
};

const syncCompactModeFromWindowSize = async () => {
  try {
    const size = await getCurrentWindow().outerSize();
    forceCompactMode.value = Number(size?.width || 0) <= 260;
  } catch (_e) {
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
    resetInteractionState();
    requestToolbarLayoutSync(true);
  });
  unlistenRecordingError = await listen("recording-error", () => {
    state.state = "error";
    resetInteractionState();
    requestToolbarLayoutSync(true);
  });
  unlistenForceCompact = await listen("recording-toolbar-force-compact", () => {
    forceCompactMode.value = true;
    isPointerOverBar.value = false;
    clearTransientOverlayState();
    requestToolbarLayoutSync(true);
  });
  try {
    const settings = await AISettingsService.getSettings();
    captureSystemAudio.value = settings.recording_capture_system_audio === true;
    captureMicrophone.value = settings.recording_capture_microphone !== false;
    fps.value = Number(settings.recording_default_fps || 30);
  } catch (_e) {
  }
  try {
    const outs = await RecordingService.listSystemOutputs();
    systemOutputs.value = Array.isArray(outs) ? outs : [];
    const def = systemOutputs.value.find((it) => it.isDefault);
    systemOutputId.value = def ? def.id : (systemOutputs.value[0]?.id ?? null);
  } catch (e) {
    systemOutputs.value = [];
    systemOutputId.value = null;
    ElMessage.error(`加载系统音频设备失败: ${String(e)}`);
  }
  try {
    const mics = await RecordingService.listAudioDevices();
    microphones.value = Array.isArray(mics) ? mics : [];
    const def = microphones.value.find((it) => it.isDefault);
    microphoneDeviceId.value = def
        ? def.id
        : (microphones.value[0]?.id ?? null);
  } catch (e) {
    microphones.value = [];
    microphoneDeviceId.value = null;
    ElMessage.error(`加载麦克风设备失败: ${String(e)}`);
  }
  await syncCompactModeFromWindowSize();
  await refresh();
  requestToolbarLayoutSync(true);
});

watch(currentRecordingState, (next) => {
  if (next === "idle" || next === "error") {
    resetInteractionState();
  }
  requestToolbarLayoutSync(true);
});

watch([microphonePopoverVisible, systemAudioPopoverVisible], () => {
  reconcileSelectOverlayState();
  requestToolbarLayoutSync(true);
});

onBeforeUnmount(() => {
  window.removeEventListener("blur", onWindowBlur);
  clearCollapseTimer();
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

.collapsed-shell {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  height: 100%;
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
  background: #4aa7f8;
  border: 1px solid #66b5f7;
  user-select: none;
  cursor: pointer;
  transition: none !important;
  animation: none !important;
  background-clip: padding-box;
  clip-path: inset(0 round 999px);
}

.collapsed-pill:hover {
  filter: none;
  background: #5ab0fa;
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
  background: #ff4d4d;
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

.collapsed-stop-btn,
.collapsed-expand-btn {
  width: 22px;
  height: 22px;
  border-radius: 999px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  cursor: pointer;
  flex-shrink: 0;
}

.collapsed-stop-btn {
  border: 1px solid #d85a5a;
  background: #e46a6a;
}

.collapsed-stop-btn:hover:not(:disabled) {
  background: #ec7878;
}

.collapsed-stop-btn:active:not(:disabled) {
  background: #cf5959;
}

.collapsed-stop-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.collapsed-stop-icon {
  width: 8px;
  height: 8px;
  border-radius: 2px;
  background: #fff7f7;
}

.collapsed-expand-btn {
  border: 1px solid rgba(98, 170, 255, 0.7);
  background: #4f96f3;
  color: #f4f8ff;
}

.collapsed-expand-btn:hover {
  background: #63a5fb;
}

.collapsed-expand-btn:active {
  background: #3f88e6;
}

.collapsed-shell[data-state="paused"] .collapsed-expand-btn {
  border-color: rgba(227, 172, 36, 0.78);
  background: #edbe43;
  color: #402800;
}

.collapsed-shell[data-state="paused"] .collapsed-expand-btn:hover {
  background: #f3c95b;
}

.collapsed-shell[data-state="paused"] .collapsed-expand-btn:active {
  background: #d9aa33;
}

.collapsed-shell[data-state="idle"] .collapsed-expand-btn {
  border-color: rgba(83, 182, 123, 0.8);
  background: #46bf77;
  color: #f4fff8;
}

.collapsed-shell[data-state="idle"] .collapsed-expand-btn:hover {
  background: #58ca86;
}

.collapsed-shell[data-state="idle"] .collapsed-expand-btn:active {
  background: #38ad67;
}

.collapsed-shell[data-state="starting"] .collapsed-expand-btn,
.collapsed-shell[data-state="stopping"] .collapsed-expand-btn {
  border-color: rgba(143, 192, 229, 0.8);
  background: #7eb5df;
  color: #f4f8ff;
}

.collapsed-shell[data-state="starting"] .collapsed-expand-btn:hover,
.collapsed-shell[data-state="stopping"] .collapsed-expand-btn:hover {
  background: #93c2e6;
}

.collapsed-shell[data-state="starting"] .collapsed-expand-btn:active,
.collapsed-shell[data-state="stopping"] .collapsed-expand-btn:active {
  background: #6ba4d3;
}

.collapsed-shell[data-state="error"] .collapsed-expand-btn {
  border-color: rgba(227, 118, 118, 0.78);
  background: #df7171;
  color: #fff4f4;
}

.collapsed-shell[data-state="error"] .collapsed-expand-btn:hover {
  background: #e78383;
}

.collapsed-shell[data-state="error"] .collapsed-expand-btn:active {
  background: #cb5f5f;
}

.collapsed-expand-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  line-height: 1;
}

.collapsed-pill-content {
  display: inline-flex;
  flex-direction: row;
  align-items: center;
  justify-content: center;
  gap: 6px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 13px;
  color: #f8fbff;
  font-weight: 700;
  letter-spacing: 0.2px;
  text-shadow: none;
  line-height: 1;
  width: 100%;
  height: 100%;
  padding: 0 10px;
  text-align: center;
  box-sizing: border-box;
  -webkit-font-smoothing: antialiased;
}

.collapsed-pill-text {
  display: inline-block;
  vertical-align: middle;
}

.bar:not(.bar-collapsed) .collapsed-pill {
  display: none;
}

.collapsed-pill[data-state="paused"] {
  background: #f0c44c;
  border-color: #f3d16f;
}

.collapsed-pill[data-state="idle"] {
  background: #44c277;
  border-color: #68d093;
}

.collapsed-pill[data-state="paused"] .collapsed-pill-content {
  color: #3a2500;
  text-shadow: none;
}

.collapsed-pill[data-state="stopping"],
.collapsed-pill[data-state="starting"] {
  background: #7eb6de;
  border-color: #9cc8e8;
}

.collapsed-pill[data-state="error"] {
  background: #de7373;
  border-color: #e79a9a;
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

.action-loading-icon {
  animation: pause-loading-spin 1s linear infinite;
}

.action-icon-slot {
  width: 18px;
  height: 18px;
  position: relative;
  display: inline-block;
}

.action-icon {
  position: absolute;
  inset: 0;
  width: 18px;
  height: 18px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: opacity 0.12s linear;
}

.action-icon-hidden {
  opacity: 0;
}

@keyframes pause-loading-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
