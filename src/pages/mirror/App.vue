<template>
  <div class="mirror-page">
    <div class="toolbar">
      <div class="toolbar-left">
        <span
          class="status"
          :class="{ connected: connected }"
        >
          <span class="dot" />
          {{ statusText }}
        </span>
        <span
          v-if="connected"
          class="diag"
        >{{ diag.recv }} / {{ diag.decoded }}</span>
      </div>
      <div class="actions">
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
        <span class="divider" />
        <button
          class="btn"
          :disabled="!connected"
          title="旋转屏幕"
          @click="sendRotate"
        >
          <svg viewBox="0 0 24 24"><path d="M4.5 9a8 8 0 0 1 14.3-2.6M19.5 15a8 8 0 0 1-14.3 2.6M18.8 3.2v3.4h-3.4M5.2 20.8v-3.4h3.4" /></svg>
        </button>
        <button
          class="btn danger"
          :disabled="!connected"
          title="断开投屏"
          @click="disconnect"
        >
          <svg viewBox="0 0 24 24"><path d="M12 3v8M6.3 6.7a8 8 0 1 0 11.4 0" /></svg>
          <span class="btn-text">断开</span>
        </button>
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
    </div>
  </div>
</template>

<script setup>
import { onMounted, onUnmounted, reactive, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { IPC_COMMANDS } from '../../services/ipc'   
import { getCurrentWindow } from '@tauri-apps/api/window'

const stageRef = ref(null)
const canvasRef = ref(null)
const connected = ref(false)
const errorMsg = ref('')
const statusText = ref('未连接')
const diag = reactive({ recv: 0, decoded: 0 })

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
  diag.recv++
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
    diag.decoded++
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

async function disconnect() {
  try { await invoke(IPC_COMMANDS.MIRROR_STOP) } catch { /* ignore */ }
  try { await getCurrentWindow().close() } catch { /* ignore */ }
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
    case 'session':
      videoW = msg.width
      videoH = msg.height
      break
    case 'stopped':
      connected.value = false
      statusText.value = '已断开'
      if (!stopped) errorMsg.value = '投屏会话已结束（设备断开或被停止）'
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

function connectWebSocket(port) {
  ws = new WebSocket(`ws://127.0.0.1:${port}/mirror`)
  ws.binaryType = 'arraybuffer'
  ws.onopen = () => {
    connected.value = true
    errorMsg.value = ''
    statusText.value = '已连接'
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
  if (ws) { try { ws.close() } catch { /* ignore */ } ws = null }
  if (decoder) { try { decoder.close() } catch { /* ignore */ } decoder = null }
})
</script>

<style scoped>
.mirror-page {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: radial-gradient(ellipse at 50% -20%, #1a222c 0%, #0b0e12 60%);
  color: #dfe5ec;
  font-family: 'Segoe UI', 'Microsoft YaHei', sans-serif;
  user-select: none;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 14px;
  background: rgba(20, 24, 30, 0.85);
  border-bottom: 1px solid #20262e;
  flex: 0 0 auto;
  backdrop-filter: blur(6px);
}

.toolbar-left {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.status {
  font-size: 13px;
  color: #8a97a5;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 4px 12px;
  border-radius: 999px;
  background: #171c22;
  border: 1px solid #242b33;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.status .dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #5b6773;
  flex: 0 0 auto;
}

.status.connected {
  color: #dfe5ec;
  border-color: #2a4a37;
}

.status.connected .dot {
  background: #4cd47c;
  box-shadow: 0 0 6px #4cd47c;
  animation: pulse 2s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.45; }
}

.diag {
  font-size: 11px;
  color: #5b6773;
  font-family: Consolas, monospace;
  white-space: nowrap;
}

.actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 34px;
  height: 30px;
  background: #1a2027;
  color: #b7c2cd;
  border: 1px solid #262f39;
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
  background: #242d37;
  color: #ffffff;
  border-color: #3a4552;
}

.btn:active:not(:disabled) {
  background: #2c3742;
  transform: translateY(1px);
}

.btn:disabled {
  opacity: 0.35;
  cursor: default;
}

.btn.danger {
  width: auto;
  padding: 0 12px;
  color: #ff8f8f;
  border-color: #4a2c2c;
}

.btn.danger:hover:not(:disabled) {
  background: #33201f;
  color: #ffb3b3;
  border-color: #6e3a3a;
}

.btn-text {
  font-size: 13px;
}

.divider {
  width: 1px;
  height: 18px;
  background: #2a323c;
  margin: 0 4px;
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
  border: 1px solid #232a33;
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
  color: #67737f;
  font-size: 14px;
  padding: 0 24px;
  text-align: center;
  line-height: 1.6;
}

.placeholder-icon svg {
  width: 44px;
  height: 44px;
  fill: none;
  stroke: #39434e;
  stroke-width: 1.6;
  stroke-linecap: round;
}
</style>
