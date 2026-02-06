<template>
  <el-config-provider :locale="zhCn">
    <div :class="{ dark: isDark }" class="settings-container">
      <div class="header">
        <el-radio-group v-model="activeTab" size="large">
          <el-radio-button label="clipboard">
            <el-icon>
              <DocumentCopy/>
            </el-icon>
            剪切板设置
          </el-radio-button>
          <el-radio-button label="ai">
            <el-icon>
              <Cpu/>
            </el-icon>
            AI设置
          </el-radio-button>
          <el-radio-button label="about">
            <el-icon>
              <InfoFilled/>
            </el-icon>
            关于
          </el-radio-button>
        </el-radio-group>
        <el-button @click="toggleTheme">
          <template #icon>
            <component :is="isDark ? Sunny : Moon"/>
          </template>
          {{ isDark ? '白天' : '黑夜' }}
        </el-button>
      </div>

      <div class="content">
        <!-- Clipboard Settings -->
        <div v-show="activeTab === 'clipboard'">
          <el-form :model="form" label-position="top">
            <el-form-item label="最大历史记录数">
              <el-input-number v-model="form.maxItems" :max="1000" :min="1"/>
              <div class="form-hint">设置剪贴板历史记录的最大保存数量 (1-1000)</div>
            </el-form-item>

            <el-form-item label="打开剪切板窗口快捷键">
              <el-input
                  v-model="form.toggleShortcut"
                  :class="{ recording: isRecording }"
                  placeholder="例如: Ctrl+Shift+K"
                  readonly
              >
                <template #append>
                  <el-button :type="isRecording ? 'danger' : 'primary'" @click="toggleRecording">
                    <el-icon>
                      <component :is="isRecording ? VideoPause : Edit"/>
                    </el-icon>
                  </el-button>
                </template>
              </el-input>
              <div class="form-hint">点击编辑按钮来自定义打开剪切板窗口的快捷键</div>
            </el-form-item>
          </el-form>
        </div>

        <!-- AI Settings -->
        <div v-show="activeTab === 'ai'">
          <el-form :model="form" label-position="top">
            <el-form-item label="AI服务提供商">
              <el-select v-model="form.aiProvider" placeholder="请选择提供商" @change="handleProviderChange">
                <el-option
                    v-for="provider in providers"
                    :key="provider.value"
                    :label="provider.label"
                    :value="provider.value"
                />
                <el-option label="自定义" value="custom"/>
              </el-select>
            </el-form-item>

            <el-form-item v-if="form.aiProvider === 'custom'" label="自定义提供商名称">
              <el-input v-model="form.customProviderName" placeholder="请输入自定义提供商名称，如：OpenAI"/>
            </el-form-item>

            <el-form-item label="AI服务地址">
              <el-input v-model="form.apiUrl" placeholder="例如: https://api.openai.com/v1">
                <template #append>
                  <el-button :loading="testingConnection" @click="testConnection">
                    <el-icon>
                      <Connection/>
                    </el-icon>
                  </el-button>
                </template>
              </el-input>
            </el-form-item>

            <el-form-item label="AI模型名称">
              <el-input v-model="form.modelName" placeholder="例如: gpt-3.5-turbo"/>
            </el-form-item>

            <el-form-item label="API密钥">
              <el-input
                  v-model="form.apiKey"
                  placeholder="请输入您的API密钥"
                  show-password
                  type="password"
              />
            </el-form-item>
          </el-form>
        </div>

        <!-- About Settings -->
        <div v-show="activeTab === 'about'">
          <div class="about-section">
            <h3>
              <el-icon>
                <Refresh/>
              </el-icon>
              检查更新
            </h3>
            <p>当前版本：<strong>{{ currentVersion }}</strong></p>
            <el-button :loading="checkingUpdate" type="warning" @click="checkUpdate">
              检查更新
            </el-button>
            <div v-if="updateStatus" :class="updateStatus.type" class="update-status">
              {{ updateStatus.message }}
            </div>
            <div v-if="showUpdateProgress" class="update-progress">
              <el-progress :percentage="updateProgress" :status="updateProgress === 100 ? 'success' : ''"/>
              <div class="progress-text">正在更新... {{ updateProgress }}%</div>
            </div>
          </div>

          <div class="about-section">
            <h3>
              <el-icon>
                <Star/>
              </el-icon>
              软件功能
            </h3>
            <ul class="feature-list">
              <li>
                <el-icon>
                  <CircleCheck/>
                </el-icon>
                <strong>剪贴板管理</strong> - 自动记录剪贴板历史，支持快速选择和粘贴
              </li>
              <li>
                <el-icon>
                  <Pointer/>
                </el-icon>
                <strong>划词翻译</strong> - 选中文本后自动显示翻译和解释选项
              </li>
              <li>
                <el-icon>
                  <Cpu/>
                </el-icon>
                <strong>AI集成</strong> - 支持OpenAI等AI服务，提供智能翻译和解释
              </li>
              <li>
                <el-icon>
                  <Lightning/>
                </el-icon>
                <strong>快捷键操作</strong> - 支持自定义快捷键，提高工作效率
              </li>
              <li>
                <el-icon>
                  <Timer/>
                </el-icon>
                <strong>历史记录</strong> - 保存剪贴板历史，方便重复使用
              </li>
            </ul>
          </div>

          <div class="about-section">
            <h3>
              <el-icon>
                <Reading/>
              </el-icon>
              使用方法
            </h3>
            <ol class="usage-list">
              <li><strong>剪贴板使用</strong>：按 <code>{{ form.toggleShortcut || 'Ctrl+Shift+K' }}</code> 显示剪贴板历史窗口
              </li>
              <li><strong>划词功能</strong>：选中文本后显示工具栏</li>
              <li><strong>AI设置</strong>：在AI设置页面配置API密钥和服务地址</li>
              <li><strong>更新检查</strong>：点击按钮检查软件更新</li>
              <li><strong>系统托盘</strong>：右键系统托盘图标访问更多功能</li>
            </ol>
          </div>
        </div>
      </div>

      <div v-if="activeTab !== 'about'" class="footer">
        <el-button size="large" type="success" @click="saveSettings">
          <el-icon><Select/></el-icon>
          保存设置
        </el-button>
      </div>

      <div class="footer-links">
        <p>
          需要帮助？
          <el-link type="primary" @click="openExternal('https://github.com/zRq1351/fuyun_tools')">查看文档</el-link>
          |
          <el-link type="primary" @click="openExternal('https://github.com/zRq1351/fuyun_tools/issues')">报告问题
          </el-link>
        </p>
        <p>版本 {{ currentVersion }} | © {{ new Date().getFullYear() }} fuyun_tools</p>
      </div>
    </div>
  </el-config-provider>
</template>

<script setup>
import {onMounted, reactive, ref} from 'vue'
import {ElMessage, ElMessageBox} from 'element-plus'
import zhCn from 'element-plus/dist/locale/zh-cn'
import {
  CircleCheck,
  Connection,
  Cpu,
  DocumentCopy,
  Edit,
  InfoFilled,
  Lightning,
  Moon,
  Pointer,
  Reading,
  Refresh,
  Select,
  Star,
  Sunny,
  Timer,
  VideoPause
} from '@element-plus/icons-vue'
import {invoke} from '@tauri-apps/api/core'
import {openUrl} from '@tauri-apps/plugin-opener'
import {check} from '@tauri-apps/plugin-updater'
import {relaunch} from '@tauri-apps/plugin-process'

const activeTab = ref('clipboard')
const isDark = ref(false)
const currentVersion = ref('0.0.0')
const providers = ref([])
const testingConnection = ref(false)
const checkingUpdate = ref(false)
const updateStatus = ref(null)
const updateProgress = ref(0)
const showUpdateProgress = ref(false)

const form = reactive({
  maxItems: 100,
  toggleShortcut: '',
  aiProvider: '',
  apiUrl: '',
  modelName: '',
  apiKey: '',
  customProviderName: ''
})

const isRecording = ref(false)
const recordedShortcut = ref('')

const toggleTheme = () => {
  isDark.value = !isDark.value
  const html = document.documentElement
  if (isDark.value) {
    html.classList.add('dark')
    localStorage.setItem('settings-theme', 'dark')
  } else {
    html.classList.remove('dark')
    localStorage.setItem('settings-theme', 'light')
  }
}

const loadAiProviders = async () => {
  try {
    const result = await invoke('get_all_configured_providers')
    if (Array.isArray(result)) {
      providers.value = result.map(([value, label]) => ({value, label}))
    }
  } catch (error) {
    ElMessage.error('加载AI提供商失败: ' + error)
  }
}

const handleProviderChange = async (provider) => {
  if (!provider) return
  if (provider === 'custom') {
    form.apiUrl = ''
    form.modelName = ''
    form.apiKey = ''
    return
  }

  try {
    const settings = await invoke('get_ai_settings')
    const providerConfigs = settings.provider_configs || {}

    if (providerConfigs[provider]) {
      const config = providerConfigs[provider]
      form.apiUrl = config.api_url || ''
      form.modelName = config.model_name || ''
      form.apiKey = config.api_key || ''
    } else {
      const configResult = await invoke('get_provider_config', {provider})
      if (Array.isArray(configResult) && configResult.length >= 2) {
        const [url, model] = configResult
        form.apiUrl = url || ''
        form.modelName = model || ''
        form.apiKey = ''
      }
    }
  } catch (error) {
    ElMessage.error('加载提供商配置失败: ' + error)
  }
}

const toggleRecording = () => {
  if (isRecording.value) {
    stopRecording()
  } else {
    startRecording()
  }
}

const startRecording = () => {
  isRecording.value = true
  recordedShortcut.value = ''
  form.toggleShortcut = '请按下快捷键...'
  document.addEventListener('keydown', handleKeyDown, true)
  ElMessage.info('开始录制快捷键，请按下组合键')
}

const stopRecording = () => {
  isRecording.value = false
  document.removeEventListener('keydown', handleKeyDown, true)
  if (recordedShortcut.value) {
    form.toggleShortcut = recordedShortcut.value
  } else {
  }
}

const handleKeyDown = (event) => {
  if (!isRecording.value) return
  event.preventDefault()
  event.stopPropagation()
  if (event.repeat) return

  const modifiers = []
  if (event.ctrlKey) modifiers.push('Ctrl')
  if (event.altKey) modifiers.push('Alt')
  if (event.shiftKey) modifiers.push('Shift')

  let key = ''
  if (event.key.length === 1 && /[a-zA-Z0-9]/.test(event.key)) {
    key = event.key.toUpperCase()
  } else {
    const k = event.key.toLowerCase()
    const keyMap = {
      ' ': 'Space',
      'spacebar': 'Space',
      'enter': 'Enter',
      'tab': 'Tab',
      'backspace': 'Backspace',
      'delete': 'Delete',
      'escape': 'Escape',
      'esc': 'Escape',
      'arrowup': 'Up', 'up': 'Up',
      'arrowdown': 'Down', 'down': 'Down',
      'arrowleft': 'Left', 'left': 'Left',
      'arrowright': 'Right', 'right': 'Right'
    }
    if (keyMap[k]) {
      key = keyMap[k]
    } else if (k.startsWith('f') && k.length <= 3) {
      key = k.toUpperCase()
    }
  }

  if (modifiers.length > 0 && key) {
    recordedShortcut.value = [...modifiers, key].join('+')
    form.toggleShortcut = recordedShortcut.value
    stopRecording()
    ElMessage.success(`已录制快捷键: ${recordedShortcut.value}`)
  }
}

const checkUpdate = async () => {
  checkingUpdate.value = true
  updateStatus.value = {message: '正在检查更新...', type: 'info'}
  showUpdateProgress.value = false
  updateProgress.value = 0

  try {
    const update = await check()
    if (update) {
      updateStatus.value = null

      try {
        await ElMessageBox.confirm(
            `发现新版本 ${update.version}，是否立即更新？\n\n更新内容:\n${update.body || '暂无更新说明'}`,
            '发现更新',
            {
              confirmButtonText: '立即更新',
              cancelButtonText: '稍后提醒',
              type: 'info',
            }
        )

        showUpdateProgress.value = true
        updateStatus.value = {message: '正在下载更新...', type: 'info'}

        let contentLength = 0
        let downloaded = 0

        await update.downloadAndInstall((event) => {
          if (event.event === 'Started') {
            contentLength = event.data.contentLength || 0
            downloaded = 0
            updateProgress.value = 0
          } else if (event.event === 'Progress') {
            downloaded += event.data.chunkLength
            if (contentLength > 0) {
              updateProgress.value = Math.round((downloaded / contentLength) * 100)
            }
          } else if (event.event === 'Finished') {
            updateProgress.value = 100
          }
        })

        updateStatus.value = {message: '更新下载完成', type: 'success'}

        await ElMessageBox.confirm(
            '更新已下载完成，是否立即重启应用以应用更新？',
            '更新完成',
            {
              confirmButtonText: '立即重启',
              cancelButtonText: '稍后重启',
              type: 'success',
            }
        )

        await relaunch()

      } catch (action) {
        if (action === 'cancel') {
          updateStatus.value = {message: '已取消更新', type: 'info'}
        }
      }
    } else {
      updateStatus.value = {message: '已是最新版本', type: 'success'}
    }
  } catch (error) {
    if (error !== 'cancel') {
      updateStatus.value = {message: '网络连接失败，请检查您的网络设置后重试', type: 'error'}
    }
  } finally {
    checkingUpdate.value = false
  }
}

const testConnection = async () => {
  if (!form.apiUrl || !form.modelName || !form.apiKey) {
    ElMessage.warning('请填写完整信息后再测试')
    return
  }
  testingConnection.value = true
  try {
    const result = await invoke('test_ai_connection', {
      aiApiUrl: form.apiUrl,
      aiModelName: form.modelName,
      aiApiKey: form.apiKey
    })
    ElMessage.success(result)
  } catch (error) {
    ElMessage.error(`连接失败: ${error}`)
  } finally {
    testingConnection.value = false
  }
}


const saveSettings = async () => {
  let selectedProvider = form.aiProvider
  if (selectedProvider === 'custom') {
    if (!form.customProviderName) {
      ElMessage.warning('请输入自定义提供商名称')
      return
    }
    selectedProvider = form.customProviderName
  }

  try {
    await invoke('save_app_settings', {
      maxItems: form.maxItems,
      aiProvider: selectedProvider,
      aiApiUrl: form.apiUrl,
      aiModelName: form.modelName,
      aiApiKey: form.apiKey,
      hotKey: form.toggleShortcut
    })

    if (form.aiProvider === 'custom') {
      ElMessage.success(`自定义提供商 '${selectedProvider}' 添加成功`)
      await loadAiProviders()
      form.aiProvider = selectedProvider
    } else {
      ElMessage.success('保存成功')
    }
  } catch (error) {
    ElMessage.error(`保存失败: ${error}`)
  }
}

const openExternal = async (url) => {
  try {
    await openUrl(url)
  } catch (err) {
    ElMessage.error(err)
  }
}

onMounted(async () => {
  const savedTheme = localStorage.getItem('settings-theme')
  const prefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches
  if (savedTheme === 'dark' || (!savedTheme && prefersDark)) {
    isDark.value = true
    document.documentElement.classList.add('dark')
  }

  try {
    await loadAiProviders()
    const settings = await invoke('get_ai_settings')

    form.maxItems = settings.max_items || 50
    currentVersion.value = settings.version || '0.3.1'
    form.aiProvider = settings.ai_provider || ''
    form.toggleShortcut = settings.hot_key || ''

    const currentProvider = settings.ai_provider
    if (currentProvider && settings.provider_configs && settings.provider_configs[currentProvider]) {
      const config = settings.provider_configs[currentProvider]
      form.apiUrl = config.api_url || ''
      form.modelName = config.model_name || ''
      form.apiKey = config.api_key || ''
    }
  } catch (error) {
    ElMessage.error(`加载设置失败: ${error}`)
  }
})
</script>

<style>
body {
  margin: 0;
  font-family: 'Helvetica Neue', Helvetica, 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', '微软雅黑', Arial, sans-serif;
}

.settings-container {
  padding: 20px;
  max-width: 800px;
  margin: 0 auto;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.content {
  background: #fff;
  padding: 20px;
  border-radius: 8px;
  box-shadow: 0 2px 12px 0 rgba(0, 0, 0, 0.1);
}

.dark .content {
  background: #1d1e1f;
  box-shadow: 0 2px 12px 0 rgba(0, 0, 0, 0.3);
}

.form-hint {
  font-size: 12px;
  color: #909399;
  margin-top: 4px;
}

.footer {
  margin-top: 20px;
  text-align: right;
}

.footer-links {
  margin-top: 40px;
  text-align: center;
  color: #909399;
  font-size: 14px;
}

.recording :deep(.el-input__inner) {
  color: #f56c6c !important;
}

.feature-list, .usage-list {
  padding-left: 20px;
  line-height: 1.8;
}

.feature-list li, .usage-list li {
  margin-bottom: 8px;
}

.update-status {
  margin-top: 10px;
  padding: 10px;
  border-radius: 4px;
}

.update-status.success {
  background-color: #f0f9eb;
  color: #67c23a;
}

.update-status.error {
  background-color: #fef0f0;
  color: #f56c6c;
}

.update-status.info {
  background-color: #f4f4f5;
  color: #909399;
}
</style>
