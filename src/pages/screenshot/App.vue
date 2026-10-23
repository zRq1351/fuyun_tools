<template>
  <div
    :class="['fy-editor', editorCursorClass]"
    @mousedown="onMouseDown"
    @mousemove="onMouseMove"
    @mouseup="onMouseUp"
    @wheel.prevent="onMouseWheel"
    @contextmenu.prevent="onContextMenu"
  >
    <!-- 底层截图 -->
    <img
      v-if="screenshotSrc && !longshotOverlayOnly"
      :src="screenshotSrc"
      :class="['bg-image', { 'longshot-view-bg': longshotResultActive }]"
      :style="sceneLayerStyle"
      draggable="false"
    >

    <!-- 绘制层 (全屏尺寸，缩放适配高DPI) -->
    <canvas
      v-show="!longshotOverlayOnly"
      ref="canvas"
      :style="sceneLayerStyle"
      class="draw-canvas"
    />

    <!-- 遮罩与选区层 -->
    <div
      :class="{ 'pointer-none': state === 'drawing', 'longshot-overlay-only': longshotOverlayOnly }"
      :style="sceneLayerStyle"
      class="mask-layer"
    >
      <!-- 智能窗口高亮 -->
      <div
        v-if="highlightedWindow && state === 'idle'"
        ref="windowHighlightRef"
        class="window-highlight"
      >
        <div class="window-border" />
      </div>

      <!-- 选区镂空及控制点 -->
      <div
        v-if="hasSelection || state === 'selecting' || state === 'idle'"
        ref="cutoutRef"
        :class="{ 'is-active': state === 'selected', 'longshot-running': longshotOverlayOnly, 'picker-active': currentTool === 'picker' }"
        class="cutout"
      >
        <div
          :class="{ 'is-active': state !== 'drawing' }"
          class="cutout-border"
        />

        <div
          v-if="(state === 'selecting' || state === 'resizing' || state === 'moving' || state === 'selected') && !longshotOverlayOnly"
          :style="sizeInfoStyle"
          class="size-info"
        >
          {{ selectionInfoText }}
        </div>

        <!-- 8个调整控制点 -->
        <template v-if="hasSelection && state !== 'drawing' && currentTool === 'select' && !longshotOverlayOnly">
          <div
            class="handle tl"
            @mousedown.stop="startResize('tl', $event)"
          />
          <div
            class="handle tm"
            @mousedown.stop="startResize('tm', $event)"
          />
          <div
            class="handle tr"
            @mousedown.stop="startResize('tr', $event)"
          />
          <div
            class="handle ml"
            @mousedown.stop="startResize('ml', $event)"
          />
          <div
            class="handle mr"
            @mousedown.stop="startResize('mr', $event)"
          />
          <div
            class="handle bl"
            @mousedown.stop="startResize('bl', $event)"
          />
          <div
            class="handle bm"
            @mousedown.stop="startResize('bm', $event)"
          />
          <div
            class="handle br"
            @mousedown.stop="startResize('br', $event)"
          />
        </template>
      </div>
    </div>

    <div
      v-if="regionSelectMode === 'recording_region' && hasSelection && state === 'selected'"
      :style="recordingConfirmStyle"
      class="recording-region-confirm"
      @mousedown.stop
    >
      <button
        :title="t('screenshot.confirmRegion')"
        class="region-icon-btn primary"
        @click.stop="commitRecordingRegionSelection"
      >
        <Check class="tool-icon-wrap" />
      </button>
      <button
        :title="t('screenshot.reselect')"
        class="region-icon-btn"
        @click.stop="cancelSelection"
      >
        <RotateCcw class="tool-icon-wrap" />
      </button>
      <button
        :title="t('screenshot.cancel')"
        class="region-icon-btn danger"
        @click.stop="close"
      >
        <X class="tool-icon-wrap" />
      </button>
    </div>

    <div
      v-if="regionSelectMode === 'manual_longshot' && hasSelection && state === 'selected' && !manualLongshotRunning"
      :style="recordingConfirmStyle"
      class="recording-region-confirm"
      @mousedown.stop
    >
      <button
        class="region-icon-btn primary"
        :title="manualLongshotRunning ? t('screenshot.pauseLongshot') : t('screenshot.startLongshot')"
        @click.stop="toggleManualLongshotRunning"
      >
        <Pause
          v-if="manualLongshotRunning"
          :size="12"
        />
        <Play
          v-else
          :size="12"
        />
      </button>
      <button
        class="region-icon-btn"
        :title="t('screenshot.finishLongshot')"
        :disabled="!manualLongshotSessionId"
        @click.stop="finishManualLongshotCapture"
      >
        <Check class="tool-icon-wrap" />
      </button>
      <button
        :title="t('screenshot.reselect')"
        class="region-icon-btn"
        @click.stop="cancelSelection"
      >
        <RotateCcw class="tool-icon-wrap" />
      </button>
      <button
        :title="t('screenshot.cancelLongshot')"
        class="region-icon-btn danger"
        @click.stop="cancelManualLongshotCapture(true)"
      >
        <X class="tool-icon-wrap" />
      </button>
    </div>

    <div
      v-if="regionSelectMode === 'manual_longshot' && hasSelection && state === 'selected' && manualLongshotHint"
      class="manual-longshot-hint"
    >
      {{ manualLongshotHint }}
    </div>

    <div
      v-if="captureReadyError"
      class="capture-ready-error-toast"
      @mousedown.stop
    >
      {{ captureReadyError }}
    </div>

    <div
      v-if="showExportRouteIndicator"
      :class="['export-route-indicator', exportRouteIndicator.kind]"
      @mousedown.stop
    >
      <div class="export-route-title">
        {{ t('screenshot.currentLinkStatus') }}
      </div>
      <div
        v-for="item in exportRouteIndicator.items"
        :key="item.key"
        class="export-route-detail"
      >
        {{ item.label }}：{{ item.value }}
      </div>
    </div>

    <!-- 浮动工具栏 -->
    <div
      v-if="regionSelectMode === 'screenshot' && hasSelection && (state === 'selected' || state === 'drawing')"
      ref="floatingToolbarRef"
      class="floating-toolbar"
      @mousedown.stop
    >
      <div class="tools-row primary-tools">
        <button
          v-for="tool in drawingTools"
          :key="tool.id"
          :class="{ active: currentTool === tool.id }"
          :title="t(tool.name)"
          class="tool-btn"
          @click="setTool(tool.id)"
        >
          <component
            :is="tool.icon"
            class="tool-icon-wrap"
          />
        </button>

        <div class="divider" />
        <button
          :disabled="manualLongshotAvailability.status !== 'available' && manualLongshotAvailability.status !== 'busy'"
          :title="manualLongshotButtonTitle"
          class="tool-btn"
          @click="enterManualLongshotMode"
        >
          <svg
            aria-hidden="true"
            class="tool-icon-wrap"
            fill="none"
            viewBox="0 0 16 16"
          >
            <path
              d="M8 1.5 L6.2 3.3 M8 1.5 L9.8 3.3 M8 1.5 V5"
              stroke="currentColor"
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.4"
            />
            <rect
              height="7"
              rx="1.6"
              stroke="currentColor"
              stroke-width="1.4"
              width="8"
              x="4"
              y="4.5"
            />
            <path
              d="M8 14.5 L6.2 12.7 M8 14.5 L9.8 12.7 M8 11 V14.5"
              stroke="currentColor"
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.4"
            />
          </svg>
        </button>

        <div class="divider" />

        <button
          :disabled="!canCropToSelection"
          :title="t('screenshot.cropToSelection')"
          class="tool-btn"
          @click="applyCropToSelection"
        >
          <Crop class="tool-icon-wrap" />
        </button>

        <button
          :disabled="historyIndex <= 0"
          :title="t('screenshot.undo')"
          class="tool-btn"
          @click="undo"
        >
          <Undo2 class="tool-icon-wrap" />
        </button>
        <button
          :disabled="historyIndex >= history.length - 1"
          :title="t('screenshot.redo')"
          class="tool-btn"
          @click="redo"
        >
          <Redo2 class="tool-icon-wrap" />
        </button>

        <div class="divider" />

        <button
          :disabled="!canExport"
          :title="t('screenshot.copyToClipboard')"
          class="tool-btn"
          @click="copyToClipboardLinked"
        >
          <Copy class="tool-icon-wrap" />
        </button>
        <button
          :disabled="!canExport"
          :title="t('screenshot.saveFile')"
          class="tool-btn"
          @click="saveAndClose"
        >
          <Download class="tool-icon-wrap" />
        </button>
        <button
          :disabled="!canExport"
          :title="t('screenshot.pinToScreen')"
          class="tool-btn"
          @click="pinToScreenAndClose"
        >
          <Pin class="tool-icon-wrap" />
        </button>
        <button
          :title="t('screenshot.cancelSelection')"
          class="tool-btn danger"
          @click="cancelSelection"
        >
          <X class="tool-icon-wrap" />
        </button>
        <button
          :disabled="!canExport"
          :title="t('screenshot.finishAndCopy')"
          class="tool-btn confirm"
          @click="completeAndCopyUnlinked"
        >
          <Check class="tool-icon-wrap" />
        </button>
      </div>

      <!-- 二级属性栏：绘图工具激活，或选中可填充形状时 -->
      <div
        v-if="showSecondaryTools"
        class="tools-row secondary-tools"
      >
        <input
          v-model="currentColor"
          :title="t('screenshot.textColor')"
          class="color-picker"
          type="color"
          @input="syncEditingTextStyle"
        >
        <input
          v-model="lineWidth"
          :title="t('screenshot.lineWidth')"
          class="line-slider"
          max="20"
          min="1"
          type="range"
          @input="syncEditingTextStyle"
        >
        <template v-if="showMosaicSizeControl">
          <input
            v-model="mosaicSize"
            :title="t('screenshot.mosaicSize')"
            class="line-slider"
            max="24"
            min="2"
            type="range"
          >
        </template>
        <template v-if="showShapeFillControls">
          <button
            :class="{ active: fillControlsActive }"
            :title="t('screenshot.fill')"
            class="tool-btn mini"
            @click="toggleShapeFill"
          >
            {{ t('screenshot.fill') }}
          </button>
          <input
            v-if="fillControlsActive"
            v-model="shapeFillOpacity"
            :title="t('screenshot.fillOpacity')"
            class="line-slider"
            max="1"
            min="0.05"
            step="0.05"
            type="range"
            @change="onShapeFillOpacityInput"
          >
        </template>
        <template v-if="currentTool === 'text'">
          <select
            v-model="textStyle.fontFamily"
            :title="t('screenshot.font')"
            class="text-style-select"
            @change="syncEditingTextStyle"
          >
            <option
              v-for="font in fontFamilies"
              :key="font"
              :value="font"
            >
              {{ font }}
            </option>
          </select>
          <button
            :class="{ active: textStyle.bold }"
            :title="t('screenshot.bold')"
            class="tool-btn mini"
            @click="toggleTextBold"
          >
            <Bold :size="12" />
          </button>
          <button
            :class="{ active: textStyle.stroke }"
            :title="t('screenshot.stroke')"
            class="tool-btn mini"
            @click="toggleTextStroke"
          >
            <PenLine :size="12" />
          </button>
          <input
            v-if="textStyle.stroke"
            v-model="textStyle.strokeColor"
            :title="t('screenshot.strokeColor')"
            class="color-picker mini-picker"
            type="color"
            @input="syncEditingTextStyle"
          >
          <button
            :class="{ active: textStyle.shadow }"
            :title="t('screenshot.shadow')"
            class="tool-btn mini"
            @click="toggleTextShadow"
          >
            <Layers :size="12" />
          </button>
        </template>
      </div>
    </div>

    <!-- 取色器放大镜 -->
    <div
      v-if="currentTool === 'picker' && pickColor"
      ref="pickerInfoRef"
      class="color-picker-info"
    >
      <canvas
        ref="pickerMagnifierCanvasRef"
        class="picker-magnifier"
      />
      <div class="picker-meta">
        <div
          :style="{ backgroundColor: pickColor }"
          class="picker-swatch"
        />
        <div class="picker-value">
          {{ pickerDisplayValue }}
        </div>
        <div class="picker-hint">
          {{ pickerCopyHint }}
        </div>
      </div>
    </div>

    <div
      v-for="shape in shapeItems"
      :key="`shape-${shape.id}`"
      :class="{ selected: selectedShapeId === shape.id }"
      :style="getShapeItemStyle(shape)"
      class="shape-overlay-item"
      @mousedown.stop="startDragShapeItem(shape.id, $event)"
    >
      <template v-if="shape.type === 'rect' || shape.type === 'redact'">
        <div
          :style="getShapeFillStyle(shape)"
          class="shape-fill"
        />
        <div
          v-if="shape.type !== 'redact'"
          :style="getShapeStrokeStyle(shape)"
          class="shape-rect"
        />
      </template>
      <template v-else-if="shape.type === 'circle'">
        <div
          :style="getShapeFillStyle(shape)"
          class="shape-fill shape-fill-circle"
        />
        <div
          :style="getShapeStrokeStyle(shape)"
          class="shape-circle"
        />
      </template>
      <template v-else-if="shape.type === 'number'">
        <div
          :style="getShapeNumberStyle(shape)"
          class="shape-number"
        >
          {{ shape.n }}
        </div>
      </template>
      <template v-else>
        <svg
          :viewBox="getShapeLineViewBox(shape)"
          class="shape-line-svg"
          preserveAspectRatio="none"
        >
          <line
            :stroke="shape.color"
            :stroke-width="shape.lineWidth"
            :x1="shape.x1"
            :x2="shape.x2"
            :y1="shape.y1"
            :y2="shape.y2"
            stroke-linecap="round"
          />
          <template v-if="shape.type === 'arrow'">
            <line
              :stroke="shape.color"
              :stroke-width="shape.lineWidth"
              :x1="shape.x2"
              :x2="shape.arrowLeft.x"
              :y1="shape.y2"
              :y2="shape.arrowLeft.y"
              stroke-linecap="round"
            />
            <line
              :stroke="shape.color"
              :stroke-width="shape.lineWidth"
              :x1="shape.x2"
              :x2="shape.arrowRight.x"
              :y1="shape.y2"
              :y2="shape.arrowRight.y"
              stroke-linecap="round"
            />
          </template>
        </svg>
      </template>
      <template v-if="selectedShapeId === shape.id && (shape.type === 'rect' || shape.type === 'circle' || shape.type === 'number' || shape.type === 'redact')">
        <div
          class="shape-resize-handle tl"
          @mousedown.stop="startResizeShapeItem(shape.id, 'tl', $event)"
        />
        <div
          class="shape-resize-handle tm"
          @mousedown.stop="startResizeShapeItem(shape.id, 'tm', $event)"
        />
        <div
          class="shape-resize-handle tr"
          @mousedown.stop="startResizeShapeItem(shape.id, 'tr', $event)"
        />
        <div
          class="shape-resize-handle ml"
          @mousedown.stop="startResizeShapeItem(shape.id, 'ml', $event)"
        />
        <div
          class="shape-resize-handle mr"
          @mousedown.stop="startResizeShapeItem(shape.id, 'mr', $event)"
        />
        <div
          class="shape-resize-handle bl"
          @mousedown.stop="startResizeShapeItem(shape.id, 'bl', $event)"
        />
        <div
          class="shape-resize-handle bm"
          @mousedown.stop="startResizeShapeItem(shape.id, 'bm', $event)"
        />
        <div
          class="shape-resize-handle br"
          @mousedown.stop="startResizeShapeItem(shape.id, 'br', $event)"
        />
      </template>
      <template v-if="selectedShapeId === shape.id && (shape.type === 'line' || shape.type === 'arrow')">
        <div
          :style="getShapePointHandleStyle(shape.x1, shape.y1)"
          class="shape-point-handle"
          @mousedown.stop="startAdjustLineEndpoint(shape.id, 'start', $event)"
        />
        <div
          :style="getShapePointHandleStyle(shape.x2, shape.y2)"
          class="shape-point-handle"
          @mousedown.stop="startAdjustLineEndpoint(shape.id, 'end', $event)"
        />
      </template>
    </div>

    <div
      v-for="item in textItems"
      :key="item.id"
      :ref="(el) => setTextOverlayRef(el, item.id)"
      :class="{ editing: editingTextId === item.id, selected: selectedTextId === item.id }"
      class="text-overlay-item"
      @mousedown.stop="startDragTextItem(item.id, $event)"
      @dblclick.stop="startEditTextItem(item)"
    >
      <div
        v-if="editingTextId === item.id"
        :ref="setEditingElementRef"
        class="text-inline-editor"
        contenteditable="plaintext-only"
        @blur="finishInlineEdit"
        @input="onInlineTextInput(item, $event)"
        @keydown.stop="handleInlineEditorKeydown($event)"
      />
      <template v-else>
        <div
          v-for="(line, index) in item.text.split('\n')"
          :key="`${item.id}-${index}`"
        >
          {{ line }}
        </div>
      </template>
    </div>
  </div>
</template>

<script setup>
import {IPC_COMMANDS} from '../../services/ipc.js'
import {nextTick, onMounted, onUnmounted, reactive, ref, watchPostEffect} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {listen} from '@tauri-apps/api/event'
import {
  Bold,
  Check,
  Copy,
  Crop,
  Download,
  Layers,
  PenLine,
  Pin,
  Pause,
  Play,
  Redo2,
  RotateCcw,
  Undo2,
  X
} from 'lucide-vue-next'
import {useI18n} from 'vue-i18n'
import {parseErrorMessage} from '@/utils/errorHandler.js'
import {
  computeMosaicBlockParams,
  constrainLinePoint,
  constrainRectPoint,
  hexToRgba,
  resolveExportShapeFill
} from '../../utils/screenshotAnnotation'
import {useAnnotationTools} from './composables/useAnnotationTools'
import {useSelectionRect} from './composables/useSelectionRect'
import {useScreenshotCapture} from './composables/useScreenshotCapture'
import {useManualLongshot} from './composables/useManualLongshot'
import {useColorPicker} from './composables/useColorPicker'

const {t} = useI18n()

let captureReadyErrorTimer = null

function showCaptureError(msg) {
  captureReadyError.value = msg
  if (captureReadyErrorTimer) {
    clearTimeout(captureReadyErrorTimer)
  }
  captureReadyErrorTimer = setTimeout(() => {
    captureReadyError.value = ''
    captureReadyErrorTimer = null
  }, 3000)
}

let currentDrawingSnapshot = null

// 历史记录
const history = ref([])
const historyIndex = ref(-1)
let overlayCommandLog = []
let vectorCommandLog = []
let activeRasterCommand = null
const regionConfirmAnchor = reactive({x: 0, y: 0, ready: false})

const captureOriginX = ref(0)
const captureOriginY = ref(0)
const hasScreenshotPayload = ref(false)
const screenshotSessionRequested = ref(false)
const activeSessionId = ref(0)
const payloadSessionId = ref(0)
const regionSelectMode = ref('screenshot')
const screenshotRequestInFlight = ref(false)
let screenshotRequestPromise = null
const fallbackRequestedSessionIds = new Set()
let fallbackRequestedWithoutSession = false
let screenshotFallbackTimer = null
const manualLongshotSessionId = ref(0)
const manualLongshotRunning = ref(false)
const longshotResultActive = ref(false)
const longshotRawPngBase64 = ref('')
const sourceImagePath = ref('')
const overlayDirty = ref(false)
const longshotViewScale = ref(1)
const longshotViewOffset = reactive({x: 0, y: 0})
let unlistenManualLongshotProgress = null
let unlistenManualLongshotLifecycle = null
let unlistenManualLongshotFirstFrame = null
let unlistenSessionReady = null
let unlistenManualLongshotShortcutFinished = null
let unlistenManualLongshotShortcutCanceled = null
let unlistenManualLongshotShortcutPaused = null
let unlistenManualLongshotShortcutResumed = null
let manualLongshotAvailabilityRefreshTimer = null

// 物理像素比例
const dpr = window.devicePixelRatio || 1

const isDevMode = import.meta.env.DEV

// ========== 按域抽取的 composables ==========
const {
  currentTool,
  currentColor,
  lineWidth,
  shapeFillEnabled,
  shapeFillOpacity,
  mosaicSize,
  NUMBER_DEFAULT_RADIUS,
  NUMBER_MIN_RADIUS,
  SHAPE_TOOLS,
  textItems,
  shapeItems,
  editingTextId,
  selectedTextId,
  selectedShapeId,
  editingElementRef,
  showShapeFillControls,
  showMosaicSizeControl,
  showSecondaryTools,
  fillControlsActive,
  textOverlayRefMap,
  editingBeforeItem,
  fontFamilies,
  textStyle,
  drawingTools,
  setTextOverlayRef,
  toggleShapeFill,
  onShapeFillOpacityInput,
  nextNumberCalloutIndex,
  startCreateTextItem,
  startEditTextItem,
  setEditingElementRef,
  onInlineTextInput,
  handleInlineEditorKeydown,
  finishInlineEdit,
  cancelInlineEdit,
  syncFillControlsFromShape,
  handleResizeShapeItem,
  handleAdjustLineEndpoint,
  createShapeItem,
  getShapeItemStyle,
  getShapeStrokeStyle,
  getShapeFillStyle,
  getShapeNumberStyle,
  getShapeLineViewBox,
  getShapePointHandleStyle,
  toggleTextBold,
  toggleTextStroke,
  toggleTextShadow,
  syncEditingTextStyle,
  getTextItemStyle,
  cloneTextItem,
  cloneShapeItem,
  areFlatItemsEqual,
  drawArrowHead,
  movingTextStart,
  movingShapeStart,
  resizingShapeStart,
  adjustingLinePointStart
} = useAnnotationTools({
  toScenePoint,
  longshotResultActive,
  longshotViewScale,
  longshotViewOffset,
  getTextCreationBounds,
  appendVectorUpsertCommand,
  appendVectorDeleteCommand,
  saveToHistory
})

const {
  state,
  rect,
  startRect,
  startPoint,
  drawStart,
  isDrawing,
  windowHighlightRef,
  cutoutRef,
  floatingToolbarRef,
  highlightedWindow,
  hasSelection,
  cutoutStyle,
  sizeInfoStyle,
  selectionInfoText,
  recordingConfirmStyle,
  windowHighlightStyle,
  toolbarStyle,
  adjustToolbarPosition,
  editorCursorClass,
  isInside,
  isInsideBorder,
  detectWindowAt,
  fetchWindows,
  startResize,
  handleResize
} = useSelectionRect({
  toScenePoint,
  dpr,
  regionConfirmAnchor,
  currentTool,
  longshotResultActive,
  captureOriginX,
  captureOriginY,
  onMouseDown
})

const {
  screenshotSrc,
  screenshotImg,
  canvas,
  isCaptureReady,
  captureReadyError,
  canExport,
  canCropToSelection,
  showExportRouteIndicator,
  exportRouteIndicator,
  revokeScreenshotObjectUrl,
  loadScreenshotSession,
  initCanvas,
  loadImageFromBase64,
  loadImageFromPath,
  getGlobalSelectionRect,
  getLongshotImageViewportRect,
  getLongshotFullCanvas,
  sceneToImagePoint,
  drawShapeItemsOnCroppedCanvas,
  drawTextItemsOnCroppedCanvas,
  copyToClipboardLinked,
  completeAndCopyUnlinked,
  pinToScreenAndClose,
  saveAndClose,
  commitRecordingRegionSelection,
  onContextMenu,
  resolveCropDataUrl
} = useScreenshotCapture({
  t,
  dpr,
  isDevMode,
  sourceImagePath,
  longshotResultActive,
  longshotRawPngBase64,
  longshotViewScale,
  longshotViewOffset,
  overlayDirty,
  regionSelectMode,
  manualLongshotSessionId,
  manualLongshotRunning,
  captureOriginX,
  captureOriginY,
  activeSessionId,
  payloadSessionId,
  screenshotSessionRequested,
  hasScreenshotPayload,
  fetchWindows,
  state,
  rect,
  currentTool,
  textItems,
  shapeItems,
  hasSelection,
  SHAPE_TOOLS,
  getCroppedCanvas,
  buildBackendExportRequest,
  requestScreenshot,
  showCaptureError,
  saveToHistory,
  isPixelCtxReadable,
  loadScreenshotPixelFromBlob,
  loadImageFromSrc,
  close
})

const {
  manualLongshotAvailability,
  manualLongshotPhase,
  manualLongshotHint,
  longshotOverlayOnly,
  pendingLongshotBorderAnchor,
  longshotBorderShown,
  refreshManualLongshotAvailability,
  cleanupManualLongshotUi,
  applyManualLongshotPhase,
  toggleManualLongshotRunning,
  finishManualLongshotCapture,
  cancelManualLongshotCapture,
  applyManualLongshotResult,
  sceneLayerStyle,
  onMouseWheel,
  manualLongshotButtonTitle
} = useManualLongshot({
  t,
  regionSelectMode,
  sourceImagePath,
  longshotResultActive,
  longshotRawPngBase64,
  longshotViewScale,
  longshotViewOffset,
  manualLongshotSessionId,
  manualLongshotRunning,
  state,
  rect,
  scheduleManualLongshotAvailabilityRefresh,
  getGlobalSelectionRect,
  loadImageFromBase64,
  loadImageFromPath
})

const {
  pickColor,
  pickColorRgb,
  pickerDisplayMode,
  pickerCopyHint,
  pickerInfoRef,
  pickerMagnifierCanvasRef,
  pickerDisplayValue,
  pickerStyle,
  pickColorAtScene,
  copyPickedColor
} = useColorPicker({
  t,
  dpr,
  drawStart,
  isDrawing,
  currentColor,
  currentTool,
  toScenePoint,
  getMergedPixelColorAt,
  renderPickerMagnifier
})

async function applyCropToSelection() {
  if (!canCropToSelection.value) {
    showCaptureError(t('screenshot.cropUnavailable'))
    return
  }
  try {
    const {dataUrl, cssW, cssH} = await resolveCropDataUrl()
    sourceImagePath.value = ''
    screenshotSrc.value = dataUrl
    const img = new Image()
    await new Promise((resolve, reject) => {
      img.onload = resolve
      img.onerror = reject
      img.src = dataUrl
    })
    screenshotImg.value = img
    screenshotPixelCanvas = document.createElement('canvas')
    screenshotPixelCanvas.width = img.width
    screenshotPixelCanvas.height = img.height
    screenshotPixelCtx = screenshotPixelCanvas.getContext('2d')
    if (screenshotPixelCtx) {
      try {
        screenshotPixelCtx.drawImage(img, 0, 0)
        screenshotPixelCtx.getImageData(0, 0, 1, 1)
      } catch (_) { /* ignore */ }
    }
    resetAnnotationStateForNewImage()
    await nextTick()
    initCanvas()
    rect.x = 0
    rect.y = 0
    rect.width = cssW
    rect.height = cssH
    state.value = 'selected'
    currentTool.value = 'select'
    isCaptureReady.value = true
  } catch (err) {
    console.error('裁剪失败:', err)
    showCaptureError(String(err?.message || err || t('screenshot.cropUnavailable')))
  }
}

function scheduleManualLongshotAvailabilityRefresh(delay = 180) {
  if (manualLongshotAvailabilityRefreshTimer) {
    window.clearTimeout(manualLongshotAvailabilityRefreshTimer)
    manualLongshotAvailabilityRefreshTimer = null
  }
  manualLongshotAvailabilityRefreshTimer = window.setTimeout(async () => {
    manualLongshotAvailabilityRefreshTimer = null
    await refreshManualLongshotAvailability()
  }, delay)
}

let screenshotPixelCanvas = null
let screenshotPixelCtx = null
/** 无 CORS 污染的位图源（blob/createImageBitmap），马赛克/取色采样用 */
let screenshotBitmap = null
let screenshotPixelBlobLoadPromise = null
let movingTextBeforeItem = null
let movingShapeBeforeItem = null
let resizingShapeBeforeItem = null
let adjustingLineBeforeItem = null

function toScenePoint(event) {
  if (!longshotResultActive.value) {
    return {x: event.clientX, y: event.clientY}
  }
  const scale = Math.max(0.1, longshotViewScale.value)
  return {
    x: (event.clientX - longshotViewOffset.x) / scale,
    y: (event.clientY - longshotViewOffset.y) / scale
  }
}

function sceneToDisplayPoint(x, y) {
  if (!longshotResultActive.value) {
    return {x, y}
  }
  const scale = Math.max(0.1, longshotViewScale.value)
  return {
    x: x * scale + longshotViewOffset.x,
    y: y * scale + longshotViewOffset.y
  }
}

function applyStyleObject(el, styleObj) {
  if (!el || !styleObj) return
  Object.entries(styleObj).forEach(([key, value]) => {
    el.style[key] = value
  })
}

watchPostEffect(() => {
  applyStyleObject(windowHighlightRef.value, windowHighlightStyle.value)
  applyStyleObject(cutoutRef.value, cutoutStyle.value)
  applyStyleObject(floatingToolbarRef.value, toolbarStyle.value)
  applyStyleObject(pickerInfoRef.value, pickerStyle.value)
  const textItemById = new Map(textItems.value.map((item) => [item.id, item]))
  for (const [id, el] of textOverlayRefMap.entries()) {
    const item = textItemById.get(id)
    if (!item) {
      textOverlayRefMap.delete(id)
      continue
    }
    applyStyleObject(el, getTextItemStyle(item))
  }
  // 调整工具栏位置，确保不超出屏幕边界
  nextTick(() => {
    adjustToolbarPosition()
  })
})

// 初始化与通信
onMounted(async () => {
  window.__SCREENSHOT_KEYDOWN_HANDLER_READY__ = true
  await refreshManualLongshotAvailability()
  // 预创建窗口后，热键截图通过 Tauri 事件触发重新加载
  unlistenSessionReady = await listen('screenshot-session-ready', () => {
    loadScreenshotSession()
  })
  window.addEventListener('screenshot-reset', handleScreenshotReset)
  document.addEventListener('keydown', handleKeyDown)
  window.addEventListener('mouseup', onGlobalMouseUp)
  window.addEventListener('blur', onWindowBlur)
  unlistenManualLongshotProgress = await listen('manual-longshot-progress', (event) => {
    const payload = event.payload || {}
    const sessionId = Number(payload.sessionId || 0)
    if (manualLongshotSessionId.value > 0 && sessionId !== manualLongshotSessionId.value) return
    applyManualLongshotPhase(payload)
    const stitchedHeight = Number(payload.stitchedHeight || 0)
    const frameCount = Number(payload.frameCount || 0)
    const dropped = Number(payload.droppedFrames || 0)
    const confidence = Number(payload.lastConfidence || 0)
    manualLongshotHint.value = t('screenshot.longshotProgress', {
      height: stitchedHeight,
      frame: frameCount,
      dropped,
      confidence: confidence.toFixed(2)
    })
    if (!longshotBorderShown.value && frameCount >= 1 && pendingLongshotBorderAnchor.value) {
      invoke(IPC_COMMANDS.SHOW_LONGSHOT_BORDER, {anchor: pendingLongshotBorderAnchor.value}).catch(() => {
      })
      longshotBorderShown.value = true
    }
  })
  unlistenManualLongshotFirstFrame = await listen('manual-longshot-first-frame', (event) => {
    const payload = event.payload || {}
    const sessionId = Number(payload.sessionId || 0)
    if (manualLongshotSessionId.value > 0 && sessionId !== manualLongshotSessionId.value) return
    applyManualLongshotPhase(payload)
    if (pendingLongshotBorderAnchor.value && !longshotBorderShown.value) {
      invoke(IPC_COMMANDS.SHOW_LONGSHOT_BORDER, {anchor: pendingLongshotBorderAnchor.value}).catch(() => {
      })
      longshotBorderShown.value = true
    }
  })
  unlistenManualLongshotLifecycle = await listen('manual-longshot-lifecycle', (event) => {
    const payload = event.payload || {}
    const sessionId = Number(payload.sessionId || 0)
    if (manualLongshotSessionId.value > 0 && sessionId !== manualLongshotSessionId.value) return
    const stateName = String(payload.state || '')
    applyManualLongshotPhase(payload)
    if (stateName === 'started' || stateName === 'resumed') {
      if (!manualLongshotHint.value) {
        manualLongshotHint.value = t('screenshot.longshotManualScroll')
      }
    } else if (stateName === 'canceled') {
      manualLongshotPhase.value = 'canceling'
      cleanupManualLongshotUi(true)
      manualLongshotHint.value = t('screenshot.longshotCancelled')
    } else if (stateName === 'failed' || stateName === 'error') {
      manualLongshotPhase.value = 'failed'
      cleanupManualLongshotUi(true)
      const errMsg = parseErrorMessage(payload.userMessage || payload.message || '')
      manualLongshotHint.value = String(errMsg || t('screenshot.longshotFailed', {error: String(payload.message || '')}))
    } else if (stateName === 'ended') {
      scheduleManualLongshotAvailabilityRefresh()
    }
  })
  unlistenManualLongshotShortcutFinished = await listen('manual-longshot-shortcut-finished', (event) => {
    const payload = event.payload || {}
    try {
      applyManualLongshotResult(payload)
    } catch (error) {
      manualLongshotHint.value = t('screenshot.longshotFinishFailed', {error: String(error)})
    }
  })
  unlistenManualLongshotShortcutCanceled = await listen('manual-longshot-shortcut-canceled', () => {
    manualLongshotPhase.value = 'canceling'
    cleanupManualLongshotUi(true)
    manualLongshotHint.value = t('screenshot.longshotCancelled')
  })
  unlistenManualLongshotShortcutPaused = await listen('manual-longshot-shortcut-paused', () => {
    manualLongshotPhase.value = 'paused'
    manualLongshotRunning.value = false
    manualLongshotHint.value = t('screenshot.longshotPausedHint')
  })
  unlistenManualLongshotShortcutResumed = await listen('manual-longshot-shortcut-resumed', () => {
    manualLongshotPhase.value = 'running'
    manualLongshotRunning.value = true
    manualLongshotHint.value = t('screenshot.longshotResumeHint')
  })
  await loadScreenshotSession()
})

onUnmounted(() => {
  delete window.__SCREENSHOT_KEYDOWN_HANDLER_READY__
  if (captureReadyErrorTimer) {
    clearTimeout(captureReadyErrorTimer)
    captureReadyErrorTimer = null
  }
  revokeScreenshotObjectUrl()
  cancelManualLongshotCapture(false, false)
  window.removeEventListener('screenshot-reset', handleScreenshotReset)
  document.removeEventListener('keydown', handleKeyDown)
  if (typeof unlistenManualLongshotProgress === 'function') {
    unlistenManualLongshotProgress()
    unlistenManualLongshotProgress = null
  }
  if (typeof unlistenManualLongshotLifecycle === 'function') {
    unlistenManualLongshotLifecycle()
    unlistenManualLongshotLifecycle = null
  }
  if (typeof unlistenManualLongshotFirstFrame === 'function') {
    unlistenManualLongshotFirstFrame()
    unlistenManualLongshotFirstFrame = null
  }
  if (typeof unlistenManualLongshotShortcutFinished === 'function') {
    unlistenManualLongshotShortcutFinished()
    unlistenManualLongshotShortcutFinished = null
  }
  if (typeof unlistenManualLongshotShortcutCanceled === 'function') {
    unlistenManualLongshotShortcutCanceled()
    unlistenManualLongshotShortcutCanceled = null
  }
  if (typeof unlistenManualLongshotShortcutPaused === 'function') {
    unlistenManualLongshotShortcutPaused()
    unlistenManualLongshotShortcutPaused = null
  }
  if (typeof unlistenManualLongshotShortcutResumed === 'function') {
    unlistenManualLongshotShortcutResumed()
    unlistenManualLongshotShortcutResumed = null
  }
  if (typeof unlistenSessionReady === 'function') {
    unlistenSessionReady()
    unlistenSessionReady = null
  }
  if (screenshotFallbackTimer) {
    window.clearTimeout(screenshotFallbackTimer)
    screenshotFallbackTimer = null
  }
  if (manualLongshotAvailabilityRefreshTimer) {
    window.clearTimeout(manualLongshotAvailabilityRefreshTimer)
    manualLongshotAvailabilityRefreshTimer = null
  }
  window.removeEventListener('mouseup', onGlobalMouseUp)
  window.removeEventListener('blur', onWindowBlur)
})

function onGlobalMouseUp(e) {
  // 全局 mouseup：清理 Alt+Tab 切走后残留的绘制状态
  if (isDrawing.value) {
    handleCanvasMouseUp(e)
    state.value = 'selected'
  }
}

function onWindowBlur() {
  // 窗口失焦时结束当前绘制，防止状态残留
  if (isDrawing.value) {
    isDrawing.value = false
    currentDrawingSnapshot = null
    activeRasterCommand = null
    state.value = 'selected'
  }
}

function handleScreenshotReset() {
  cancelManualLongshotCapture(false)
  screenshotSrc.value = ''
  screenshotImg.value = null
  isCaptureReady.value = false
  highlightedWindow.value = null
  state.value = 'idle'
  currentTool.value = 'select'
  textItems.value = []
  shapeItems.value = []
  selectedTextId.value = null
  selectedShapeId.value = null
  editingTextId.value = null
  rect.x = 0
  rect.y = 0
  rect.width = 0
  rect.height = 0
  if (canvas.value) {
    canvas.value.width = 0
    canvas.value.height = 0
  }
  pickColor.value = ''
  pickColorRgb.value = ''
  pickerCopyHint.value = t('screenshot.pickerHint')
  screenshotPixelCanvas = null
  screenshotPixelCtx = null
  if (screenshotBitmap) {
    try {
      screenshotBitmap.close()
    } catch (_) {
      /* ignore */
    }
  }
  screenshotBitmap = null
  screenshotPixelCtx = null
  screenshotRequestInFlight.value = false
  screenshotRequestPromise = null
  fallbackRequestedSessionIds.clear()
  fallbackRequestedWithoutSession = false
  manualLongshotSessionId.value = 0
  manualLongshotRunning.value = false
  manualLongshotPhase.value = 'idle'
  manualLongshotHint.value = ''
  longshotResultActive.value = false
  longshotRawPngBase64.value = ''
  overlayDirty.value = false
  overlayCommandLog = []
  vectorCommandLog = []
  activeRasterCommand = null
  editingBeforeItem.value = null
  movingTextBeforeItem = null
  movingShapeBeforeItem = null
  resizingShapeBeforeItem = null
  adjustingLineBeforeItem = null
  longshotViewScale.value = 1
  longshotViewOffset.x = 0
  longshotViewOffset.y = 0
}

function scheduleScreenshotFallback() {
  if (!screenshotSessionRequested.value || hasScreenshotPayload.value) return
  if (screenshotFallbackTimer) {
    window.clearTimeout(screenshotFallbackTimer)
  }
  const scheduledSessionId = activeSessionId.value
  screenshotFallbackTimer = window.setTimeout(async () => {
    screenshotFallbackTimer = null
    if (scheduledSessionId > 0 && activeSessionId.value !== scheduledSessionId) {
      return
    }
    if (screenshotSessionRequested.value && !hasScreenshotPayload.value) {
      if (scheduledSessionId > 0) {
        if (fallbackRequestedSessionIds.has(scheduledSessionId)) {
          return
        }
        fallbackRequestedSessionIds.add(scheduledSessionId)
      } else {
        if (fallbackRequestedWithoutSession) {
          return
        }
        fallbackRequestedWithoutSession = true
      }
      const success = await requestScreenshot()
      if (!success) {
        if (scheduledSessionId > 0) {
          fallbackRequestedSessionIds.delete(scheduledSessionId)
        } else {
          fallbackRequestedWithoutSession = false
        }
      }
    }
  }, 120)
}

async function requestScreenshot() {
  if (screenshotRequestPromise) {
    return await screenshotRequestPromise
  }
  screenshotRequestInFlight.value = true
  screenshotRequestPromise = (async () => {
    try {
      const result = await invoke(IPC_COMMANDS.START_SCREENSHOT)
      if (result.success && (result.image_path || result.png_base64)) {
        longshotResultActive.value = false
        longshotRawPngBase64.value = ''
        longshotViewScale.value = 1
        longshotViewOffset.x = 0
        longshotViewOffset.y = 0
        sourceImagePath.value = String(result.image_path || '')
        hasScreenshotPayload.value = true
        captureOriginX.value = Number(result.origin_x) || 0
        captureOriginY.value = Number(result.origin_y) || 0
        await fetchWindows()
        if (result.png_base64) {
          loadImageFromBase64(result.png_base64)
        } else if (result.image_path) {
          loadImageFromPath(result.image_path)
        }
        return true
      }
      return false
    } catch (error) {
      console.error('请求截图失败:', error)
      return false
    } finally {
      screenshotRequestInFlight.value = false
      screenshotRequestPromise = null
    }
  })()
  return await screenshotRequestPromise
}

function handleScreenshotData(event) {
  if (event.detail && (event.detail.png_base64 || event.detail.image_path)) {
    longshotResultActive.value = false
    longshotRawPngBase64.value = ''
    sourceImagePath.value = String(event.detail.image_path || '')
    longshotViewScale.value = 1
    longshotViewOffset.x = 0
    longshotViewOffset.y = 0
    const sessionId = Number(event.detail.session_id) || 0
    if (sessionId > 0) {
      activeSessionId.value = sessionId
      payloadSessionId.value = sessionId
    }
    isCaptureReady.value = false
    screenshotSessionRequested.value = true
    hasScreenshotPayload.value = true
    if (screenshotFallbackTimer) {
      window.clearTimeout(screenshotFallbackTimer)
      screenshotFallbackTimer = null
    }
    captureOriginX.value = Number(event.detail.origin_x) || 0
    captureOriginY.value = Number(event.detail.origin_y) || 0
    fetchWindows()
    if (event.detail.png_base64) {
      loadImageFromBase64(event.detail.png_base64)
    } else if (event.detail.image_path) {
      loadImageFromPath(event.detail.image_path)
    }
  }
}

function handleStartRegionSelect(event) {
  const sessionId = Number(event?.detail?.session_id) || 0
  regionSelectMode.value = String(event?.detail?.mode || 'screenshot')
  if (sessionId > 0) {
    fallbackRequestedSessionIds.delete(sessionId)
    activeSessionId.value = sessionId
    screenshotSessionRequested.value = true
    hasScreenshotPayload.value = payloadSessionId.value === sessionId
  } else {
    fallbackRequestedWithoutSession = false
    screenshotSessionRequested.value = true
  }
  scheduleScreenshotFallback()
  state.value = 'idle'
  currentTool.value = 'select'
  rect.width = 0
  rect.height = 0
  regionConfirmAnchor.ready = false
  manualLongshotSessionId.value = 0
  manualLongshotRunning.value = false
  longshotResultActive.value = false
  longshotRawPngBase64.value = ''
  manualLongshotHint.value = regionSelectMode.value === 'manual_longshot'
      ? t('screenshot.longshotManualHint')
      : ''
}

function buildBackendExportRequest(outputPath) {
  return {
    sourceImagePath: sourceImagePath.value,
    outputPath,
    isLongshot: longshotResultActive.value,
    devicePixelRatio: dpr,
    viewportWidth: window.innerWidth,
    viewportHeight: window.innerHeight,
    selection: longshotResultActive.value ? null : {
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height
    },
    textItems: textItems.value.map((item) => ({
      x: item.x,
      y: item.y,
      text: item.text,
      color: item.color,
      fontSize: item.fontSize,
      fontFamily: item.fontFamily,
      bold: !!item.bold,
      stroke: !!item.stroke,
      strokeColor: item.strokeColor,
      shadow: !!item.shadow
    })),
    shapeItems: shapeItems.value.map((item) => {
      const fill = resolveExportShapeFill(item)
      return {
        type: item.type,
        x: item.x,
        y: item.y,
        width: item.width,
        height: item.height,
        x1: item.x1,
        y1: item.y1,
        x2: item.x2,
        y2: item.y2,
        color: item.color,
        lineWidth: item.lineWidth,
        filled: fill.filled,
        fillOpacity: fill.fillOpacity,
        number: item.type === 'number' ? Number(item.n) || 0 : undefined
      }
    }),
    overlayCommands: overlayCommandLog.map((command) => ({
      type: command.type,
      color: command.color,
      lineWidth: command.lineWidth,
      opacity: command.opacity != null ? Number(command.opacity) : undefined,
      points: Array.isArray(command.points)
          ? command.points.map((point) => ({x: point.x, y: point.y}))
          : []
    }))
  }
}

function loadImageFromSrc(src) {
  isCaptureReady.value = false
  screenshotSrc.value = src
  const img = new Image()
  // 避免跨域 canvas taint（Tauri asset:// 与 app:// 不同源）
  // asset.localhost 不支持 CORS，跳过 crossOrigin 设置
  if (src.startsWith('http') && !src.includes('asset.localhost')) {
    img.crossOrigin = 'anonymous'
  }
  img.onload = () => {
    screenshotImg.value = img
    screenshotPixelCanvas = document.createElement('canvas')
    screenshotPixelCanvas.width = img.width
    screenshotPixelCanvas.height = img.height
    screenshotPixelCtx = screenshotPixelCanvas.getContext('2d')
    let pixelReadable = false
    if (screenshotPixelCtx) {
      try {
        screenshotPixelCtx.drawImage(img, 0, 0)
        // drawImage 对 asset:// 通常不抛错，但会 taint；必须用 getImageData 探测
        screenshotPixelCtx.getImageData(0, 0, 1, 1)
        pixelReadable = true
      } catch (_) {
        pixelReadable = false
      }
    }
    try {
      resetAnnotationStateForNewImage()
    } catch (e) {
      console.error('重置标注状态失败:', e)
    }
    nextTick(() => {
      initCanvas()
      isCaptureReady.value = Boolean(canvas.value && canvas.value.width > 0 && canvas.value.height > 0)
    })
    if (!pixelReadable) {
      // 异步重建无污染像素源，不阻塞预览显示
      loadScreenshotPixelFromBlob(src, {reset: false}).catch((e) => {
        console.error('重建截图像素源失败:', e)
      })
    }
  }
  img.onerror = () => {
    console.error('截图源图加载失败, src:', (src || '').substring(0, 100))
    isCaptureReady.value = false
  }
  img.src = src
}

function isPixelCtxReadable() {
  if (!screenshotPixelCtx || !screenshotPixelCanvas) return false
  try {
    screenshotPixelCtx.getImageData(0, 0, 1, 1)
    return true
  } catch (_) {
    return false
  }
}

function readCleanPixelRegion(sx, sy, sw, sh) {
  if (sw <= 0 || sh <= 0) return null
  if (isPixelCtxReadable()) {
    try {
      return screenshotPixelCtx.getImageData(sx, sy, sw, sh)
    } catch (_) {
      /* fallthrough */
    }
  }
  if (screenshotBitmap) {
    try {
      const tmp = document.createElement('canvas')
      tmp.width = screenshotBitmap.width
      tmp.height = screenshotBitmap.height
      const tctx = tmp.getContext('2d')
      if (!tctx) return null
      tctx.drawImage(screenshotBitmap, 0, 0)
      return tctx.getImageData(sx, sy, sw, sh)
    } catch (err) {
      console.warn('从 bitmap 读取像素失败:', err)
    }
  }
  // 触发一次异步重建，后续笔触可恢复真彩色马赛克
  const src = screenshotSrc.value
  if (src && !screenshotPixelBlobLoadPromise) {
    loadScreenshotPixelFromBlob(src, {reset: false}).catch(() => {})
  }
  return null
}

async function loadScreenshotPixelFromBlob(src, {reset = true} = {}) {
  if (screenshotPixelBlobLoadPromise) {
    return screenshotPixelBlobLoadPromise
  }
  screenshotPixelBlobLoadPromise = (async () => {
    try {
      const resp = await fetch(src)
      const blob = await resp.blob()
      const bitmap = await createImageBitmap(blob)
      if (screenshotBitmap) {
        try {
          screenshotBitmap.close()
        } catch (_) {
          /* ignore */
        }
      }
      screenshotBitmap = bitmap
      screenshotPixelCanvas = document.createElement('canvas')
      screenshotPixelCanvas.width = bitmap.width
      screenshotPixelCanvas.height = bitmap.height
      screenshotPixelCtx = screenshotPixelCanvas.getContext('2d')
      if (screenshotPixelCtx) {
        screenshotPixelCtx.drawImage(bitmap, 0, 0)
      }
      if (reset) {
        resetAnnotationStateForNewImage()
        nextTick(() => {
          initCanvas()
          isCaptureReady.value = Boolean(
              canvas.value && canvas.value.width > 0 && canvas.value.height > 0
          )
        })
      }
    } catch (e) {
      console.error('通过 blob 加载截图像素失败:', e)
      if (reset) {
        isCaptureReady.value = false
      }
    } finally {
      screenshotPixelBlobLoadPromise = null
    }
  })()
  return screenshotPixelBlobLoadPromise
}

function resetAnnotationStateForNewImage() {
  revokeScreenshotObjectUrl()
  finishInlineEdit()
  selectedTextId.value = null
  selectedShapeId.value = null
  textItems.value = []
  shapeItems.value = []
  history.value = []
  historyIndex.value = -1
  overlayCommandLog = []
  vectorCommandLog = []
  activeRasterCommand = null
  editingBeforeItem.value = null
  movingTextBeforeItem = null
  movingShapeBeforeItem = null
  resizingShapeBeforeItem = null
  adjustingLineBeforeItem = null
  currentDrawingSnapshot = null
  overlayDirty.value = false
}

function onMouseDown(e) {
  if (editingTextId.value !== null) {
    finishInlineEdit()
    return
  }
  if (e.button !== 0) return
  if (state.value !== 'text-moving') {
    selectedTextId.value = null
  }
  if (state.value !== 'shape-moving') {
    selectedShapeId.value = null
  }
  const p = toScenePoint(e)
  if (isDevMode) console.log('[mousedown]', 'state=' + state.value, 'target=' + e.target.className, 'p=' + Math.round(p.x) + ',' + Math.round(p.y), 'rect=' + Math.round(rect.x) + ',' + Math.round(rect.y) + ' ' + Math.round(rect.width) + 'x' + Math.round(rect.height), 'border=' + isInsideBorder(p.x, p.y, rect), 'inside=' + isInside(p.x, p.y, rect))
  if (state.value === 'idle') {
    state.value = 'selecting'
    startPoint.x = p.x
    startPoint.y = p.y
    rect.x = p.x
    rect.y = p.y
    rect.width = 0
    rect.height = 0
  } else if (state.value === 'selected') {
    if (currentTool.value === 'select') {
      if (longshotResultActive.value) {
        state.value = 'moving'
        startPoint.x = e.clientX
        startPoint.y = e.clientY
        startRect.x = longshotViewOffset.x
        startRect.y = longshotViewOffset.y
      } else if (isInsideBorder(p.x, p.y, rect)) {
        state.value = 'selecting'
        startPoint.x = p.x
        startPoint.y = p.y
        rect.x = p.x
        rect.y = p.y
        rect.width = 0
        rect.height = 0
        if (history.value.length > 0) {
          historyIndex.value = 0
          restoreFromHistory()
        }
      } else if (isInside(p.x, p.y, rect)) {
        state.value = 'moving'
        startPoint.x = p.x
        startPoint.y = p.y
        Object.assign(startRect, rect)
      } else {

        state.value = 'selecting'
        startPoint.x = p.x
        startPoint.y = p.y
        rect.x = p.x
        rect.y = p.y
        rect.width = 0
        rect.height = 0

        if (history.value.length > 0) {
          historyIndex.value = 0
          restoreFromHistory()
        }
      }
    } else {

      if (!isInside(p.x, p.y, rect)) {
        if (currentTool.value === 'picker') {
          // 取色器在选区外点击忽略
          return
        }

        state.value = 'selecting'
        startPoint.x = p.x
        startPoint.y = p.y
        rect.x = p.x
        rect.y = p.y
        rect.width = 0
        rect.height = 0
        currentTool.value = 'select'

        if (history.value.length > 0) {
          historyIndex.value = 0
          restoreFromHistory()
        }
        return
      }
      state.value = 'drawing'
      handleCanvasMouseDown(e)
    }
  } else if (state.value === 'drawing') {
    // 正在绘制时再次点击：先结束当前绘制，再开始新的
    handleCanvasMouseUp(e)
    if (currentTool.value !== 'picker') {
      handleCanvasMouseDown(e)
    }
  }
}

function onMouseMove(e) {
  if (editingTextId.value !== null) return
  const p = toScenePoint(e)
  // 取色器：鼠标悬停即可拾色，无需点击
  if (currentTool.value === 'picker' && hasSelection.value) {
    if (!isInside(p.x, p.y, rect)) return
    drawStart.x = p.x
    drawStart.y = p.y
    pickColorAtScene(p.x, p.y)
    return
  }
  if (state.value === 'idle') {
    highlightedWindow.value = detectWindowAt(p.x, p.y)
  } else if (state.value === 'selecting') {

    const x = Math.min(startPoint.x, p.x)
    const y = Math.min(startPoint.y, p.y)
    const width = Math.abs(p.x - startPoint.x)
    const height = Math.abs(p.y - startPoint.y)
    rect.x = x
    rect.y = y
    rect.width = width
    rect.height = height
  } else if (state.value === 'moving') {
    if (longshotResultActive.value && currentTool.value === 'select') {
      longshotViewOffset.x = startRect.x + (e.clientX - startPoint.x)
      longshotViewOffset.y = startRect.y + (e.clientY - startPoint.y)
    } else {
      const dx = p.x - startPoint.x
      const dy = p.y - startPoint.y
      rect.x = startRect.x + dx
      rect.y = startRect.y + dy
    }
  } else if (state.value === 'resizing') {
    handleResize(e)
  } else if (state.value === 'text-moving') {
    const item = textItems.value.find((entry) => entry.id === movingTextStart.id)
    if (!item) return
    const dx = p.x - movingTextStart.x
    const dy = p.y - movingTextStart.y
    item.x = movingTextStart.itemX + dx
    item.y = movingTextStart.itemY + dy
  } else if (state.value === 'shape-moving') {
    const item = shapeItems.value.find((entry) => entry.id === movingShapeStart.id)
    if (!item) return
    const dx = p.x - movingShapeStart.x
    const dy = p.y - movingShapeStart.y
    item.x = movingShapeStart.itemX + dx
    item.y = movingShapeStart.itemY + dy
  } else if (state.value === 'shape-resizing') {
    handleResizeShapeItem(e)
  } else if (state.value === 'shape-point-moving') {
    handleAdjustLineEndpoint(e)
  } else if (state.value === 'drawing') {
    handleCanvasMouseMove(e)
  }
}

function onMouseUp(e) {
  if (editingTextId.value !== null) return
  if (e.button !== 0) return
  const p = toScenePoint(e)
  if (state.value === 'selecting') {
    if (rect.width < 10 || rect.height < 10) {
      if (highlightedWindow.value) {
        const w = highlightedWindow.value
        const almostFullscreen =
            w.width >= window.innerWidth - 16 &&
            w.height >= window.innerHeight - 16
        if (almostFullscreen) {
          rect.x = 0
          rect.y = 0
          rect.width = window.innerWidth
          rect.height = window.innerHeight
        } else {
          Object.assign(rect, highlightedWindow.value)
        }
        state.value = 'selected'
      } else {
        rect.x = 0
        rect.y = 0
        rect.width = window.innerWidth
        rect.height = window.innerHeight
        state.value = 'selected'
      }
    } else {
      state.value = 'selected'
    }
    if (regionSelectMode.value === 'recording_region' && state.value === 'selected' && rect.width > 0 && rect.height > 0) {
      regionConfirmAnchor.x = p.x + 8
      regionConfirmAnchor.y = p.y + 8
      regionConfirmAnchor.ready = true
    }
  } else if (state.value === 'moving' || state.value === 'resizing') {
    state.value = 'selected'
  } else if (state.value === 'text-moving') {
    state.value = 'selected'
    const item = textItems.value.find((entry) => entry.id === movingTextStart.id)
    if (item && !areFlatItemsEqual(movingTextBeforeItem, item)) {
      appendVectorUpsertCommand('text', item)
      saveToHistory()
    }
    movingTextBeforeItem = null
  } else if (state.value === 'shape-moving') {
    state.value = 'selected'
    const item = shapeItems.value.find((entry) => entry.id === movingShapeStart.id)
    if (item && !areFlatItemsEqual(movingShapeBeforeItem, item)) {
      appendVectorUpsertCommand('shape', item)
      saveToHistory()
    }
    movingShapeBeforeItem = null
  } else if (state.value === 'shape-resizing') {
    state.value = 'selected'
    const item = shapeItems.value.find((entry) => entry.id === resizingShapeStart.id)
    if (item && !areFlatItemsEqual(resizingShapeBeforeItem, item)) {
      appendVectorUpsertCommand('shape', item)
      saveToHistory()
    }
    resizingShapeBeforeItem = null
  } else if (state.value === 'shape-point-moving') {
    state.value = 'selected'
    const item = shapeItems.value.find((entry) => entry.id === adjustingLinePointStart.id)
    if (item && !areFlatItemsEqual(adjustingLineBeforeItem, item)) {
      appendVectorUpsertCommand('shape', item)
      saveToHistory()
    }
    adjustingLineBeforeItem = null
  } else if (state.value === 'drawing') {
    handleCanvasMouseUp(e)
    state.value = 'selected'
  }
}

function cancelSelection() {
  if (manualLongshotSessionId.value > 0) {
    cancelManualLongshotCapture(true)
  }
  finishInlineEdit()
  selectedTextId.value = null
  selectedShapeId.value = null
  state.value = 'idle'
  rect.width = 0
  rect.height = 0
  regionConfirmAnchor.ready = false
  currentTool.value = 'select'

  if (history.value.length > 0) {
    historyIndex.value = 0
    restoreFromHistory()
  }
}

function setTool(toolId) {
  currentTool.value = toolId
  selectedShapeId.value = null
  if (toolId !== 'picker') {
    pickerCopyHint.value = t('screenshot.pickerHint')
  }
}

function enterManualLongshotMode() {
  if (manualLongshotAvailability.status !== 'available' && manualLongshotAvailability.status !== 'busy') {
    manualLongshotHint.value = manualLongshotAvailability.summary || t('screenshot.longshotUnavailable')
    return
  }
  if (manualLongshotAvailability.status === 'busy' && !manualLongshotSessionId.value) {
    manualLongshotHint.value = t('screenshot.longshotRunning')
    return
  }
  cancelManualLongshotCapture(false)
  finishInlineEdit()
  regionSelectMode.value = 'manual_longshot'
  manualLongshotRunning.value = false
  manualLongshotSessionId.value = 0
  currentTool.value = 'select'
  selectedTextId.value = null
  selectedShapeId.value = null
  const keepCurrentSelection = hasSelection.value && state.value === 'selected'
  if (keepCurrentSelection) {

    manualLongshotHint.value = t('screenshot.longshotStarted')
    state.value = 'selected'
    return
  }

  manualLongshotHint.value = t('screenshot.longshotManualHint')
  state.value = 'idle'
  textItems.value = []
  shapeItems.value = []
  history.value = []
  historyIndex.value = -1
  overlayCommandLog = []
  vectorCommandLog = []
  activeRasterCommand = null
  editingBeforeItem.value = null
  movingTextBeforeItem = null
  movingShapeBeforeItem = null
  resizingShapeBeforeItem = null
  adjustingLineBeforeItem = null
  rect.x = 0
  rect.y = 0
  rect.width = 0
  rect.height = 0
  regionConfirmAnchor.ready = false
  initCanvas()
}

// 绘制相关
function handleCanvasMouseDown(event) {
  const p = toScenePoint(event)
  if (currentTool.value === 'picker') {
    pickColorAtScene(p.x, p.y)
    drawStart.x = p.x
    drawStart.y = p.y
    return
  }

  isDrawing.value = true
  drawStart.x = p.x
  drawStart.y = p.y

  const ctx = canvas.value.getContext('2d')

  if (currentTool.value === 'pen' || currentTool.value === 'mosaic' || currentTool.value === 'highlight') {
    ctx.beginPath()
    ctx.moveTo(drawStart.x, drawStart.y)
    if (currentTool.value === 'highlight') {
      ctx.globalAlpha = 0.35
      ctx.strokeStyle = currentColor.value || '#F5D76E'
      ctx.lineWidth = 18
      ctx.lineCap = 'round'
    }
    activeRasterCommand = {
      type: currentTool.value,
      color: currentTool.value === 'highlight' ? (currentColor.value || '#F5D76E') : currentColor.value,
      lineWidth: currentTool.value === 'mosaic'
          ? (Number(mosaicSize.value) || 8)
          : currentTool.value === 'highlight'
              ? 18
              : (Number(lineWidth.value) || 1),
      opacity: currentTool.value === 'highlight' ? 0.35 : undefined,
      points: [{x: drawStart.x, y: drawStart.y}]
    }
    if (currentTool.value === 'highlight') {
      ctx.globalAlpha = 1
    }
  } else if (SHAPE_TOOLS.includes(currentTool.value)) {
    currentDrawingSnapshot = ctx.getImageData(0, 0, canvas.value.width, canvas.value.height)
  }
}

function handleCanvasMouseMove(event) {
  const p = toScenePoint(event)

  if (!isDrawing.value) return

  const x0 = p.x
  const y0 = p.y
  let x = x0
  let y = y0
  const ctx = canvas.value.getContext('2d')

  if (event.shiftKey && (currentTool.value === 'rect' || currentTool.value === 'redact'
      || currentTool.value === 'circle' || currentTool.value === 'number'
      || currentTool.value === 'line' || currentTool.value === 'arrow')) {
    const constrained = (currentTool.value === 'line' || currentTool.value === 'arrow')
        ? constrainLinePoint(drawStart, {x, y})
        : constrainRectPoint(drawStart, {x, y})
    x = constrained.x
    y = constrained.y
  }

  if (currentTool.value === 'pen') {
    ctx.strokeStyle = currentColor.value
    ctx.lineWidth = lineWidth.value
    ctx.lineCap = 'round'
    ctx.lineTo(x, y)
    ctx.stroke()
    activeRasterCommand?.points?.push({x, y})
  } else if (currentTool.value === 'highlight') {
    ctx.globalAlpha = 0.35
    ctx.strokeStyle = currentColor.value || '#F5D76E'
    ctx.lineWidth = 18
    ctx.lineCap = 'round'
    ctx.lineTo(x, y)
    ctx.stroke()
    ctx.globalAlpha = 1
    activeRasterCommand?.points?.push({x, y})
  } else if (currentTool.value === 'mosaic') {
    // 先记点，再画：采样失败也不能丢掉命令，否则导出同样无马赛克
    activeRasterCommand?.points?.push({x, y})
    try {
      applyMosaicAtScenePoint(ctx, x, y, Number(mosaicSize.value) || 8)
    } catch (err) {
      console.warn('马赛克实时绘制失败:', err)
    }
  } else if (SHAPE_TOOLS.includes(currentTool.value)) {

    const oldTransform = ctx.getTransform()
    ctx.resetTransform()
    ctx.putImageData(currentDrawingSnapshot, 0, 0)
    ctx.setTransform(oldTransform)

    ctx.strokeStyle = currentColor.value
    ctx.lineWidth = lineWidth.value
    ctx.beginPath()

    if (currentTool.value === 'line' || currentTool.value === 'arrow') {
      ctx.moveTo(drawStart.x, drawStart.y)
      ctx.lineTo(x, y)
      ctx.stroke()
      if (currentTool.value === 'arrow') {
        drawArrowHead(ctx, drawStart.x, drawStart.y, x, y)
      }
    } else if (currentTool.value === 'rect' || currentTool.value === 'redact') {
      if (currentTool.value === 'redact' || shapeFillEnabled.value) {
        ctx.fillStyle = currentTool.value === 'redact'
            ? '#000000'
            : hexToRgba(currentColor.value, shapeFillOpacity.value)
        const rx = Math.min(drawStart.x, x)
        const ry = Math.min(drawStart.y, y)
        const rw = Math.abs(x - drawStart.x)
        const rh = Math.abs(y - drawStart.y)
        ctx.fillRect(rx, ry, rw, rh)
      }
      if (currentTool.value !== 'redact') {
        ctx.strokeRect(drawStart.x, drawStart.y, x - drawStart.x, y - drawStart.y)
      }
    } else if (currentTool.value === 'circle' || currentTool.value === 'number') {
      const radius = currentTool.value === 'number'
          ? Math.max(NUMBER_MIN_RADIUS, Math.sqrt(Math.pow(x - drawStart.x, 2) + Math.pow(y - drawStart.y, 2)))
          : Math.sqrt(Math.pow(x - drawStart.x, 2) + Math.pow(y - drawStart.y, 2))
      if (currentTool.value === 'number' || shapeFillEnabled.value) {
        ctx.fillStyle = currentTool.value === 'number'
            ? currentColor.value
            : hexToRgba(currentColor.value, shapeFillOpacity.value)
        ctx.arc(drawStart.x, drawStart.y, radius, 0, Math.PI * 2)
        ctx.fill()
        ctx.beginPath()
      }
      if (currentTool.value === 'number') {
        ctx.strokeStyle = currentColor.value
        ctx.lineWidth = Math.max(2, lineWidth.value)
        ctx.arc(drawStart.x, drawStart.y, radius, 0, Math.PI * 2)
        ctx.stroke()
        const previewN = nextNumberCalloutIndex()
        ctx.fillStyle = '#ffffff'
        ctx.font = `700 ${Math.max(10, radius * 0.9)}px Arial`
        ctx.textAlign = 'center'
        ctx.textBaseline = 'middle'
        ctx.fillText(String(previewN), drawStart.x, drawStart.y)
      } else {
        ctx.arc(drawStart.x, drawStart.y, radius, 0, Math.PI * 2)
        ctx.stroke()
      }
    }
  }
}

function handleCanvasMouseUp(event) {
  if (!isDrawing.value) return
  isDrawing.value = false

  const p = toScenePoint(event)
  let x = p.x
  let y = p.y
  // 与 mousemove 预览同一套 Shift 约束，保证提交几何与预览一致
  if (event.shiftKey && (currentTool.value === 'rect' || currentTool.value === 'redact'
      || currentTool.value === 'circle' || currentTool.value === 'number'
      || currentTool.value === 'line' || currentTool.value === 'arrow')) {
    const constrained = (currentTool.value === 'line' || currentTool.value === 'arrow')
        ? constrainLinePoint(drawStart, {x, y})
        : constrainRectPoint(drawStart, {x, y})
    x = constrained.x
    y = constrained.y
  }
  const ctx = canvas.value.getContext('2d')

  if (currentTool.value === 'text') {
    startCreateTextItem(x, y)
    state.value = 'selected'
    activeRasterCommand = null
    return
  }

  if (SHAPE_TOOLS.includes(currentTool.value)) {
    if (currentDrawingSnapshot && ctx) {
      const oldTransform = ctx.getTransform()
      ctx.resetTransform()
      ctx.putImageData(currentDrawingSnapshot, 0, 0)
      ctx.setTransform(oldTransform)
      currentDrawingSnapshot = null
    }
    const createdShape = createShapeItem(currentTool.value, drawStart.x, drawStart.y, x, y)
    currentTool.value = 'select'
    if (createdShape) {
      appendVectorUpsertCommand('shape', createdShape)
      saveToHistory()
    }
    activeRasterCommand = null
    return
  }

  commitActiveRasterCommand()
  saveToHistory()
}

function startDragTextItem(id, event) {
  if (editingTextId.value !== null) return
  selectedTextId.value = id
  selectedShapeId.value = null
  const item = textItems.value.find((entry) => entry.id === id)
  if (!item) return
  const p = toScenePoint(event)
  movingTextStart.x = p.x
  movingTextStart.y = p.y
  movingTextStart.itemX = item.x
  movingTextStart.itemY = item.y
  movingTextStart.id = id
  movingTextBeforeItem = {...item}
  state.value = 'text-moving'
}

function startDragShapeItem(id, event) {
  if (editingTextId.value !== null) return
  currentTool.value = 'select'
  selectedShapeId.value = id
  selectedTextId.value = null
  const item = shapeItems.value.find((entry) => entry.id === id)
  if (!item) return
  syncFillControlsFromShape(item)
  const p = toScenePoint(event)
  movingShapeStart.x = p.x
  movingShapeStart.y = p.y
  movingShapeStart.itemX = item.x
  movingShapeStart.itemY = item.y
  movingShapeStart.id = id
  movingShapeBeforeItem = {...item}
  state.value = 'shape-moving'
}

function startResizeShapeItem(id, handle, event) {
  if (editingTextId.value !== null) return
  const item = shapeItems.value.find((entry) => entry.id === id)
  if (!item || (item.type !== 'rect' && item.type !== 'redact' && item.type !== 'circle' && item.type !== 'number')) return
  selectedShapeId.value = id
  selectedTextId.value = null
  syncFillControlsFromShape(item)
  const p = toScenePoint(event)
  resizingShapeStart.x = p.x
  resizingShapeStart.y = p.y
  resizingShapeStart.itemX = item.x
  resizingShapeStart.itemY = item.y
  resizingShapeStart.itemWidth = item.width
  resizingShapeStart.itemHeight = item.height
  resizingShapeStart.handle = handle
  resizingShapeStart.id = id
  resizingShapeBeforeItem = {...item}
  state.value = 'shape-resizing'
}

function startAdjustLineEndpoint(id, point, event) {
  if (editingTextId.value !== null) return
  const item = shapeItems.value.find((entry) => entry.id === id)
  if (!item || (item.type !== 'line' && item.type !== 'arrow')) return
  selectedShapeId.value = id
  selectedTextId.value = null
  adjustingLinePointStart.id = id
  adjustingLinePointStart.point = point
  adjustingLineBeforeItem = {...item}
  state.value = 'shape-point-moving'
}

function getTextCreationBounds() {
  const displayMargin = 10
  const estimatedDisplayWidth = 240
  const estimatedDisplayHeight = 120
  if (longshotResultActive.value && screenshotImg.value) {
    const scale = Math.max(0.1, longshotViewScale.value || 1)
    const ox = longshotViewOffset.x || 0
    const oy = longshotViewOffset.y || 0
    const view = getLongshotImageViewportRect(
        Math.max(1, Number(screenshotImg.value.width) || 1),
        Math.max(1, Number(screenshotImg.value.height) || 1)
    )

    const estimatedWidth = estimatedDisplayWidth / scale
    const estimatedHeight = estimatedDisplayHeight / scale
    const margin = displayMargin / scale

    const visibleMinX = (displayMargin - ox) / scale
    const visibleMinY = (displayMargin - oy) / scale
    const visibleMaxX = (window.innerWidth - estimatedDisplayWidth - displayMargin - ox) / scale
    const visibleMaxY = (window.innerHeight - estimatedDisplayHeight - displayMargin - oy) / scale

    const minX = Math.round(Math.max(view.x + margin, visibleMinX))
    const minY = Math.round(Math.max(view.y + margin, visibleMinY))
    const maxX = Math.round(Math.max(minX, Math.min(view.x + view.width - estimatedWidth - margin, visibleMaxX)))
    const maxY = Math.round(Math.max(minY, Math.min(view.y + view.height - estimatedHeight - margin, visibleMaxY)))
    return {minX, minY, maxX, maxY}
  }
  const minX = displayMargin
  const minY = displayMargin
  const maxX = Math.max(minX, window.innerWidth - estimatedDisplayWidth - displayMargin)
  const maxY = Math.max(minY, window.innerHeight - estimatedDisplayHeight - displayMargin)
  return {minX, minY, maxX, maxY}
}

function getMergedPixelColorAt(px, py) {
  const drawCanvas = canvas.value
  if (!drawCanvas || !drawCanvas.width || !drawCanvas.height) return null
  const pixelCanvas = screenshotPixelCanvas
  if (!pixelCanvas) return null
  if (px < 0 || py < 0 || px >= pixelCanvas.width || py >= pixelCanvas.height) return null
  let baseData = null
  try {
    if (screenshotPixelCtx && isPixelCtxReadable()) {
      baseData = screenshotPixelCtx.getImageData(px, py, 1, 1).data
    } else if (screenshotBitmap) {
      const tmp = document.createElement('canvas')
      tmp.width = 1
      tmp.height = 1
      const tctx = tmp.getContext('2d')
      if (tctx) {
        tctx.drawImage(screenshotBitmap, px, py, 1, 1, 0, 0, 1, 1)
        baseData = tctx.getImageData(0, 0, 1, 1).data
      }
    }
  } catch (_) {
    baseData = null
  }
  if (!baseData) return null
  const drawCtx = drawCanvas.getContext('2d')
  if (!drawCtx) return {r: baseData[0], g: baseData[1], b: baseData[2]}
  // 叠加层标注画布使用 CSS 像素分辨率，坐标需缩放
  const cpx = Math.round(px / dpr)
  const cpy = Math.round(py / dpr)
  if (cpx < 0 || cpy < 0 || cpx >= drawCanvas.width || cpy >= drawCanvas.height) {
    return {r: baseData[0], g: baseData[1], b: baseData[2]}
  }
  const oldTransform = drawCtx.getTransform()
  drawCtx.resetTransform()
  let overlayData = null
  try {
    overlayData = drawCtx.getImageData(cpx, cpy, 1, 1).data
  } catch (_) {
    // keep null on tainted/failure
  }
  drawCtx.setTransform(oldTransform)
  if (overlayData && overlayData[3] > 0) {
    return {r: overlayData[0], g: overlayData[1], b: overlayData[2]}
  }
  return {r: baseData[0], g: baseData[1], b: baseData[2]}
}

function renderPickerMagnifier(px, py) {
  if (!pickerMagnifierCanvasRef.value || !canvas.value || !screenshotPixelCtx) return
  const magnifierCanvas = pickerMagnifierCanvasRef.value
  const magnifierCtx = magnifierCanvas.getContext('2d')
  if (!magnifierCtx) return
  const sampleSize = 11
  const zoom = 12
  const half = Math.floor(sampleSize / 2)
  magnifierCanvas.width = sampleSize * zoom
  magnifierCanvas.height = sampleSize * zoom
  magnifierCtx.clearRect(0, 0, magnifierCanvas.width, magnifierCanvas.height)
  for (let row = 0; row < sampleSize; row++) {
    for (let col = 0; col < sampleSize; col++) {
      const sx = px + col - half
      const sy = py + row - half
      const color = getMergedPixelColorAt(sx, sy)
      const fill = color ? `rgb(${color.r}, ${color.g}, ${color.b})` : 'rgba(0,0,0,0.2)'
      magnifierCtx.fillStyle = fill
      magnifierCtx.fillRect(col * zoom, row * zoom, zoom, zoom)
    }
  }
  magnifierCtx.strokeStyle = 'rgba(255,255,255,0.35)'
  magnifierCtx.lineWidth = 1
  for (let i = 0; i <= sampleSize; i++) {
    const p = i * zoom
    magnifierCtx.beginPath()
    magnifierCtx.moveTo(p, 0)
    magnifierCtx.lineTo(p, magnifierCanvas.height)
    magnifierCtx.stroke()
    magnifierCtx.beginPath()
    magnifierCtx.moveTo(0, p)
    magnifierCtx.lineTo(magnifierCanvas.width, p)
    magnifierCtx.stroke()
  }
  magnifierCtx.strokeStyle = '#00aaff'
  magnifierCtx.lineWidth = 2
  magnifierCtx.strokeRect(half * zoom + 1, half * zoom + 1, zoom - 2, zoom - 2)
}

function appendVectorUpsertCommand(target, item) {
  if (!item) return
  truncateFutureHistoryForMutation()
  vectorCommandLog.push({
    target,
    op: 'upsert',
    item: target === 'text' ? cloneTextItem(item) : cloneShapeItem(item)
  })
}

function appendVectorDeleteCommand(target, itemId) {
  if (!itemId) return
  truncateFutureHistoryForMutation()
  vectorCommandLog.push({
    target,
    op: 'delete',
    itemId
  })
}

function clearOverlayCanvas(ctx) {
  const oldTransform = ctx.getTransform()
  ctx.resetTransform()
  ctx.clearRect(0, 0, canvas.value.width, canvas.value.height)
  ctx.setTransform(oldTransform)
}

function applyMosaicAtScenePoint(ctx, x, y, strokeWidth) {
  // 与后端 apply_mosaic_at_image_point 同一算法，保证编辑器预览与导出一致：
  // center = scene * dpr（全图像素坐标）；size = stroke*3*dpr；block = 6*dpr；块内取左上角采样色
  const sourceCanvas = screenshotPixelCanvas
  const sourceImg = screenshotImg.value
  if (!sourceCanvas && !sourceImg) return

  const stroke = Math.max(1, Number(strokeWidth) || 8)
  let scale
  let sceneX
  let sceneY
  let srcW
  let srcH

  if (longshotResultActive.value) {
    const imgW = sourceCanvas
        ? sourceCanvas.width
        : (sourceImg.naturalWidth || sourceImg.width || 1)
    const imgH = sourceCanvas
        ? sourceCanvas.height
        : (sourceImg.naturalHeight || sourceImg.height || 1)
    const view = getLongshotImageViewportRect(Math.max(1, imgW), Math.max(1, imgH))
    const imagePoint = sceneToImagePoint(x, y, view)
    scale = 1 / Math.max(0.0001, view.fit)
    sceneX = imagePoint.x
    sceneY = imagePoint.y
    srcW = imgW
    srcH = imgH
  } else {
    srcW = sourceCanvas
        ? sourceCanvas.width
        : (sourceImg.naturalWidth || sourceImg.width || 1)
    srcH = sourceCanvas
        ? sourceCanvas.height
        : (sourceImg.naturalHeight || sourceImg.height || 1)
    scale = dpr
    sceneX = x * dpr
    sceneY = y * dpr
  }

  const mosaicParams = computeMosaicBlockParams({
    stroke,
    scale,
    centerX: sceneX,
    centerY: sceneY,
    srcW,
    srcH
  })
  const {size, blockSize, half, centerX, centerY, regionX, regionY, regionW, regionH} = mosaicParams
  const out = document.createElement('canvas')
  out.width = size
  out.height = size
  const outCtx = out.getContext('2d')
  if (!outCtx) return
  const imgData = outCtx.createImageData(size, size)
  const data = imgData.data

  // 必须用无 taint 的像素源；否则预览全灰，导出（后端读文件）却是真马赛克
  const region = readCleanPixelRegion(regionX, regionY, regionW, regionH)

  const sampleAt = (imgX, imgY) => {
    const cx = Math.min(Math.max(imgX, 0), Math.max(0, srcW - 1))
    const cy = Math.min(Math.max(imgY, 0), Math.max(0, srcH - 1))
    if (!region) return [107, 107, 107]
    const lx = cx - regionX
    const ly = cy - regionY
    if (lx < 0 || ly < 0 || lx >= region.width || ly >= region.height) {
      return [107, 107, 107]
    }
    const idx = (ly * region.width + lx) * 4
    return [region.data[idx], region.data[idx + 1], region.data[idx + 2]]
  }

  for (let blockY = 0; blockY < size; blockY += blockSize) {
    for (let blockX = 0; blockX < size; blockX += blockSize) {
      const srcX = centerX - half + blockX
      const srcY = centerY - half + blockY
      const [r, g, b] = sampleAt(srcX, srcY)
      for (let by = 0; by < blockSize; by++) {
        for (let bx = 0; bx < blockSize; bx++) {
          const dx = blockX + bx
          const dy = blockY + by
          if (dx >= size || dy >= size) continue
          const idx = (dy * size + dx) * 4
          data[idx] = r
          data[idx + 1] = g
          data[idx + 2] = b
          data[idx + 3] = 255
        }
      }
    }
  }
  outCtx.putImageData(imgData, 0, 0)

  // 预览绘制：物理尺寸 size 对应 CSS size/dpr（长截图为 scene 单位）
  const drawCss = longshotResultActive.value ? size : size / dpr
  const oldSmoothing = ctx.imageSmoothingEnabled
  ctx.imageSmoothingEnabled = false
  try {
    ctx.drawImage(out, x - drawCss / 2, y - drawCss / 2, drawCss, drawCss)
  } catch (err) {
    console.warn('马赛克绘制失败:', err)
  }
  ctx.imageSmoothingEnabled = oldSmoothing
}

function applyRasterCommand(ctx, command) {
  if (!command) return
  if (command.type === 'pen' || command.type === 'highlight') {
    if (!Array.isArray(command.points) || command.points.length < 2) return
    const opacity = command.type === 'highlight'
        ? (command.opacity != null ? Number(command.opacity) : 0.35)
        : 1
    ctx.globalAlpha = opacity
    ctx.strokeStyle = command.color
    ctx.lineWidth = command.lineWidth
    ctx.lineCap = 'round'
    ctx.lineJoin = 'round'
    ctx.beginPath()
    ctx.moveTo(command.points[0].x, command.points[0].y)
    for (let i = 1; i < command.points.length; i++) {
      const point = command.points[i]
      ctx.lineTo(point.x, point.y)
    }
    ctx.stroke()
    ctx.globalAlpha = 1
    return
  }
  if (command.type === 'mosaic') {
    if (!Array.isArray(command.points) || command.points.length < 1) return
    for (const point of command.points) {
      try {
        applyMosaicAtScenePoint(ctx, point.x, point.y, command.lineWidth)
      } catch (err) {
        console.warn('马赛克重放失败:', err)
      }
    }
  }
}

function replayOverlayCommands(commandCount) {
  if (!canvas.value) return
  const ctx = canvas.value.getContext('2d')
  if (!ctx) return
  clearOverlayCanvas(ctx)
  const limit = Math.max(0, Math.min(commandCount, overlayCommandLog.length))
  for (let i = 0; i < limit; i++) {
    applyRasterCommand(ctx, overlayCommandLog[i])
  }
}

function replayVectorCommands(commandCount) {
  const textMap = new Map()
  const shapeMap = new Map()
  const limit = Math.max(0, Math.min(commandCount, vectorCommandLog.length))
  for (let i = 0; i < limit; i++) {
    const command = vectorCommandLog[i]
    if (!command) continue
    if (command.target === 'text') {
      if (command.op === 'upsert' && command.item) {
        textMap.set(command.item.id, cloneTextItem(command.item))
      } else if (command.op === 'delete') {
        textMap.delete(command.itemId)
      }
      continue
    }
    if (command.target === 'shape') {
      if (command.op === 'upsert' && command.item) {
        shapeMap.set(command.item.id, cloneShapeItem(command.item))
      } else if (command.op === 'delete') {
        shapeMap.delete(command.itemId)
      }
    }
  }
  textItems.value = Array.from(textMap.values())
  shapeItems.value = Array.from(shapeMap.values())
}

function truncateFutureHistoryForMutation() {
  if (historyIndex.value < history.value.length - 1) {
    const currentSnapshot = history.value[historyIndex.value]
    const keepOverlayCommandCount = currentSnapshot?.overlayCommandCount ?? 0
    const keepVectorCommandCount = currentSnapshot?.vectorCommandCount ?? 0
    overlayCommandLog = overlayCommandLog.slice(0, keepOverlayCommandCount)
    vectorCommandLog = vectorCommandLog.slice(0, keepVectorCommandCount)
    history.value = history.value.slice(0, historyIndex.value + 1)
  }
}

function pruneHistoryWindowIfNeeded() {
  if (history.value.length <= 50) return
  history.value.shift()
  historyIndex.value = Math.max(0, historyIndex.value - 1)
  const keepOverlayFrom = history.value[0]?.overlayCommandCount ?? 0
  const keepVectorFrom = history.value[0]?.vectorCommandCount ?? 0
  if (keepOverlayFrom > 0) {
    overlayCommandLog = overlayCommandLog.slice(keepOverlayFrom)
  }
  if (keepVectorFrom > 0) {
    vectorCommandLog = vectorCommandLog.slice(keepVectorFrom)
  }
  if (keepOverlayFrom > 0 || keepVectorFrom > 0) {
    history.value = history.value.map((entry) => ({
      ...entry,
      overlayCommandCount: Math.max(0, entry.overlayCommandCount - keepOverlayFrom),
      vectorCommandCount: Math.max(0, (entry.vectorCommandCount || 0) - keepVectorFrom)
    }))
  }
}

function commitActiveRasterCommand() {
  if (!activeRasterCommand) return false
  const pointCount = activeRasterCommand.points?.length || 0
  // 马赛克单点也应落盘（点击即打码）；笔刷仍需至少 2 点成线
  const minPoints = activeRasterCommand.type === 'mosaic' ? 1 : 2
  const shouldCommit = pointCount >= minPoints
  if (!shouldCommit) {
    activeRasterCommand = null
    return false
  }
  // 马赛克：按笔刷半径加密采样点，导出覆盖与预览笔触一致
  let points = activeRasterCommand.points
  if (activeRasterCommand.type === 'mosaic' && points.length >= 2) {
    const stroke = Math.max(1, Number(activeRasterCommand.lineWidth) || 8)
    const step = Math.max(2, stroke * 0.45)
    const densified = [points[0]]
    for (let i = 1; i < points.length; i++) {
      const prev = densified[densified.length - 1]
      const cur = points[i]
      const dx = cur.x - prev.x
      const dy = cur.y - prev.y
      const dist = Math.hypot(dx, dy)
      if (dist <= step) {
        densified.push(cur)
        continue
      }
      const n = Math.floor(dist / step)
      for (let k = 1; k <= n; k++) {
        densified.push({x: prev.x + (dx * k) / (n + 1), y: prev.y + (dy * k) / (n + 1)})
      }
      densified.push(cur)
    }
    points = densified
  }
  truncateFutureHistoryForMutation()
  overlayCommandLog.push({
    type: activeRasterCommand.type,
    color: activeRasterCommand.color,
    lineWidth: activeRasterCommand.lineWidth,
    opacity: activeRasterCommand.opacity != null ? Number(activeRasterCommand.opacity) : undefined,
    points: points.map((point) => ({x: point.x, y: point.y}))
  })
  activeRasterCommand = null
  return true
}

function saveToHistory() {
  if (!canvas.value) return
  truncateFutureHistoryForMutation()
  const snapshotOverlayDirty = detectOverlayDirty(
      overlayCommandLog.length,
      vectorCommandLog.length
  )
  history.value.push({
    overlayCommandCount: overlayCommandLog.length,
    vectorCommandCount: vectorCommandLog.length,
    overlayDirty: snapshotOverlayDirty
  })
  overlayDirty.value = snapshotOverlayDirty
  historyIndex.value = history.value.length - 1
  pruneHistoryWindowIfNeeded()
}

function undo() {
  if (historyIndex.value > 0) {
    historyIndex.value--
    restoreFromHistory()
  }
}

function redo() {
  if (historyIndex.value < history.value.length - 1) {
    historyIndex.value++
    restoreFromHistory()
  }
}

function restoreFromHistory() {
  if (historyIndex.value < 0 || !history.value[historyIndex.value]) return
  const snapshot = history.value[historyIndex.value]
  replayOverlayCommands(snapshot.overlayCommandCount || 0)
  replayVectorCommands(snapshot.vectorCommandCount || 0)
  overlayDirty.value = snapshot.overlayDirty !== false
}

function detectOverlayDirty(overlayCommandCount, vectorCommandCount) {
  return Number(overlayCommandCount) > 0 || Number(vectorCommandCount) > 0
}

// 最终出图
function getCroppedCanvas() {
  if (!canvas.value || !screenshotImg.value) {
    throw new Error('截图源未就绪')
  }
  if (longshotResultActive.value && screenshotPixelCanvas && screenshotImg.value) {
    return getLongshotFullCanvas()
  }
  const drawCanvasWidth = Number(canvas.value.width) || 0
  const drawCanvasHeight = Number(canvas.value.height) || 0
  if (drawCanvasWidth <= 0 || drawCanvasHeight <= 0) {
    throw new Error('绘制画布尺寸无效')
  }
  const maxX = Math.max(0, drawCanvasWidth - 1)
  const maxY = Math.max(0, drawCanvasHeight - 1)
  const startX = Math.max(0, Math.min(maxX, Math.round(rect.x * dpr)))
  const startY = Math.max(0, Math.min(maxY, Math.round(rect.y * dpr)))
  const rawWidth = Math.max(1, Math.round(rect.width * dpr))
  const rawHeight = Math.max(1, Math.round(rect.height * dpr))
  const sourceWidth = Math.max(1, Math.min(rawWidth, drawCanvasWidth - startX))
  const sourceHeight = Math.max(1, Math.min(rawHeight, drawCanvasHeight - startY))
  const sourceX = Math.max(0, Math.min(startX, drawCanvasWidth - sourceWidth))
  const sourceY = Math.max(0, Math.min(startY, drawCanvasHeight - sourceHeight))
  if (sourceWidth <= 0 || sourceHeight <= 0) {
    throw new Error(
        `裁剪区域无效 rect=(${rect.x},${rect.y},${rect.width},${rect.height}) dpr=${dpr} canvas=${drawCanvasWidth}x${drawCanvasHeight}`
    )
  }

  const cropCanvas = document.createElement('canvas')
  cropCanvas.width = sourceWidth
  cropCanvas.height = sourceHeight
  const ctx = cropCanvas.getContext('2d')
  if (!ctx) {
    throw new Error('裁剪画布上下文创建失败')
  }

  const baseSource = (isPixelCtxReadable() && screenshotPixelCanvas)
      ? screenshotPixelCanvas
      : screenshotImg.value
  ctx.drawImage(
      baseSource,
      sourceX, sourceY, sourceWidth, sourceHeight,
      0, 0, sourceWidth, sourceHeight
  )

  const drawCtx = canvas.value.getContext('2d')
  if (!drawCtx) {
    throw new Error('绘制画布上下文获取失败')
  }
  const oldTransform = drawCtx.getTransform()
  drawCtx.resetTransform()
  const overlayData = drawCtx.getImageData(sourceX, sourceY, sourceWidth, sourceHeight)
  drawCtx.setTransform(oldTransform)

  const tempCanvas = document.createElement('canvas')
  tempCanvas.width = sourceWidth
  tempCanvas.height = sourceHeight
  const tempCtx = tempCanvas.getContext('2d')
  if (!tempCtx) {
    throw new Error('临时画布上下文创建失败')
  }
  tempCtx.putImageData(overlayData, 0, 0)

  ctx.drawImage(tempCanvas, 0, 0)
  drawShapeItemsOnCroppedCanvas(ctx, sourceX, sourceY, sourceWidth, sourceHeight)
  drawTextItemsOnCroppedCanvas(ctx, sourceX, sourceY, sourceWidth, sourceHeight)

  return cropCanvas
}

async function close() {
  try {

    cancelManualLongshotCapture(false, false)
    await invoke(IPC_COMMANDS.CLOSE_SCREENSHOT_WINDOW)
  } catch (error) {
    console.error('关闭窗口失败:', error)
  }
}

// 快捷键
function handleKeyDown(event) {
  if (event.ctrlKey && event.key === 'z') {
    event.preventDefault()
    undo()
    return
  }
  if (event.ctrlKey && event.key === 'y') {
    event.preventDefault()
    redo()
    return
  }
  if (currentTool.value === 'picker') {
    if (event.key === 'Shift' && !event.repeat) {
      event.preventDefault()
      pickerDisplayMode.value = pickerDisplayMode.value === 'hex' ? 'rgb' : 'hex'
      return
    }
    if (event.key === 'Control' && !event.repeat) {
      event.preventDefault()
      copyPickedColor()
      return
    }
  }
  if (event.key !== 'Escape') return

  if (editingTextId.value !== null) {
    event.preventDefault()
    cancelInlineEdit()
    return
  }
  if (regionSelectMode.value === 'manual_longshot' && manualLongshotSessionId.value > 0) {
    event.preventDefault()
    cancelManualLongshotCapture(true)
    return
  }
  if (state.value === 'selected' || state.value === 'selecting') {
    cancelSelection()
  } else if (state.value === 'drawing') {
    // 丢弃未完成的笔画，避免直接关闭编辑器丢失标注
    activeRasterCommand = null
    state.value = 'selected'
  } else if (state.value === 'moving' || state.value === 'resizing' || state.value === 'text-moving'
      || state.value === 'shape-moving' || state.value === 'shape-resizing' || state.value === 'shape-point-moving') {
    state.value = 'selected'
  } else {
    close()
  }
}

</script>

<style src="./screenshot.scoped.css" scoped></style>
