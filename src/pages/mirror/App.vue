<template>
  <div
    class="mirror-page"
    :class="[theme, { fullscreen: isFs, 'bar-show': barShow }]"
    @mousemove="onMouseMove"
  >
    <div
      class="toolbar"
      data-tauri-drag-region
    >
      <div
        class="toolbar-left"
        data-tauri-drag-region
      >
        <span
          class="status"
          :class="{ connected: connected }"
        >
          <span class="dot" />
          <span
            v-if="!connected"
            class="status-text"
          >{{ statusText }}</span>
        </span>
      </div>
      <div class="actions">
        <div class="seg">
          <button
            class="btn"
            :disabled="!connected"
            title="返回"
            @click="sendBack"
          >
            <svg viewBox="0 0 24 24"><path d="M15.5 4.5 8 12l7.5 7.5" /></svg>
          </button>
          <button
            class="btn"
            :disabled="!connected"
            title="主页"
            @click="sendHome"
          >
            <svg viewBox="0 0 24 24"><circle
              cx="12"
              cy="12"
              r="7.5"
            /></svg>
          </button>
          <button
            class="btn"
            :disabled="!connected"
            title="最近任务"
            @click="sendRecents"
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
            class="btn"
            :disabled="!connected"
            title="旋转屏幕"
            @click="sendRotate"
          >
            <svg viewBox="0 0 24 24"><path d="M4.5 9a8 8 0 0 1 14.3-2.6M19.5 15a8 8 0 0 1-14.3 2.6M18.8 3.2v3.4h-3.4M5.2 20.8v-3.4h3.4" /></svg>
          </button>
          <button
            class="btn"
            :disabled="!connected"
            title="导出手机最近文件/图片到电脑"
            @click="onExport"
          >
            <svg viewBox="0 0 24 24"><path d="M12 4v10M8 10.5 12 14.5l4-4M5 19h14" /></svg>
          </button>
        </div>
        <div class="seg win">
          <button
            v-if="!isFs"
            class="btn"
            title="最小化"
            @click="minimizeWindow"
          >
            <svg viewBox="0 0 24 24"><path d="M6 12h12" /></svg>
          </button>
          <button
            class="btn"
            :title="isFs ? '退出全屏' : '铺满屏幕'"
            @click="toggleFullscreen"
          >
            <svg
              v-if="!isFs"
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
            class="btn close"
            title="关闭投屏"
            @click="closeWindow"
          >
            <svg viewBox="0 0 24 24"><path d="m6 6 12 12M18 6 6 18" /></svg>
          </button>
        </div>
      </div>
    </div>

    <div
      ref="stageRef"
      class="stage"
    >
      <div
        class="device"
        :class="{ live: connected }"
      >
        <canvas
          ref="canvasRef"
          class="screen"
          @pointerdown="onPointerDown"
          @pointermove="onPointerMove"
          @pointerup="onPointerUp"
          @pointercancel="onPointerUp"
          @wheel.prevent="onWheel"
          @contextmenu.prevent
        />
      </div>
      <input
        ref="kbdRef"
        class="kbd-capture"
        type="text"
        autocomplete="off"
        @compositionend="onCompositionEnd"
        @input="onKbdInput"
      >
      <div
        v-if="dragging"
        class="drag-overlay"
      >
        松开传输到手机
      </div>
      <div
        v-if="toastMsg"
        class="toast"
      >
        {{ toastMsg }}
      </div>
      <div
        v-if="!connected"
        class="placeholder"
      >
        <div class="placeholder-icon">
          <svg viewBox="0 0 24 24"><rect
            x="7"
            y="2.5"
            width="10"
            height="19"
            rx="2.5"
          /><path d="M10.5 19h3" /></svg>
        </div>
        <template v-if="errorMsg">
          {{ errorMsg }}
        </template>
        <template v-else>
          正在连接设备…
        </template>
      </div>
      <div
        v-if="exportOpen"
        class="export-backdrop"
        @click.self="exportOpen = false"
      >
        <div class="export-panel">
          <div class="export-head">
            <span class="export-title">从手机导出文件</span>
            <button
              class="btn"
              title="关闭"
              @click="exportOpen = false"
            >
              <svg viewBox="0 0 24 24"><path d="m6 6 12 12M18 6 6 18" /></svg>
            </button>
          </div>
          <div class="export-list">
            <div class="export-tip">
              勾选文件后点「导出所选」即可保存到电脑
            </div>
            <div class="export-list">
              <template v-if="!exportLoading">
                <template
                  v-for="g in exportGroups"
                  :key="g.dir"
                >
                  <div class="export-dir">
                    <span>{{ g.dir }}</span>
                    <span
                      class="sel-all"
                      @click="toggleGroup(g)"
                    >{{ g.allSelected ? '取消全选' : '全选' }}</span>
                  </div>
                  <label
                    v-for="f in g.files"
                    :key="f.path"
                    class="export-item"
                  >
                    <input
                      v-model="selectedPaths"
                      type="checkbox"
                      :value="f.path"
                    >
                    <span class="fname">{{ f.name }}</span>
                    <span class="fsize">{{ fmtSize(f.size) }}</span>
                  </label>
                  <div
                    v-if="!g.files.length"
                    class="export-empty"
                  >
                    （无文件）
                  </div>
                </template>
              </template>
              <div
                v-else
                class="export-empty"
              >
                正在读取手机文件列表…
              </div>
            </div>
            <div class="export-foot">
              <span class="sel-count">已选 {{ selectedPaths.length }} 项</span>
              <button
                class="btn"
                title="刷新列表"
                @click="refreshExport"
              >
                <svg viewBox="0 0 24 24"><path d="M4.5 9a8 8 0 0 1 14.3-2.6M19.5 15a8 8 0 0 1-14.3 2.6M18.8 3.2v3.4h-3.4M5.2 20.8v-3.4h3.4" /></svg>
              </button>
              <button
                class="btn primary"
                :disabled="!selectedPaths.length"
                @click="confirmExport"
              >
                导出所选
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { onMounted, onUnmounted, computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { IPC_COMMANDS } from '../../services/ipc'   
import { getCurrentWindow } from '@tauri-apps/api/window'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { useTheme } from '../../composables/useTheme'
import { initTheme } from '../../utils/themeManager'

const { currentTheme: theme } = useTheme()
let unlistenResize = null
let unlistenDrag = null

const stageRef = ref(null)
const canvasRef = ref(null)
const kbdRef = ref(null)
const connected = ref(false)
const errorMsg = ref('')
const statusText = ref('未连接')
const isFs = ref(false)
const barShow = ref(true)
const dragging = ref(false)
const toastMsg = ref('')
let toastTimer = null
const exportOpen = ref(false)
const exportLoading = ref(false)
const phoneFiles = ref([])
const selectedPaths = ref([])

// ---- 状态 ----
let ws = null
let decoder = null
let videoW = 0
let videoH = 0
let pointerDown = false
let timestampUs = 0
let stopped = false

// ---- H264 annexb 工具 ----
// 找出所有起始码（00 00 01 / 00 00 00 01）的结束位置
function findStartCodeEnds(data) {
  const ends = []
  const n = data.length
  for (let i = 0; i + 2 < n; i++) {
    if (data[i] === 0 && data[i + 1] === 0 && data[i + 2] === 1) {
      ends.push(i + 3)
      i += 2
    }
  }
  return ends
}

// 从 Annex-B 流中切出 NALU（去掉起始码）
function splitNalus(data) {
  const ends = findStartCodeEnds(data)
  const nalus = []
  for (let k = 0; k < ends.length; k++) {
    const start = ends[k]
    let end = data.length
    if (k + 1 < ends.length) {
      // 下一个起始码的起点（兼容 3 字节与 4 字节起始码）
      let sc = ends[k + 1] - 3
      if (sc > start && data[sc - 1] === 0) sc -= 1
      end = sc
    }
    if (end > start) nalus.push(data.subarray(start, end))
  }
  return nalus
}

function naluType(nalu) {
  return nalu.length > 0 ? nalu[0] & 0x1f : -1
}

// 用 SPS/PPS 构造 avcC description（WebCodecs "avc1.PPCCLL" 需要）
function buildAvcC(sps, pps) {
  const out = new Uint8Array(8 + 2 + sps.length + 1 + 2 + pps.length)
  let o = 0
  out[o++] = 1 // configurationVersion
  out[o++] = sps[1] // AVCProfileIndication
  out[o++] = sps[2] // profile_compatibility
  out[o++] = sps[3] // AVCLevelIndication
  out[o++] = 0xff // lengthSizeMinusOne = 3
  out[o++] = 0xe1 // reserved(3)=111 + numOfSequenceParameterSets=1
  out[o++] = (sps.length >> 8) & 0xff
  out[o++] = sps.length & 0xff
  out.set(sps, o)
  o += sps.length
  out[o++] = 1 // numOfPictureParameterSets
  out[o++] = (pps.length >> 8) & 0xff
  out[o++] = pps.length & 0xff
  out.set(pps, o)
  return out
}

function codecStringFromSps(sps) {
  const hex = (v) => v.toString(16).padStart(2, '0')
  return 'avc1.' + hex(sps[1]) + hex(sps[2]) + hex(sps[3])
}

function ensureDecoder(configPacket) {
  const nalus = splitNalus(configPacket)
  const sps = nalus.find((n) => naluType(n) === 7)
  const pps = nalus.find((n) => naluType(n) === 8)
  if (!sps || !pps) return

  const config = {
    codec: codecStringFromSps(sps),
    description: buildAvcC(sps, pps),
    optimizeForLatency: true,
  }
  if (decoder) {
    try { decoder.close() } catch { /* ignore */ }
    decoder = null
  }
  decoder = new VideoDecoder({
    output: (frame) => {
      drawFrame(frame)
    },
    error: (e) => {
      console.error('[mirror] decoder error:', e)
      // 解码器出错：销毁，等下一个关键帧重新配置
      try { decoder.close() } catch { /* ignore */ }
      decoder = null
    },
  })
  decoder.configure(config)
}

function drawFrame(frame) {
  const canvas = canvasRef.value
  if (!canvas) {
    frame.close()
    return
  }
  if (canvas.width !== frame.displayWidth || canvas.height !== frame.displayHeight) {
    canvas.width = frame.displayWidth
    canvas.height = frame.displayHeight
    if (!videoW) {
      videoW = frame.displayWidth
      videoH = frame.displayHeight
    }
  }
  const ctx = canvas.getContext('2d')
  ctx.drawImage(frame, 0, 0, frame.displayWidth, frame.displayHeight)
  frame.close()
}

// Annex-B → AVCC：description（avcC）模式下 WebCodecs 要求数据为
// 4 字节大端长度前缀 + NALU，而非 00 00 00 01 起始码。
// keyFrame 时剥掉内嵌 SPS/PPS（类型 7/8，已通过 description 提供）。
function annexbToAvcc(payload, stripParamSets) {
  const nalus = splitNalus(payload)
  const filtered = stripParamSets
    ? nalus.filter((n) => naluType(n) !== 7 && naluType(n) !== 8)
    : nalus
  let total = 0
  for (const n of filtered) total += 4 + n.length
  const out = new Uint8Array(total)
  let o = 0
  for (const n of filtered) {
    const len = n.length
    out[o++] = (len >>> 24) & 0xff
    out[o++] = (len >>> 16) & 0xff
    out[o++] = (len >>> 8) & 0xff
    out[o++] = len & 0xff
    out.set(n, o)
    o += len
  }
  return out
}

function feedChunk(payload, keyFrame) {
  if (!decoder || decoder.state !== 'configured') return
  const data = annexbToAvcc(payload, keyFrame)
  if (!data.length) return
  timestampUs += 33333 // 30fps 节奏的单调时间戳
  try {
    decoder.decode(new EncodedVideoChunk({
      type: keyFrame ? 'key' : 'delta',
      timestamp: timestampUs,
      data,
    }))
  } catch (e) {
    console.error('[mirror] decode failed:', e)
  }
}

// ---- 控制指令 ----
function sendControl(obj) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(obj))
  }
}

function videoPoint(event) {
  const canvas = canvasRef.value
  const rect = canvas.getBoundingClientRect()
  // 画布 CSS 保持宽高比 letterbox，计算可视图像的缩放与偏移
  const scale = Math.min(rect.width / videoW, rect.height / videoH)
  const dispW = videoW * scale
  const dispH = videoH * scale
  const originX = rect.left + (rect.width - dispW) / 2
  const originY = rect.top + (rect.height - dispH) / 2
  const x = Math.max(0, Math.min(videoW - 1, Math.round((event.clientX - originX) / scale)))
  const y = Math.max(0, Math.min(videoH - 1, Math.round((event.clientY - originY) / scale)))
  return { x, y }
}

function onPointerDown(event) {
  if (!connected.value || !videoW) return
  if (event.button === 2) return // 右键不注入手机
  canvasRef.value.setPointerCapture(event.pointerId)
  pointerDown = true
  const { x, y } = videoPoint(event)
  sendControl({ kind: 'touch', action: 0, x, y, w: videoW, h: videoH, pressure: 1.0 })
}

function onPointerMove(event) {
  if (!connected.value || !pointerDown || !videoW) return
  const { x, y } = videoPoint(event)
  sendControl({ kind: 'touch', action: 2, x, y, w: videoW, h: videoH, pressure: 1.0 })
}

function onPointerUp(event) {
  // mousedown 的默认焦点行为会把焦点抢回 canvas，必须在 pointerup（默认行为之后）再聚焦
  if (connected.value && videoW) refocusKbd()
  if (!connected.value || !pointerDown || !videoW) return
  pointerDown = false
  const { x, y } = videoPoint(event)
  sendControl({ kind: 'touch', action: 1, x, y, w: videoW, h: videoH, pressure: 1.0 })
}

function onWheel(event) {
  if (!connected.value || !videoW) return
  const { x, y } = videoPoint(event)
  // 一格滚轮 = 1；向下滚 → 内容向下（Android 负值）
  const vCells = event.deltaY > 0 ? -1 : 1
  const hCells = event.deltaX > 0 ? -1 : (event.deltaX < 0 ? 1 : 0)
  sendControl({ kind: 'scroll', x, y, w: videoW, h: videoH, dx: hCells, dy: vCells })
}

// Android keycode（scrcpy 注入需要 DOWN/UP 两次调用）
function sendKeycode(code) {
  sendControl({ kind: 'keycode', action: 0, keycode: code, repeat: 0, metaState: 0 })
  sendControl({ kind: 'keycode', action: 1, keycode: code, repeat: 0, metaState: 0 })
}
const sendBack = () => sendKeycode(4)       // KEYCODE_BACK
const sendHome = () => sendKeycode(3)       // KEYCODE_HOME
const sendRecents = () => sendKeycode(187)  // KEYCODE_APP_SWITCH
const sendRotate = () => sendControl({ kind: 'rotate' })

// ---- 电脑键盘输入文字 ----
// scrcpy 单条 INJECT_TEXT 上限 300 字符，按码点分块。
// 注意：INJECT_TEXT 走键盘字符映射，只支持 ASCII；中文等走
// 「写设备剪贴板 → 等回显确认 → 注入 KEYCODE_PASTE」通道。
// 实测（HyperOS）：PASTE 粘贴有效；Ctrl+V 会被手机输入法拦截（勿用）。
function sendText(text) {
  if (!text || !connected.value) return
  const chars = [...text]
  for (let i = 0; i < chars.length; i += 200) {
    const chunk = chars.slice(i, i + 200).join('')
    if (/^[\x20-\x7e\n\r\t]*$/.test(chunk)) {
      sendControl({ kind: 'text', text: chunk })
    } else {
      sendDebug(`sendText 中文入队: "${chunk.slice(0, 20)}"`)
      pasteQueue.value.push(chunk)
      if (pasteQueue.value.length === 1) processPasteQueue()
    }
  }
}

// viewer 侧关键事件 → relay 日志（排障埋点，经中继 WARN 落盘）
function sendDebug(msg) {
  sendControl({ kind: 'debug', msg })
}

// 粘贴队列：写剪贴板 → 等设备回显确认内容已落 → 注入 KEYCODE_PASTE
const pasteQueue = ref([])
let pasteWatch = null

function processPasteQueue() {
  if (!pasteQueue.value.length) return
  const chunk = pasteQueue.value[0]
  sendDebug(`set-clipboard 发出: "${chunk.slice(0, 20)}"`)
  sendControl({ kind: 'set-clipboard', paste: false, sequence: Date.now(), text: chunk })
  if (pasteWatch) clearTimeout(pasteWatch)
  // 兜底：回显异常丢失时 3s 后强制粘贴（避免卡队）
  pasteWatch = setTimeout(() => firePaste(), 3000)
}

function onClipboardEcho(text) {
  if (!pasteQueue.value.length) return false
  const chunk = pasteQueue.value[0]
  if (text && (text === chunk || text.endsWith(chunk))) {
    sendDebug('回显确认匹配，触发 PASTE')
    if (pasteWatch) clearTimeout(pasteWatch)
    firePaste()
    return true
  }
  return false
}

function firePaste() {
  pasteQueue.value.shift()
  sendDebug('KEYCODE_PASTE 注入')
  // KEYCODE_PASTE = 279
  sendControl({ kind: 'keycode', action: 0, keycode: 279, repeat: 0, metaState: 0 })
  setTimeout(() => sendControl({ kind: 'keycode', action: 1, keycode: 279, repeat: 0, metaState: 0 }), 40)
  if (pasteQueue.value.length) setTimeout(processPasteQueue, 300)
}

// 特殊键 → Android keycode（Enter/Backspace/方向键等）
const KEY_MAP = {
  Enter: 66,
  Backspace: 67,
  Tab: 61,
  Delete: 112,
  ArrowUp: 19,
  ArrowDown: 20,
  ArrowLeft: 21,
  ArrowRight: 22,
  Home: 122,
  End: 123,
  PageUp: 92,
  PageDown: 93,
}

// window 级键盘捕获：不依赖输入框焦点
function onKeydown(e) {
  if (e.key === 'Escape') {
    if (isFs.value) getCurrentWindow().setFullscreen(false).catch(() => {})
    return
  }
  if (!connected.value) return
  // 焦点在工具栏按钮上时保留按钮语义（空格/回车激活按钮）
  if (e.target && e.target.closest && e.target.closest('.toolbar')) return
  if (e.isComposing || e.key === 'Process') return // IME 组合中，等 compositionend
  if ((e.ctrlKey || e.metaKey) && (e.key === 'v' || e.key === 'V')) {
    e.preventDefault()
    handlePaste()
    return
  }
  if (e.ctrlKey || e.metaKey || e.altKey) return
  if (e.key.length === 1) {
    e.preventDefault()
    sendText(e.key)
    return
  }
  const code = KEY_MAP[e.key]
  if (code) {
    e.preventDefault()
    sendKeycode(code)
  }
}

function onCompositionEnd(e) {
  sendDebug(`compositionend: "${e.data}"`)
  sendText(e.data)
  if (kbdRef.value) kbdRef.value.value = ''
}

function onKbdInput(e) {
  // 输入框只作 IME 组合捕获面。组合过程中严禁清空 value（会打断 IME 组合，
  // 导致 compositionend 不触发、中文丢失）；组合结束由 onCompositionEnd 清空。
  if (e && (e.isComposing || (e.inputType && e.inputType.startsWith('insertComposition')))) return
  if (kbdRef.value) kbdRef.value.value = ''
}

async function handlePaste() {
  try {
    const text = await invoke(IPC_COMMANDS.CLIPBOARD_READ_TEXT)
    if (text) sendText(text)
  } catch { /* ignore */ }
}

function refocusKbd() {
  if (connected.value && kbdRef.value) kbdRef.value.focus({ preventScroll: true })
}

// ---- 拖文件/图片到窗口：adb push 到手机 ----
function showToast(msg, ms = 3000) {
  toastMsg.value = msg
  if (toastTimer) clearTimeout(toastTimer)
  toastTimer = setTimeout(() => { toastMsg.value = '' }, ms)
}

async function pushFile(path) {
  if (!connected.value) return
  const name = path.split(/[\\/]/).pop() || path
  showToast(`正在传输 ${name} …`, 10000)
  try {
    const r = await invoke(IPC_COMMANDS.MIRROR_PUSH_FILE, { path })
    showToast(`已传输到手机：${r.dest}`)
  } catch (err) {
    showToast(`传输失败：${String(err)}`, 5000)
  }
}

async function onExport() {
  exportOpen.value = true
  refreshExport()
}

async function refreshExport() {
  exportLoading.value = true
  selectedPaths.value = []
  try {
    const r = await invoke(IPC_COMMANDS.MIRROR_LIST_PHONE_FILES)
    phoneFiles.value = r.files || []
  } catch (err) {
    showToast(`读取手机文件列表失败：${String(err)}`, 5000)
    phoneFiles.value = []
  } finally {
    exportLoading.value = false
  }
}

const exportGroups = computed(() => {
  const map = new Map()
  for (const f of phoneFiles.value) {
    if (!map.has(f.dir)) map.set(f.dir, [])
    map.get(f.dir).push(f)
  }
  return [...map.entries()].map(([dir, files]) => {
    const all = files.length > 0 && files.every((f) => selectedPaths.value.includes(f.path))
    return { dir, files, allSelected: all }
  })
})

function toggleGroup(g) {
  const paths = g.files.map((f) => f.path)
  if (g.allSelected) {
    selectedPaths.value = selectedPaths.value.filter((p) => !paths.includes(p))
  } else {
    selectedPaths.value = [...new Set([...selectedPaths.value, ...paths])]
  }
}

function fmtSize(n) {
  if (!n) return ''
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`
}

async function confirmExport() {
  if (!selectedPaths.value.length) return
  try {
    const dir = await openDialog({ directory: true, title: '选择导出保存位置' })
    if (!dir) return
    showToast(`正在导出 ${selectedPaths.value.length} 个文件…`, 20000)
    const r = await invoke(IPC_COMMANDS.MIRROR_PULL_FILES, {
      paths: selectedPaths.value,
      saveDir: dir,
    })
    const n = (r.exported || []).length
    const failed = r.failed || 0
    showToast(
      failed > 0
        ? `导出完成：成功 ${n} 个，失败 ${failed} 个（保存于 ${r.saveDir}）`
        : `已导出 ${n} 个文件到 ${r.saveDir}`,
      6000,
    )
    exportOpen.value = false
  } catch (err) {
    showToast(`导出失败：${String(err)}`, 5000)
  }
}

async function closeWindow() {
  stopped = true
  // 关窗即断开：后端监听窗口 Destroyed 会停止会话
  try { await getCurrentWindow().close() } catch { /* ignore */ }
}

async function minimizeWindow() {
  try { await getCurrentWindow().minimize() } catch { /* ignore */ }
}

// 最大化键语义：在「铺满屏幕」（真全屏）与窗口化之间切换
async function toggleFullscreen() {
  try {
    const win = getCurrentWindow()
    if (await win.isFullscreen()) {
      await win.setFullscreen(false)
    } else {
      await win.setFullscreen(true)
    }
  } catch { /* ignore */ }
}

function onMouseMove(event) {
  if (!isFs.value) {
    barShow.value = true
    return
  }
  barShow.value = event.clientY <= 48
}

// ---- 窗口随视频方向横竖切换 ----
async function syncWindowOrientation() {
  if (!videoW || !videoH) return
  const wantLandscape = videoW > videoH
  const win = getCurrentWindow()
  try {
    const size = await win.innerSize()
    const scale = await win.scaleFactor()
    const curW = size.width / scale
    const curH = size.height / scale
    const isLandscape = curW > curH
    if (isLandscape !== wantLandscape) {
      // 宽高互换，保持用户当前窗口大小比例
      const w = Math.max(288, Math.round(curH))
      const h = Math.max(288, Math.round(curW))
      await win.setSize(new LogicalSize(w, h))
    }
  } catch { /* 窗口操作失败不影响画面 */ }
}

// ---- WS 生命周期 ----
function handleText(text) {
  let msg
  try { msg = JSON.parse(text) } catch { return }
  switch (msg.type) {
    case 'device':
      statusText.value = `已连接 ${msg.name}`
      break
    case 'codec':
      // h264 确认，等待 session/config
      break
    case 'session': {
      const changed = videoW !== msg.width || videoH !== msg.height
      videoW = msg.width
      videoH = msg.height
      if (changed) syncWindowOrientation()
      break
    }
    case 'stopped':
      connected.value = false
      statusText.value = '已断开'
      if (!stopped) {
        // 控制链路断开/流结束/外部停止（如设备切换）：自动重建会话（用户无感恢复）
        if (
          msg.reason === 'control-writer'
          || msg.reason === 'control-reader'
          || msg.reason === 'stream-eof'
          || msg.reason === 'stopped'
        ) {
          recoverSession()
        } else {
          errorMsg.value = '投屏会话已结束（设备断开或被停止）'
        }
      }
      break
    case 'clipboard':
      // 优先消费粘贴队列的写入确认；非队列触发的回显 = 手机端复制 → 提示
      if (!onClipboardEcho(String(msg.text || '')) && msg.text) {
        showToast(`手机剪贴板：${String(msg.text).slice(0, 40)}`, 2500)
      }
      break
    default:
      break
  }
}

function handleBinary(buffer) {
  const data = new Uint8Array(buffer)
  if (data.length < 1) return
  const flags = data[0]
  const payload = data.subarray(1)
  const isConfig = (flags & 1) !== 0
  const isKey = (flags & 2) !== 0
  if (isConfig) {
    ensureDecoder(payload)
    return
  }
  if (isKey && (!decoder || decoder.state !== 'configured')) {
    // 中途加入错过 config 包：关键帧自带 SPS/PPS，直接重建解码器
    ensureDecoder(payload)
  }
  feedChunk(payload, isKey)
}

// ---- 会话自动恢复：控制链路/视频流异常断开时重建会话 ----
let recovering = false
let lastRecoverAt = 0

async function recoverSession() {
  if (stopped || recovering) return
  const now = Date.now()
  if (now - lastRecoverAt < 30000) {
    errorMsg.value = '投屏连接中断，自动恢复间隔中（可手动停止后重新开启投屏）'
    return
  }
  lastRecoverAt = now
  recovering = true
  showToast('投屏连接中断，正在自动恢复…', 4000)
  try {
    if (ws) { try { ws.close() } catch { /* ignore */ } ws = null }
    if (decoder) { try { decoder.close() } catch { /* ignore */ } decoder = null }
    videoW = 0
    videoH = 0
    await invoke(IPC_COMMANDS.MIRROR_STOP)
    const info = await invoke(IPC_COMMANDS.MIRROR_START)
    connectWebSocket(info.relayPort)
  } catch (e) {
    errorMsg.value = `自动恢复失败：${String(e)}（请手动停止后重新开启投屏）`
  } finally {
    recovering = false
  }
}

// ---- 会话看门狗：断连状态下每 3s 尝试重连 ----
// 覆盖自动恢复失败后的场景（如 USB 切无线期间无设备），后端会话重建好后
// MIRROR_START 走复用分支返回新中继端口，窗口自动接上画面。
let watchdogTimer = null

async function watchdogTick() {
  if (stopped || connected.value || recovering) return
  try {
    if (ws) { try { ws.close() } catch { /* ignore */ } ws = null }
    const info = await invoke(IPC_COMMANDS.MIRROR_START)
    connectWebSocket(info.relayPort)
  } catch { /* 会话未就绪（无设备/启动中），等下一轮 */ }
}

function connectWebSocket(port) {
  ws = new WebSocket(`ws://127.0.0.1:${port}/mirror`)
  ws.binaryType = 'arraybuffer'
  ws.onopen = () => {
    connected.value = true
    errorMsg.value = ''
    statusText.value = '已连接'
    refocusKbd() // 为 IME 组合输入聚焦捕获框
  }
  ws.onmessage = (event) => {
    if (typeof event.data === 'string') {
      handleText(event.data)
    } else {
      handleBinary(event.data)
    }
  }
  ws.onclose = () => {
    connected.value = false
    if (!stopped) errorMsg.value = '与投屏服务的连接已断开'
    statusText.value = '未连接'
  }
  ws.onerror = () => {}
}

onMounted(async () => {
  initTheme() // 同步后端主题并应用（跨窗口 storage 事件由 useTheme 监听）
  try {
    unlistenResize = await getCurrentWindow().onResized(async () => {
      try { isFs.value = await getCurrentWindow().isFullscreen() } catch { /* ignore */ }
    })
  } catch { /* ignore */ }
  window.addEventListener('keydown', onKeydown)
  try {
    unlistenDrag = await getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload
      if (p.type === 'over') {
        dragging.value = connected.value
      } else if (p.type === 'drop') {
        dragging.value = false
        ;(p.paths || []).forEach((f) => pushFile(f))
      } else {
        dragging.value = false
      }
    })
  } catch { /* ignore */ }
  refocusKbd()
  window.addEventListener('focus', refocusKbd)
  watchdogTimer = setInterval(watchdogTick, 3000)
  try {
    const info = await invoke(IPC_COMMANDS.MIRROR_START)
    connectWebSocket(info.relayPort)
  } catch (e) {
    stopped = true
    errorMsg.value = String(e)
    statusText.value = '启动失败'
  }
})

onUnmounted(() => {
  stopped = true
  if (watchdogTimer) { clearInterval(watchdogTimer); watchdogTimer = null }
  if (unlistenResize) { unlistenResize(); unlistenResize = null }
  if (unlistenDrag) { unlistenDrag(); unlistenDrag = null }
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('focus', refocusKbd)
  if (toastTimer) clearTimeout(toastTimer)
  if (ws) { try { ws.close() } catch { /* ignore */ } ws = null }
  if (decoder) { try { decoder.close() } catch { /* ignore */ } decoder = null }
})
</script>

<style scoped>
/* ---- 主题令牌（跟随设置页 dark / light / eye-care）---- */
.mirror-page.dark {
  --bg: #0b0e12;
  --bg-hi: #1a222c;
  --panel: rgba(20, 24, 30, 0.85);
  --panel-border: #20262e;
  --text: #dfe5ec;
  --muted: #8a97a5;
  --btn-bg: #1a2027;
  --btn-border: #262f39;
  --btn-hover-bg: #242d37;
  --btn-hover-border: #3a4552;
  --btn-text: #b7c2cd;
  --accent: #4cd47c;
  --danger-text: #ff8f8f;
  --danger-border: #4a2c2c;
  --danger-hover-bg: #33201f;
  --danger-hover-text: #ffb3b3;
  --divider: #2a323c;
  --pill-bg: #171c22;
  --pill-border: #242b33;
}

.mirror-page.light {
  --bg: #eef1f5;
  --bg-hi: #f8fafc;
  --panel: rgba(255, 255, 255, 0.92);
  --panel-border: #d9dfe6;
  --text: #1f2730;
  --muted: #6a7683;
  --btn-bg: #f6f8fa;
  --btn-border: #d9dfe6;
  --btn-hover-bg: #e7ecf2;
  --btn-hover-border: #b9c3ce;
  --btn-text: #3c4653;
  --accent: #21a35c;
  --danger-text: #d34040;
  --danger-border: #ecc9c9;
  --danger-hover-bg: #f6dfdf;
  --danger-hover-text: #b02c2c;
  --divider: #d9dfe6;
  --pill-bg: #ffffff;
  --pill-border: #e3e8ee;
}

.mirror-page.eye-care {
  --bg: #c9dcc2;
  --bg-hi: #dcead6;
  --panel: rgba(243, 248, 238, 0.9);
  --panel-border: #adc2a5;
  --text: #2f3a2d;
  --muted: #62745c;
  --btn-bg: #d8e6d1;
  --btn-border: #b7c9af;
  --btn-hover-bg: #c9dcc1;
  --btn-hover-border: #a3b89b;
  --btn-text: #3a4a36;
  --accent: #2f9e5e;
  --danger-text: #a94434;
  --danger-border: #d4b3a8;
  --danger-hover-bg: #e3c8bd;
  --danger-hover-text: #8f3322;
  --divider: #b7c9af;
  --pill-bg: #e9f0e4;
  --pill-border: #b7c9af;
}

.mirror-page {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: radial-gradient(ellipse at 50% -20%, var(--bg-hi) 0%, var(--bg) 60%);
  color: var(--text);
  font-family: 'Segoe UI', 'Microsoft YaHei', sans-serif;
  user-select: none;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 14px;
  background: var(--panel);
  border-bottom: 1px solid var(--panel-border);
  flex: 0 0 auto;
}

.toolbar-left {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.status {
  font-size: 13px;
  color: var(--muted);
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 4px 12px;
  border-radius: 999px;
  background: var(--pill-bg);
  border: 1px solid var(--pill-border);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.status .dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--muted);
  flex: 0 0 auto;
}

.status.connected {
  color: var(--text);
  border-color: var(--accent);
}

.status.connected .dot {
  background: var(--accent);
  box-shadow: 0 0 6px var(--accent);
  animation: pulse 2s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.45; }
}

.actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* 分段式按钮组：手机功能键 / 窗口控制键 */
.seg {
  display: flex;
  align-items: stretch;
  border: 1px solid var(--btn-border);
  border-radius: 8px;
  overflow: hidden;
  background: var(--btn-bg);
}

.seg .btn {
  border: none;
  border-radius: 0;
  background: transparent;
}

.seg .btn:hover:not(:disabled) {
  background: var(--btn-hover-bg);
  color: var(--text);
}

.seg .btn:active:not(:disabled) {
  transform: none;
  background: var(--btn-hover-bg);
}

.seg .btn + .btn {
  border-left: 1px solid var(--btn-border);
}

.seg .btn.close:hover:not(:disabled) {
  background: var(--danger-hover-bg);
  color: var(--danger-hover-text);
}

.seg .btn.active {
  background: var(--accent);
  color: #fff;
}

.seg .btn.active:hover:not(:disabled) {
  background: var(--accent);
  opacity: 0.9;
}

.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 34px;
  height: 30px;
  background: var(--btn-bg);
  color: var(--btn-text);
  border: 1px solid var(--btn-border);
  border-radius: 8px;
  cursor: pointer;
  transition: background 0.15s, color 0.15s, border-color 0.15s;
}

.btn svg {
  width: 16px;
  height: 16px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.btn:hover:not(:disabled) {
  background: var(--btn-hover-bg);
  color: var(--text);
  border-color: var(--btn-hover-border);
}

.btn:active:not(:disabled) {
  transform: translateY(1px);
}

.btn:disabled {
  opacity: 0.35;
  cursor: default;
}

/* 全屏（铺满屏幕）：工具栏滑出隐藏，画面去 bezel 填满 */
.mirror-page.fullscreen .toolbar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  z-index: 10;
  transform: translateY(-110%);
  transition: transform 0.2s ease;
}

.mirror-page.fullscreen.bar-show .toolbar {
  transform: none;
}

.mirror-page.fullscreen .device {
  padding: 0;
  border: none;
  border-radius: 0;
  box-shadow: none;
}

.mirror-page.fullscreen .screen {
  max-width: 100vw;
  max-height: 100vh;
  border-radius: 0;
}

.stage {
  flex: 1;
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}

.device {
  display: none;
  padding: 8px;
  border-radius: 22px;
  background: #05070a;
  border: 1px solid var(--panel-border);
  box-shadow:
    0 0 0 1px rgba(255, 255, 255, 0.04) inset,
    0 18px 50px rgba(0, 0, 0, 0.55),
    0 4px 16px rgba(0, 0, 0, 0.4);
}

.device.live {
  display: block;
}

.screen {
  display: block;
  max-width: calc(100vw - 48px);
  max-height: calc(100vh - 100px);
  background: #000;
  cursor: pointer;
  touch-action: none;
  border-radius: 14px;
}

.placeholder {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  color: var(--muted);
  font-size: 14px;
  padding: 0 24px;
  text-align: center;
  line-height: 1.6;
}

.placeholder-icon svg {
  width: 44px;
  height: 44px;
  fill: none;
  stroke: var(--muted);
  stroke-width: 1.6;
  stroke-linecap: round;
  opacity: 0.6;
}

/* 键盘/IME 捕获输入框：不可见但保持焦点（需真实尺寸否则部分 IME 不附着） */
.kbd-capture {
  position: fixed;
  top: 0;
  left: 0;
  width: 40px;
  height: 20px;
  opacity: 0.01;
  border: none;
  padding: 0;
  pointer-events: none;
  z-index: -1;
}

/* 拖文件悬停提示 */
.drag-overlay {
  position: absolute;
  inset: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(10, 30, 20, 0.55);
  color: #fff;
  font-size: 18px;
  letter-spacing: 2px;
  pointer-events: none;
  border: 2px dashed var(--accent);
  border-radius: 14px;
}

/* 操作提示 toast */
.toast {
  position: absolute;
  bottom: 18px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 30;
  max-width: 80%;
  padding: 8px 16px;
  border-radius: 8px;
  background: rgba(15, 22, 28, 0.92);
  color: #dfe5ec;
  font-size: 13px;
  border: 1px solid #2a323c;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 从手机导出文件面板 */
.export-backdrop {
  position: fixed;
  inset: 0;
  z-index: 50;
  background: rgba(0, 0, 0, 0.45);
  display: flex;
  align-items: center;
  justify-content: center;
}

.export-panel {
  width: 88%;
  max-height: 82%;
  display: flex;
  flex-direction: column;
  background: var(--pill-bg);
  border: 1px solid var(--pill-border);
  border-radius: 12px;
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.5);
  overflow: hidden;
}

.export-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  border-bottom: 1px solid var(--pill-border);
}

.export-title {
  font-size: 14px;
  color: var(--text);
}

.export-tip {
  padding: 8px 8px 0;
  font-size: 11px;
  color: var(--muted);
}

.export-list {
  flex: 1;
  overflow-y: auto;
  padding: 6px 8px;
}

.export-dir {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 8px 4px;
  font-size: 12px;
  color: var(--muted);
  font-family: Consolas, monospace;
}

.sel-all {
  cursor: pointer;
  color: var(--accent);
}

.sel-all:hover {
  text-decoration: underline;
}

.export-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  color: var(--text);
  user-select: none;
}

.export-item:hover {
  background: var(--btn-hover-bg);
}

.export-item .fname {
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.export-item .fsize {
  flex: 0 0 auto;
  color: var(--muted);
  font-size: 11px;
  font-family: Consolas, monospace;
}

.export-empty {
  padding: 14px 10px;
  color: var(--muted);
  font-size: 12px;
  text-align: center;
}

.export-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border-top: 1px solid var(--pill-border);
}

.sel-count {
  flex: 1;
  font-size: 12px;
  color: var(--muted);
}

.btn.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: #08130c;
  width: auto;
  padding: 0 14px;
  font-size: 13px;
}

.btn.primary:hover:not(:disabled) {
  background: var(--accent);
  color: #08130c;
  opacity: 0.9;
}

.btn.primary:disabled {
  opacity: 0.4;
}
</style>
