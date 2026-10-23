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
  </div>
</template>

<script setup>
import {computed, onMounted, onUnmounted, reactive, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage, ElMessageBox} from 'element-plus'
import {SyncService} from '../../../services/ipc'
import {parseErrorMessage} from '../../../utils/errorHandler'

const {t} = useI18n()

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
    } catch (e) {
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
</style>
