<template>
  <div class="sync-section">
    <h3>{{ $t('settings.sync.title') }}</h3>

    <!-- 开关 -->
    <div class="sync-toggle-row">
      <div class="sync-toggle-text">
        <p>{{ $t('settings.sync.enableHint') }}</p>
      </div>
      <el-switch
        v-model="enabled"
        :loading="switching"
        :active-text="$t('settings.sync.enable')"
        @change="onToggle"
      />
    </div>

    <template v-if="status.running">
      <!-- 状态信息 -->
      <el-descriptions
        :column="2"
        border
        class="sync-status-card"
      >
        <el-descriptions-item :label="$t('settings.sync.pcName')">
          {{ status.pcName }}
        </el-descriptions-item>
        <el-descriptions-item :label="$t('settings.sync.port')">
          {{ status.port }}
        </el-descriptions-item>
      </el-descriptions>

      <!-- 配对码 -->
      <div class="pairing-block">
        <div class="pairing-code">
          {{ status.pairingCode || '------' }}
        </div>
        <div class="pairing-meta">
          <span v-if="status.pairingCode && status.pairingExpiresInSecs > 0">
            {{ $t('settings.sync.expiresIn', {secs: status.pairingExpiresInSecs}) }}
          </span>
          <span v-else>{{ $t('settings.sync.codeExpired') }}</span>
          <el-button
            size="small"
            @click="onRegenerate"
          >
            {{ $t('settings.sync.regenerate') }}
          </el-button>
        </div>
        <p class="hint">
          {{ $t('settings.sync.pairingCodeHint') }}
        </p>
      </div>

      <!-- 手动连接地址 -->
      <div
        v-if="address"
        class="address-row"
      >
        <span>{{ $t('settings.sync.address') }}：<code>{{ address }}</code></span>
        <el-button
          size="small"
          @click="copyAddress"
        >
          {{ $t('settings.sync.copyAddress') }}
        </el-button>
      </div>

      <!-- 已配对设备 -->
      <div
        v-if="status.paired"
        class="paired-row"
      >
        <el-tag type="success">
          {{ $t('settings.sync.paired') }}
        </el-tag>
        <el-button
          size="small"
          type="danger"
          @click="onUnpair"
        >
          {{ $t('settings.sync.unpair') }}
        </el-button>
        <span class="hint">{{ $t('settings.sync.unpairHint') }}</span>
      </div>
    </template>

    <!-- 使用说明 -->
    <div class="how-to">
      <h4>{{ $t('settings.sync.howToTitle') }}</h4>
      <p>{{ $t('settings.sync.howToStep1') }}</p>
      <p>{{ $t('settings.sync.howToStep2') }}</p>
      <p>{{ $t('settings.sync.howToStep3') }}</p>
      <p>{{ $t('settings.sync.howToStep4') }}</p>
    </div>

    <!-- 手机投屏 -->
    <div class="mirror-entry">
      <h4>手机投屏</h4>
      <p class="hint">
        将手机画面实时投屏到电脑，并支持鼠标点击、滑动等反向控制。
        连接前需在手机上<b>开启开发者模式</b>：USB 投屏需打开「USB 调试」，WiFi 首次连接需「无线调试」（Android 11+）或先插线切换。
      </p>
      <el-collapse class="mirror-guide">
        <el-collapse-item
          title="首次使用？展开查看手机设置步骤"
          name="guide"
        >
          <p>1. 手机「设置 → 关于手机」→ 连续点击「版本号」7 次，开启开发者模式</p>
          <p>2. 进入「设置 → 系统 → 开发者选项」</p>
          <p>3. USB 投屏：打开「USB 调试」，插上数据线，在手机弹窗中点「允许」</p>
          <p>4. WiFi 投屏（Android 11+）：打开「无线调试」；或先插线用「USB 一键切换无线」</p>
          <p>5. 中文输入：先在投屏画面上点击手机输入框，键盘弹出后再打字</p>
        </el-collapse-item>
      </el-collapse>

      <div class="mirror-modes">
        <div class="mode-card">
          <div class="mode-title">
            USB 投屏
            <el-tag
              size="small"
              type="success"
            >
              推荐
            </el-tag>
          </div>
          <p class="mode-desc">
            数据线连接，延迟低、画面稳定，不依赖 WiFi。
          </p>
          <el-button
            type="primary"
            :loading="mirrorStarting"
            @click="onStartMirror"
          >
            开启 USB 投屏
          </el-button>
        </div>
        <div class="mode-card">
          <div class="mode-title">
            WiFi 投屏
          </div>
          <p class="mode-desc">
            手机与电脑在同一 WiFi 下连接，无需数据线（画面延迟略高于 USB）。
            手机重启后无线模式失效，需重新插线切换或配对。
          </p>
          <div class="mode-actions">
            <el-button
              :loading="wirelessStarting"
              @click="onEnableWireless"
            >
              USB 一键切换无线
            </el-button>
            <el-button
              :loading="wirelessConnecting"
              @click="onConnectWireless"
            >
              输入 IP 连接
            </el-button>
          </div>
        </div>
      </div>

      <!-- 画质模式 -->
      <div class="quality-block">
        <div class="mode-title">
          画质模式
        </div>
        <p class="mode-desc">
          影响清晰度、流畅度与延迟；切换后下次开启投屏生效（USB 带宽充裕，可选更高档位）。
        </p>
        <div class="quality-row">
          <el-radio-group
            v-model="qualityMode"
            size="small"
            @change="onQualityChange"
          >
            <el-radio-button value="smooth">
              流畅
            </el-radio-button>
            <el-radio-button value="balanced">
              均衡
            </el-radio-button>
            <el-radio-button value="hd">
              高清 60fps
            </el-radio-button>
            <el-radio-button value="native">
              原生
            </el-radio-button>
          </el-radio-group>
          <span class="quality-hint">{{ qualityHint }}</span>
        </div>
      </div>

      <!-- 历史设备：多台手机快速切换 -->
      <template v-if="deviceHistory.length">
        <div class="history-title">
          历史设备
          <el-button
            size="small"
            text
            class="history-refresh"
            @click="refreshDeviceHistory"
          >
            刷新
          </el-button>
        </div>
        <div
          v-for="d in deviceHistory"
          :key="d.serial"
          class="history-device"
        >
          <span
            class="hd-name"
            :title="d.serial"
          >{{ d.name }}</span>
          <span
            class="hd-conn"
            :class="connLabel(d) === 'WiFi' ? 'is-wifi' : 'is-usb'"
          >{{ connLabel(d) }}</span>
          <span class="hd-time">{{ lastUsedLabel(d.lastUsedMs) }}</span>
          <span
            v-if="d.online"
            class="hd-online"
          >在线</span>
          <el-button
            size="small"
            type="primary"
            plain
            :loading="reconnecting === d.serial"
            @click="onReconnectDevice(d)"
          >
            连接并投屏
          </el-button>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup>
import {computed, onMounted, onUnmounted, reactive, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage, ElMessageBox} from 'element-plus'
import {SyncService, MirrorService} from '../../../services/ipc'
import {parseErrorMessage} from '../../../utils/errorHandler'

const {t} = useI18n()
const mirrorStarting = ref(false)
const wirelessStarting = ref(false)
const wirelessConnecting = ref(false)
const deviceHistory = ref([])
const reconnecting = ref('')
const qualityMode = ref('balanced')

const QUALITY_HINTS = {
  smooth: '长边 1280 · 30fps · 6Mbps — 省流量，适合弱网',
  balanced: '长边 1600 · 30fps · 8Mbps — 延迟与画质均衡（默认）',
  hd: '长边 1920 · 60fps · 16Mbps — 高刷顺滑，USB 首选',
  native: '原生分辨率 · 60fps · 24Mbps — 接近手机原生体验，建议 USB',
}
const qualityHint = computed(() => QUALITY_HINTS[qualityMode.value] || '')

// 保存画质模式（后端持久化，下次开启投屏生效）
async function onQualityChange(mode) {
  try {
    await MirrorService.setQuality(mode)
    ElMessage.success('画质模式已保存，重开投屏后生效')
  } catch (e) {
    ElMessage.error({message: parseErrorMessage(e) || String(e), duration: 5000})
  }
}

async function loadQuality() {
  try {
    const r = await MirrorService.getQuality()
    if (r && r.mode) qualityMode.value = r.mode
  } catch { /* 读取失败保持默认 */ }
}

// 拉取历史设备（含在线标记）
async function refreshDeviceHistory() {
  try {
    const r = await MirrorService.deviceHistory()
    deviceHistory.value = r.devices || []
  } catch { /* 忽略：历史不可用不影响主流程 */ }
}

// 连接方式徽标：WiFi / USB（兼容旧记录的 wireless 字段）
function connLabel(d) {
  return (d.connType === 'wireless' || d.wireless) ? 'WiFi' : 'USB'
}

// 最近使用时间（相对时间，如「5 分钟前」）
function lastUsedLabel(ms) {
  if (!ms) return ''
  const min = Math.floor((Date.now() - ms) / 60000)
  if (min < 1) return '刚刚'
  if (min < 60) return `${min} 分钟前`
  const h = Math.floor(min / 60)
  if (h < 24) return `${h} 小时前`
  return `${Math.floor(h / 24)} 天前`
}

// 历史设备一键重连：无线先 adb connect（幂等），再开启投屏
async function onReconnectDevice(d) {
  reconnecting.value = d.serial
  try {
    await MirrorService.reconnectDevice(d.serial)
    await startMirrorWithSerial(d.serial)
    refreshDeviceHistory()
  } catch (e) {
    ElMessage.error({message: parseErrorMessage(e) || String(e), duration: 6000})
  } finally {
    reconnecting.value = ''
  }
}

async function startMirrorWithSerial(serial) {
  mirrorStarting.value = true
  try {
    await MirrorService.start(serial || undefined)
    ElMessage.success('投屏已开启')
    refreshDeviceHistory()
  } catch (e) {
    ElMessage.error({message: String(e), duration: 5000})
  } finally {
    mirrorStarting.value = false
  }
}

async function onStartMirror() {
  await startMirrorWithSerial(null)
}

// USB 一键切换无线：tcpip 5555 + 读手机 WiFi IP + connect，随后直接开投屏
async function onEnableWireless() {
  wirelessStarting.value = true
  try {
    const r = await MirrorService.enableWireless()
    ElMessage.success(`无线连接已建立（${r.serial}），现在可以拔掉 USB 线`)
    await startMirrorWithSerial(r.serial)
  } catch (e) {
    ElMessage.error({message: parseErrorMessage(e) || String(e), duration: 6000})
  } finally {
    wirelessStarting.value = false
  }
}

// 手动 IP 连接：先尝试直连 5555，失败引导走无线调试配对（Android 11+）
async function onConnectWireless() {
  const knownIp = status.deviceIp || ''
  let ip
  try {
    const r = await ElMessageBox.prompt(
      knownIp
        ? `已检测到同步连接的手机 IP：${knownIp}。确认无误后点确定即可`
        : '请输入手机的 WiFi IP 地址（手机：设置 → WLAN → 当前网络 可查看）',
      '无线投屏',
      {inputValue: knownIp, inputPattern: /^\d{1,3}(\.\d{1,3}){3}$/, inputErrorMessage: 'IP 地址格式不正确'},
    )
    ip = r.value.trim()
  } catch { return }

  wirelessConnecting.value = true
  try {
    let r
    try {
      r = await MirrorService.connectWireless({ip})
    } catch {
      // 直连失败 → 引导配对流程（无线调试端口随机，需配对后按显示端口连接）
      ElMessage.info({message: '直连失败，请改用「无线调试」配对：手机 → 开发者选项 → 无线调试 → 使用配对码配对设备', duration: 8000})
      const pr = await ElMessageBox.prompt(
        '输入手机无线调试弹窗中显示的「配对 IP 地址与端口」（如 192.168.0.5:37845）',
        '无线配对',
        {inputPattern: /^\d{1,3}(\.\d{1,3}){3}:\d+$/, inputErrorMessage: '格式应为 IP:端口'},
      )
      const pc = await ElMessageBox.prompt('输入 6 位配对码', '无线配对', {inputPattern: /^\d{6}$/, inputErrorMessage: '应为 6 位数字'})
      const cr = await ElMessageBox.prompt(
        '配对成功后，输入无线调试页面上显示的「IP 地址与端口」（无线调试主界面第一行）',
        '连接无线调试',
        {inputPattern: /^\d{1,3}(\.\d{1,3}){3}:\d+$/, inputErrorMessage: '格式应为 IP:端口'},
      )
      const [pairIp, pairPort] = pr.value.trim().split(':')
      const [cIp, cPort] = cr.value.trim().split(':')
      if (cIp !== ip) {
        ElMessage.warning('两次输入的 IP 不同，将以最后一次为准')
      }
      r = await MirrorService.connectWireless({
        ip: cIp,
        connectPort: Number(cPort),
        pairAddress: `${pairIp}:${pairPort}`,
        pairCode: pc.value.trim(),
      })
    }
    ElMessage.success(`无线连接成功（${r.serial}）`)
    await startMirrorWithSerial(r.serial)
  } catch (e) {
    ElMessage.error({message: parseErrorMessage(e) || String(e), duration: 6000})
  } finally {
    wirelessConnecting.value = false
  }
}

const DEFAULT_STATUS = {
    running: false,
    port: 45817,
    localIp: null,
    pcName: '',
    paired: false,
    pairingCode: null,
    pairingExpiresInSecs: 0,
    protocolVersion: '1',
    lastError: null,
    deviceIp: null,
}

const status = reactive({...DEFAULT_STATUS})
const enabled = ref(false)
const switching = ref(false)
let pollTimer = null

/** 手动连接地址（IP 由后端探测） */
const address = computed(() => {
    if (!status.running || !status.localIp) return ''
    return `http://${status.localIp}:${status.port}`
})

async function refreshStatus() {
    try {
        const snapshot = await SyncService.getStatus()
        Object.assign(status, snapshot)
        enabled.value = snapshot.running
    } catch {
        // 轮询失败不打扰用户（如服务刚停止的瞬间）
    }
}

async function onToggle(val) {
    switching.value = true
    try {
        if (val) {
            const snapshot = await SyncService.start()
            Object.assign(status, snapshot)
            enabled.value = snapshot.running
        } else {
            await SyncService.stop()
            Object.assign(status, {...DEFAULT_STATUS})
        }
        localStorage.setItem('fuyun-sync-enabled', val ? '1' : '0')
    } catch (e) {
        enabled.value = !val
        const key = val ? 'startFailed' : 'stopFailed'
        ElMessage.error(t(`settings.sync.${key}`, {error: parseErrorMessage(e)}))
    } finally {
        switching.value = false
    }
}

async function onRegenerate() {
    try {
        const code = await SyncService.regeneratePairingCode()
        status.pairingCode = code
        status.pairingExpiresInSecs = 300
    } catch (e) {
        ElMessage.error(t('settings.sync.regenerateFailed', {error: parseErrorMessage(e)}))
    }
}

async function copyAddress() {
    if (!address.value) return
    try {
        await navigator.clipboard.writeText(address.value)
        ElMessage.success(t('settings.sync.addressCopied'))
    } catch (e) {
        ElMessage.error(parseErrorMessage(e))
    }
}

async function onUnpair() {
    try {
        await ElMessageBox.confirm(
            t('settings.sync.unpairConfirm'),
            t('settings.sync.confirmTitle'),
            {type: 'warning', confirmButtonText: t('settings.sync.unpair')}
        )
        await SyncService.unpairDevice()
        status.paired = false
        await refreshStatus()
    } catch (e) {
        // 用户取消不报错
        if (e !== 'cancel' && e !== 'close') {
            ElMessage.error(t('settings.sync.unpairFailed', {error: parseErrorMessage(e)}))
        }
    }
}

onMounted(async () => {
    await refreshStatus()
    refreshDeviceHistory()
    loadQuality()
    // 记住用户选择：开启过同步则随设置页自动恢复
    if (localStorage.getItem('fuyun-sync-enabled') === '1' && !status.running) {
        await onToggle(true)
    }
    // 轮询刷新配对码倒计时
    pollTimer = setInterval(refreshStatus, 2000)
})

onUnmounted(() => {
    if (pollTimer) {
        clearInterval(pollTimer)
        pollTimer = null
    }
})
</script>

<style scoped>
.sync-section h3 {
  margin-top: 0;
}

.sync-toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 16px;
}

.sync-toggle-text p {
  margin: 0;
  color: var(--el-text-color-secondary);
}

.sync-status-card {
  margin-bottom: 16px;
}

.pairing-block {
  text-align: center;
  padding: 20px;
  border: 1px dashed var(--el-border-color);
  border-radius: 8px;
  margin-bottom: 16px;
}

.pairing-code {
  font-size: 40px;
  font-weight: 700;
  letter-spacing: 8px;
  font-family: 'Courier New', monospace;
  color: var(--el-color-primary);
}

.pairing-meta {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  margin-top: 8px;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}

.hint {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  margin: 8px 0 0;
}

.address-row {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
  font-size: 13px;
}

.address-row code {
  background: var(--el-fill-color-light);
  padding: 2px 8px;
  border-radius: 4px;
}

.paired-row {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
}

.how-to {
  border-top: 1px solid var(--el-border-color-lighter);
  padding-top: 12px;
}

.how-to h4 {
  margin: 0 0 8px;
}

.how-to p {
  margin: 4px 0;
  color: var(--el-text-color-regular);
  font-size: 13px;
}

.mirror-entry {
  margin-top: 16px;
  padding-top: 16px;
  border-top: 1px dashed var(--el-border-color);
}

.mirror-entry h4 {
  margin: 0 0 8px;
}

.mirror-entry .hint {
  margin: 0 0 12px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  line-height: 1.6;
}

/* 历史设备列表 */
.history-title {
  margin: 14px 0 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--el-text-color-secondary);
  display: flex;
  align-items: center;
  gap: 8px;
}

.history-refresh {
  font-weight: 400;
}

/* 投屏模式卡片 */
.mirror-guide {
  margin: 10px 0;
}

.mirror-guide p {
  margin: 4px 0;
  font-size: 13px;
  color: var(--el-text-color-regular);
}

.mirror-modes {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}

.mode-card {
  flex: 1 1 240px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 10px;
  padding: 14px 16px;
  background: var(--fy-bg-card, transparent);
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.mode-card .mode-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--el-text-color-primary);
  display: flex;
  align-items: center;
  gap: 8px;
}

.mode-card .mode-desc {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  line-height: 1.6;
  flex: 1;
}

.mode-card .mode-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

/* 画质模式块 */
.quality-block {
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 10px;
  padding: 14px 16px;
  background: var(--fy-bg-card, transparent);
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 12px;
}

.quality-block .mode-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.quality-block .mode-desc {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  line-height: 1.6;
}

.quality-row {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.quality-hint {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.mode-card .el-button {
  align-self: flex-start;
}

.history-device {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  margin-bottom: 6px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  background: var(--fy-bg-card, transparent);
}

.history-device .hd-name {
  font-weight: 600;
  color: var(--el-text-color-primary);
}

/* 最近使用时间 */
.history-device .hd-time {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

/* 连接方式徽标 */
.history-device .hd-conn {
  font-size: 11px;
  padding: 0 6px;
  border-radius: 3px;
  line-height: 18px;
  flex-shrink: 0;
}

.history-device .hd-conn.is-usb {
  color: var(--el-color-primary);
  border: 1px solid var(--el-color-primary-light-5, var(--el-color-primary));
  background: var(--el-color-primary-light-9, transparent);
}

.history-device .hd-conn.is-wifi {
  color: var(--el-color-success);
  border: 1px solid var(--el-color-success-light-5, var(--el-color-success));
  background: var(--el-color-success-light-9, transparent);
}

.history-device .hd-online {
  font-size: 11px;
  color: var(--el-color-success);
  border: 1px solid var(--el-color-success);
  border-radius: 10px;
  padding: 0 8px;
  line-height: 18px;
}

.history-device .el-button {
  margin-left: auto;
}
</style>
