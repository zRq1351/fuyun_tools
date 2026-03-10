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
        <div v-show="activeTab === 'clipboard'">
          <ClipboardSettings :form="form"/>
        </div>

        <div v-show="activeTab === 'ai'">
          <AISettings ref="aiSettingsRef" :form="form"/>
        </div>

        <div v-show="activeTab === 'about'">
          <AboutSettings
              :current-version="currentVersion"
              :image-toggle-shortcut="form.imageToggleShortcut"
              :toggle-shortcut="form.toggleShortcut"
          />
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
import {ElMessage} from 'element-plus'
import zhCn from 'element-plus/dist/locale/zh-cn'
import {Cpu, DocumentCopy, InfoFilled, Moon, Select, Sunny} from '@element-plus/icons-vue'
import {openUrl} from '@tauri-apps/plugin-opener'
import {AISettingsService} from '../../services/ipc'
import ClipboardSettings from './components/ClipboardSettings.vue'
import AISettings from './components/AISettings.vue'
import AboutSettings from './components/AboutSettings.vue'

const activeTab = ref('clipboard')
const isDark = ref(false)
const currentVersion = ref('0.0.0')
const aiSettingsRef = ref(null)

const form = reactive({
  maxItems: 100,
  groupedItemsProtectedFromLimit: true,
  toggleShortcut: '',
  imageToggleShortcut: '',
  aiProvider: '',
  apiUrl: '',
  modelName: '',
  apiKey: '',
  customProviderName: '',
  selectionEnabled: true
})

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
    await AISettingsService.saveSettings({
      maxItems: form.maxItems,
      aiProvider: selectedProvider,
      aiApiUrl: form.apiUrl,
      aiModelName: form.modelName,
      aiApiKey: form.apiKey,
      hotKey: form.toggleShortcut,
      imageHotKey: form.imageToggleShortcut,
      selectionEnabled: form.selectionEnabled,
      groupedItemsProtectedFromLimit: form.groupedItemsProtectedFromLimit
    })

    if (form.aiProvider === 'custom') {
      ElMessage.success(`自定义提供商 '${selectedProvider}' 添加成功`)
      if (aiSettingsRef.value) {
        await aiSettingsRef.value.loadAiProviders()
      }
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
    const settings = await AISettingsService.getSettings()

    form.maxItems = settings.max_items || 50
    currentVersion.value = settings.version || '0.3.1'
    form.toggleShortcut = settings.hot_key || ''
    form.imageToggleShortcut = settings.image_hot_key || ''
    form.selectionEnabled = settings.selection_enabled !== false
    form.groupedItemsProtectedFromLimit = settings.grouped_items_protected_from_limit !== false

    if (aiSettingsRef.value) {
      aiSettingsRef.value.applyCurrentProviderConfig(settings)
    } else {
      const currentProvider = settings.ai_provider || ''
      form.aiProvider = currentProvider
      const providerConfigs = settings.provider_configs || {}
      if (currentProvider && providerConfigs[currentProvider]) {
        const config = providerConfigs[currentProvider]
        form.apiUrl = config.api_url || ''
        form.modelName = config.model_name || ''
        form.apiKey = config.api_key || ''
      }
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
</style>
