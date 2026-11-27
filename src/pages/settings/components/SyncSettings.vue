<template>
  <div class="sync-section">
    <!-- 手机功能总开关：与其它功能区保持一致的启用/停用开关 -->
    <div class="module-toggle">
      <div class="module-toggle-text">
        <h4>{{ $t('settings.sync.moduleTitle') }}</h4>
        <p>{{ $t('settings.sync.moduleHint') }}</p>
      </div>
      <el-switch
        :model-value="form.phoneEnabled"
        :loading="!!pendingToggles.phoneEnabled"
        :active-text="pendingToggles.phoneEnabled === 'enabling' ? $t('common.enabling') : $t('common.enable')"
        :inactive-text="pendingToggles.phoneEnabled === 'disabling' ? $t('common.disabling') : $t('common.disable')"
        @update:model-value="(val) => toggleFeature('phoneEnabled', val)"
      />
    </div>

    <div
      v-if="!form.phoneEnabled"
      class="module-off"
    >
      {{ $t('settings.sync.moduleDisabledHint') }}
    </div>

    <template v-else>
      <!-- 连接状态总览（一行看清两条通道） -->
      <div class="link-status">
        <div class="ls-item">
          <span class="ls-label">手机 App</span>
          <el-tag
            size="small"
            :type="status.running ? (status.paired ? 'success' : 'primary') : 'info'"
          >
            {{ status.running ? (status.paired ? '已配对' : '待配对') : '服务已停止' }}
          </el-tag>
        </div>
        <div class="ls-item">
          <span class="ls-label">ADB 设备</span>
          <el-tag
            size="small"
            :type="adbStatus.connected ? 'success' : 'info'"
          >
            {{ adbStatus.connected ? adbStatus.serial : '未连接' }}
          </el-tag>
        </div>
        <el-button
          size="small"
          text
          :loading="linkRefreshing"
          @click="refreshLinkStatus"
        >
          刷新
        </el-button>
      </div>

      <!-- ===== 剪贴板同步 ===== -->
      <div class="section-card">
        <div class="section-head">
          <div class="section-title">
            剪贴板同步
          </div>
          <div class="section-head-actions">
            <el-button
              size="small"
              text
              @click="syncHelpOpen = true"
            >
              使用帮助
            </el-button>
            <el-switch
              v-model="enabled"
              :loading="switching"
              @change="onToggle"
            />
          </div>
        </div>

        <!-- 已配对设备（手机型号 / IP / 在线状态 / 最近活跃） -->
        <div
          v-if="status.paired"
          class="device-row"
        >
          <div class="device-avatar">
            <svg viewBox="0 0 24 24"><rect
              x="7"
              y="2.5"
              width="10"
              height="19"
              rx="2.5"
            /><path d="M10.5 19h3" /></svg>
          </div>
          <div class="device-info">
            <div class="device-name">
              {{ status.deviceName || '已配对的手机' }}
              <span
                class="device-dot"
                :class="{ online: status.deviceOnline }"
              />
              <span class="device-state">{{ status.deviceOnline ? '在线' : '离线' }}</span>
            </div>
            <div class="device-meta">
              {{ status.deviceIp || '地址未知' }}
              <template v-if="status.lastSeenMs">
                · 最近活跃 {{ lastSeenLabel(status.lastSeenMs) }}
              </template>
            </div>
            <!-- 扫码授权后可分别启用：剪贴板同步 / 投屏 -->
            <div class="cap-row">
              <el-tag
                size="small"
                type="success"
                effect="plain"
              >
                剪贴板同步 · 已授权
              </el-tag>
              <el-tag
                size="small"
                :type="status.mirrorAuthorized ? 'success' : 'info'"
                effect="plain"
              >
                投屏 · {{ status.mirrorAuthorized ? '已授权' : '未授权' }}
              </el-tag>
            </div>
          </div>
          <el-button
            size="small"
            type="danger"
            plain
            @click="onUnpair"
          >
            吊销设备
          </el-button>
        </div>
        <div
          v-else
          class="device-empty"
        >
          尚未配对 — 手机打开「浮云剪贴板」扫描下方二维码即可自动完成配对
        </div>

        <!-- 配对二维码 -->
        <template v-if="status.running">
          <div
            v-if="!status.paired || showPairBox"
            class="pair-box"
          >
            <img
              v-if="pairQr"
              class="pairing-qr"
              :src="pairQr"
              alt="配对二维码"
            >
            <div class="pair-side">
              <div class="pair-code">
                {{ status.pairingCode || '------' }}
              </div>
              <div class="pair-meta">
                <span v-if="status.pairingCode && status.pairingExpiresInSecs > 0">
                  {{ $t('settings.sync.expiresIn', {secs: status.pairingExpiresInSecs}) }}
                </span>
                <span v-else>{{ $t('settings.sync.codeExpired') }}</span>
                <el-button
                  size="small"
                  text
                  @click="onRegenerate"
                >
                  换一个
                </el-button>
              </div>
              <div class="pair-hint">
                手机扫码即自动完成配对
              </div>
              <div
                v-if="address"
                class="pair-address"
              >
                <code>{{ address }}</code>
                <el-button
                  size="small"
                  text
                  @click="copyAddress"
                >
                  {{ $t('settings.sync.copyAddress') }}
                </el-button>
              </div>
            </div>
          </div>
          <div
            v-else
            class="device-empty pair-collapsed"
          >
            要连接新手机？
            <el-button
              size="small"
              text
              @click="showPairBox = true"
            >
              显示配对二维码
            </el-button>
          </div>
        </template>
        <div
          v-else
          class="device-empty"
        >
          打开上方开关启动同步服务后显示配对二维码
        </div>
      </div>

      <!-- ===== 手机投屏 ===== -->
      <div class="section-card">
        <div class="section-head">
          <div class="section-title">
            手机投屏
          </div>
          <div class="section-head-actions">
            <el-button
              size="small"
              text
              @click="mirrorHelpOpen = true"
            >
              使用帮助
            </el-button>
          </div>
        </div>

        <div
          v-if="mirrorLocked"
          class="auth-hint"
        >
          手机尚未授权投屏 — 请在手机 App「设备能力」中打开「投屏」开关（首次需先扫码授权）
        </div>

        <div class="mode-grid">
          <div class="mode-item">
            <div class="mode-title">
              USB 投屏
              <el-tag
                size="small"
                type="success"
              >
                推荐
              </el-tag>
            </div>
            <div class="mode-desc">
              数据线连接，延迟最低
            </div>
            <el-button
              class="mode-btn"
              type="primary"
              :disabled="mirrorLocked"
              :loading="mirrorStarting"
              @click="onStartMirror"
            >
              开启 USB 投屏
            </el-button>
          </div>
          <div class="mode-item">
            <div class="mode-title">
              WiFi 投屏
            </div>
            <div class="mode-desc">
              同一 Wi-Fi 下无线投屏
            </div>
            <div class="mode-btn-row">
              <el-button
                class="mode-btn"
                type="primary"
                plain
                :disabled="mirrorLocked"
                :loading="wirelessStarting"
                @click="onEnableWireless"
              >
                一键切换无线
              </el-button>
              <el-button
                class="mode-btn"
                :disabled="mirrorLocked"
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
          <div class="quality-label">
            画质模式
          </div>
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

        <!-- 当前投屏会话（停止投屏 / 注销） -->
        <div class="session-row">
          <div class="session-info">
            <span class="session-label">当前投屏</span>
            <el-tag
              v-if="mirrorSession.running"
              size="small"
              type="success"
            >
              {{ mirrorSession.deviceName || '设备' }} · 运行中
            </el-tag>
            <el-tag
              v-else
              size="small"
              type="info"
            >
              未投屏
            </el-tag>
          </div>
          <el-button
            size="small"
            type="danger"
            plain
            :disabled="!mirrorSession.running"
            :loading="mirrorStopping"
            @click="onStopMirror"
          >
            停止投屏
          </el-button>
        </div>

        <!-- 历史设备：多台手机快速切换 -->
        <template v-if="deviceHistory.length">
          <div class="history-title">
            历史设备
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

      <!-- 使用帮助弹窗（把长文字说明从页面移出） -->
      <el-dialog
        v-model="syncHelpOpen"
        title="剪贴板同步 · 使用说明"
        width="520px"
        append-to-body
      >
        <ol class="help-list">
          <li>电脑端打开「剪贴板同步」开关，页面会显示 6 位配对码</li>
          <li>手机安装「浮云剪贴板」，与电脑连同一个 Wi-Fi</li>
          <li>手机打开 App 自动发现电脑（找不到可手动输入地址）</li>
          <li>扫描电脑上的二维码即自动配对完成；也可手动输入 6 位配对码</li>
          <li>电脑复制任意文本/图片 → 手机自动写入剪贴板；手机 App 内可发送文本/图片到电脑</li>
        </ol>
        <p class="help-note">
          为什么手机不能自动读剪贴板？Android 10+ 仅允许前台应用或默认输入法读取剪贴板，
          因此采用 App 内主动发送的合法方案。
        </p>
      </el-dialog>

      <el-dialog
        v-model="mirrorHelpOpen"
        title="手机投屏 · 使用说明"
        width="520px"
        append-to-body
      >
        <ol class="help-list">
          <li>手机「设置 → 关于手机」连续点击「版本号」7 次，开启开发者模式</li>
          <li>进入「设置 → 系统 → 开发者选项」</li>
          <li>USB 投屏：打开「USB 调试」，插上数据线，在手机弹窗点「允许」</li>
          <li>WiFi 投屏（Android 11+）：打开「无线调试」；或先插线用「一键切换无线」</li>
          <li>独立投屏（Android 10+）：在投屏窗口工具栏点「独立投屏」，把当前应用投到虚拟显示器，手机可继续做别的事</li>
          <li>中文输入：先在投屏画面上点击手机输入框，键盘弹出后再打字</li>
        </ol>
        <p class="help-note">
          投屏画面仅经 USB 或局域网传输，不经过任何服务器。
        </p>
      </el-dialog>
    </template>
  </div>
</template>

<script setup>
import {computed, onMounted, onUnmounted, reactive, ref, watch} from 'vue'
import QRCode from 'qrcode'
import {useI18n} from 'vue-i18n'
import {ElMessage, ElMessageBox} from 'element-plus'
import {SyncService, MirrorService} from '../../../services/ipc'
import {parseErrorMessage} from '../../../utils/errorHandler'

const {t} = useI18n()

const props = defineProps({
  form: {type: Object, required: true},
  onFeatureToggle: {type: Function, default: null},
})
// 总开关的启用/停用中间态（与其它功能设置页一致的交互反馈）
const pendingToggles = ref({})

const toggleFeature = async (fieldName, value) => {
  if (pendingToggles.value[fieldName]) return
  if (!props.onFeatureToggle) {
    props.form[fieldName] = value
    return
  }
  pendingToggles.value = {...pendingToggles.value, [fieldName]: value ? 'enabling' : 'disabling'}
  await props.onFeatureToggle(fieldName, value)
  pendingToggles.value = {...pendingToggles.value, [fieldName]: undefined}
  // 关闭后同步服务已被后端停止，刷新页面状态显示
  if (fieldName === 'phoneEnabled') {
    await refreshStatus()
    refreshLinkStatus()
  }
}

const mirrorStarting = ref(false)
const wirelessStarting = ref(false)
const wirelessConnecting = ref(false)
const deviceHistory = ref([])
const reconnecting = ref('')
const qualityMode = ref('balanced')
// 统一连接状态：ADB 设备（投屏通道）
const adbStatus = ref({connected: false, serial: ''})
const linkRefreshing = ref(false)
// 使用帮助弹窗 / 配对二维码折叠
const syncHelpOpen = ref(false)
const mirrorHelpOpen = ref(false)
const showPairBox = ref(false)
// 当前投屏会话（用于「停止投屏/注销」）
const mirrorSession = ref({running: false, deviceName: ''})
// 已配对但手机未打开「投屏」开关时，电脑端不可发起投屏（纯 adb 场景不受限）
const mirrorLocked = computed(() => status.running && status.paired && !status.mirrorAuthorized)
const mirrorStopping = ref(false)
let mirrorTimer = null

async function refreshMirrorSession() {
    try {
        const r = await MirrorService.getStatus()
        mirrorSession.value = {running: !!r.running, deviceName: r.deviceName || ''}
    } catch {
        mirrorSession.value = {running: false, deviceName: ''}
    }
}

async function onStopMirror() {
    mirrorStopping.value = true
    try {
        await MirrorService.stop()
        ElMessage.success('已停止投屏')
    } catch (e) {
        ElMessage.error({message: parseErrorMessage(e) || String(e), duration: 5000})
    } finally {
        mirrorStopping.value = false
        refreshMirrorSession()
        refreshLinkStatus()
    }
}

/** 最近活跃时间的友好展示 */
function lastSeenLabel(ms) {
    const diff = Date.now() - Number(ms || 0)
    if (!ms || diff < 0) return '—'
    if (diff < 60_000) return '刚刚'
    if (diff < 3600_000) return `${Math.floor(diff / 60_000)} 分钟前`
    const d = new Date(Number(ms))
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

async function refreshLinkStatus() {
  linkRefreshing.value = true
  try {
    const r = await MirrorService.adbStatus()
    adbStatus.value = {connected: !!r.connected, serial: r.serial || ''}
  } catch {
    adbStatus.value = {connected: false, serial: ''}
  } finally {
    linkRefreshing.value = false
  }
}

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
    refreshMirrorSession()
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
    refreshMirrorSession()
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

/**
 * 配对二维码：手机端「浮云」扫码后自动填入地址与配对码。
 * 内容格式 fuyun://pair?host=<ip:port>&code=<6位>，手机端用正则解析，兼容纯文本。
 */
const pairQr = ref('')

watch(
    [address, () => status.pairingCode],
    async () => {
        const hostPort = address.value.replace(/^https?:\/\//, '')
        if (!hostPort || !status.pairingCode) {
            pairQr.value = ''
            return
        }
        try {
            pairQr.value = await QRCode.toDataURL(
                `fuyun://pair?host=${hostPort}&code=${status.pairingCode}`,
                {margin: 1, width: 360, errorCorrectionLevel: 'M'},
            )
        } catch {
            pairQr.value = ''
        }
    },
    {immediate: true},
)

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
    refreshLinkStatus()
    loadQuality()
    refreshMirrorSession()
    // 记住用户选择：开启过同步则随设置页自动恢复
    if (localStorage.getItem('fuyun-sync-enabled') === '1' && !status.running) {
        await onToggle(true)
    }
    // 轮询刷新配对码倒计时
    pollTimer = setInterval(refreshStatus, 2000)
    // 投屏会话状态轮询（设置页被系统回收窗口时也能及时反映）
    mirrorTimer = setInterval(refreshMirrorSession, 5000)
})

onUnmounted(() => {
    if (pollTimer) {
        clearInterval(pollTimer)
        pollTimer = null
    }
    if (mirrorTimer) {
        clearInterval(mirrorTimer)
        mirrorTimer = null
    }
})
</script>

<style scoped>
.sync-section h3 {
  margin-top: 0;
}

/* 手机功能总开关卡片（与其它功能区一致的顶级开关） */
.module-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 16px;
  margin-bottom: 12px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 10px;
  background: var(--fy-bg-card, transparent);
}

.module-toggle-text h4 {
  margin: 0 0 6px;
  font-size: 14px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.module-toggle-text p {
  margin: 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--el-text-color-secondary);
}

.module-off {
  padding: 28px 16px;
  text-align: center;
  font-size: 13px;
  color: var(--el-text-color-secondary);
  border: 1px dashed var(--el-border-color);
  border-radius: 10px;
}

/* 分区小标题：剪贴板同步 / 手机投屏 */
.sub-title {
  margin: 4px 0 12px;
  padding-left: 8px;
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-primary);
  border-left: 3px solid var(--el-color-primary);
  line-height: 1.2;
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

.pairing-qr {
  display: block;
  width: 168px;
  height: 168px;
  margin: 0 auto 12px;
  padding: 6px;
  border-radius: 8px;
  /* 二维码需要纯白底才扫得动，固定白色不跟随深色主题 */
  background: #ffffff;
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

/* ===== 分区卡片（剪贴板同步 / 手机投屏） ===== */
.section-card {
  padding: 14px 16px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 10px;
  background: var(--fy-bg-card, transparent);
  margin-bottom: 12px;
}

.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}

.section-title {
  font-size: 14px;
  font-weight: 600;
  padding-left: 8px;
  border-left: 3px solid var(--el-color-primary);
  line-height: 1.2;
}

.section-head-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

/* 已配对设备 */
.device-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  border-radius: 10px;
  background: var(--el-fill-color-lighter, rgba(128, 128, 128, 0.06));
  margin-bottom: 12px;
}

.device-avatar {
  width: 36px;
  height: 36px;
  border-radius: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--el-color-primary-light-9, rgba(64, 158, 255, 0.1));
  flex: none;
}

.device-avatar svg {
  width: 20px;
  height: 20px;
  fill: none;
  stroke: var(--el-color-primary);
  stroke-width: 1.5;
}

.device-info {
  flex: 1;
  min-width: 0;
}

.device-name {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  font-weight: 600;
}

.device-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--el-text-color-placeholder);
  flex: none;
}

.device-dot.online {
  background: var(--el-color-success);
  box-shadow: 0 0 0 3px var(--el-color-success-light-9, rgba(103, 194, 58, 0.15));
}

.device-state {
  font-size: 12px;
  font-weight: 400;
  color: var(--el-text-color-secondary);
}

.cap-row {
  display: flex;
  gap: 6px;
  margin-top: 6px;
  flex-wrap: wrap;
}

.device-meta {
  margin-top: 3px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  font-family: 'SF Mono', 'Cascadia Code', Consolas, monospace;
}

.device-empty {
  padding: 14px;
  border: 1px dashed var(--el-border-color);
  border-radius: 10px;
  text-align: center;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  margin-bottom: 12px;
}

.pair-collapsed {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  margin-top: 4px;
}

/* 配对二维码区 */
.pair-box {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 12px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 10px;
}

.pair-side {
  flex: 1;
  min-width: 0;
}

.pair-code {
  font-size: 26px;
  font-weight: 600;
  letter-spacing: 6px;
  font-family: 'SF Mono', 'Cascadia Code', Consolas, monospace;
}

.pair-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 2px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.pair-hint {
  margin-top: 6px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.pair-address {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 6px;
  font-size: 12px;
}

.pair-address code {
  font-family: 'SF Mono', 'Cascadia Code', Consolas, monospace;
}

/* 投屏模式 */
.auth-hint {
  margin: 0 0 12px;
  padding: 10px 12px;
  font-size: 12px;
  line-height: 1.6;
  color: var(--el-color-warning-dark-2, #b88230);
  background: var(--el-color-warning-light-9, rgba(230, 162, 60, 0.1));
  border: 1px solid var(--el-color-warning-light-7, rgba(230, 162, 60, 0.25));
  border-radius: 8px;
}

.mode-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.mode-item {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 10px;
}

.mode-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
}

.mode-desc {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.mode-btn-row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.mode-btn {
  min-width: 110px;
  margin-left: 0;
}

/* 画质模式 */
.quality-block {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  margin-top: 14px;
}

.quality-label {
  font-size: 13px;
  font-weight: 600;
}

/* 当前投屏会话 */
.session-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px dashed var(--el-border-color-lighter);
}

.session-info {
  display: flex;
  align-items: center;
  gap: 8px;
}

.session-label {
  font-size: 13px;
  color: var(--el-text-color-secondary);
}

.history-title {
  margin: 16px 0 8px;
  font-size: 13px;
  font-weight: 600;
}

/* 帮助弹窗 */
.help-list {
  margin: 0;
  padding-left: 18px;
  font-size: 13px;
  line-height: 1.9;
}

.help-note {
  margin: 12px 0 0;
  font-size: 12px;
  line-height: 1.7;
  color: var(--el-text-color-secondary);
}
</style>
