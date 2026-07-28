<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.selection.title') }}</div>
      </template>
      <el-form-item :label="$t('settings.selection.enabled')">
        <el-switch
            :active-text="pendingToggles.selection === 'disabling' ? $t('common.disabling') : $t('common.enable')"
            :inactive-text="pendingToggles.selection === 'enabling' ? $t('common.enabling') : $t('common.close')"
            :loading="!!pendingToggles.selection"
            :model-value="form.selectionEnabled"
            @update:model-value="(val) => toggleFeature('selectionEnabled', val)"
        />
        <div class="form-hint">{{ $t('settings.selection.disabledHint') }}</div>
      </el-form-item>

      <el-form-item :label="$t('settings.selection.modifierKey')">
        <el-select v-model="form.selectionModifierKey" :placeholder="$t('settings.selection.selectModifier')">
          <el-option :label="$t('settings.selection.modifierNone')" value=""/>
          <el-option :label="$t('settings.selection.modifierCtrl')" value="Ctrl"/>
        </el-select>
        <div class="form-hint">{{ $t('settings.selection.modifierHint') }}</div>
      </el-form-item>

      <el-form-item :label="$t('settings.selection.translationTemplate')">
        <el-input
            v-model="form.translationPromptTemplate"
            :rows="4"
            :placeholder="$t('settings.selection.translationTemplatePlaceholder')"
            type="textarea"
        />
        <div class="form-actions">
          <el-button size="small" @click="resetTranslationPromptTemplate">{{ $t('common.reset') }}</el-button>
        </div>
        <div class="form-hint">{{ $t('settings.selection.translationTemplateHint') }}</div>
      </el-form-item>

      <el-form-item :label="$t('settings.selection.explanationTemplate')">
        <el-input
            v-model="form.explanationPromptTemplate"
            :rows="4"
            :placeholder="$t('settings.selection.explanationTemplatePlaceholder')"
            type="textarea"
        />
        <div class="form-actions">
          <el-button size="small" @click="resetExplanationPromptTemplate">{{ $t('common.reset') }}</el-button>
        </div>
        <div class="form-hint">{{ $t('settings.selection.explanationTemplateHint') }}</div>
      </el-form-item>
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.selection.webSearch') }}</div>
      </template>
      <el-form-item :label="$t('settings.selection.searchEngine')">
        <el-select v-model="form.selectionWebSearchEngine"
                   :placeholder="$t('settings.selection.searchEnginePlaceholder')">
          <el-option label="Bing" value="bing" />
          <el-option label="Google" value="google" />
          <el-option label="Baidu" value="baidu" />
          <el-option label="DuckDuckGo" value="duckduckgo" />
        </el-select>
        <div class="form-hint">{{ $t('settings.selection.searchEngineHint') }}</div>
      </el-form-item>
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.selection.customAIButtons') }}</div>
      </template>
      <div class="form-hint" style="margin-bottom: 12px;">{{ $t('settings.selection.customAIHint') }}</div>

      <div v-for="(item, index) in form.selectionCustomPrompts" :key="index"
           :class="{ disabled: !item.enabled }"
           class="custom-prompt-item">
        <div class="prompt-header">
          <el-input v-model="item.name" :disabled="!item.enabled"
                    :placeholder="$t('settings.selection.buttonNamePlaceholder')"
                    style="width: 200px;"/>
          <div class="prompt-style-controls">
            <el-popover :disabled="!item.enabled" :width="320" placement="bottom" trigger="click">
              <template #reference>
                <el-button :disabled="!item.enabled" size="small" style="width: 120px;">
                  <el-icon>
                    <component :is="getIconComponent(item.icon || 'Star')"/>
                  </el-icon>
                  <span style="margin-left: 4px;">{{ item.icon || 'Star' }}</span>
                </el-button>
              </template>
              <div class="icon-picker-grid">
                <div
                    v-for="iconName in availableIcons"
                    :key="iconName"
                    :class="{ selected: item.icon === iconName }"
                    class="icon-picker-item"
                    @click="item.icon = iconName"
                >
                  <el-icon :size="20">
                    <component :is="getIconComponent(iconName)"/>
                  </el-icon>
                </div>
              </div>
            </el-popover>
            <el-color-picker v-model="item.color" :disabled="!item.enabled" :predefine="colorPresets" show-alpha size="small"
                             :title="$t('settings.selection.textColor')"/>
            <el-color-picker v-model="item.bg_color" :disabled="!item.enabled" :predefine="bgColorPresets" show-alpha
                             :title="$t('settings.selection.bgColor')" size="small"/>

            <!-- 实时预览 -->
            <div class="preview-button-wrapper">
              <div :style="{ color: item.color || '#909399', background: parseBackground(item.bg_color), opacity: item.enabled ? 1 : 0.5 }"
                   class="preview-button">
                <el-icon class="btn-icon">
                  <component :is="getIconComponent(item.icon || 'Star')"/>
                </el-icon>
                <span class="btn-text">{{ item.name || $t('settings.selection.button') }}</span>
              </div>
            </div>
          </div>
          <div class="prompt-actions">
            <el-switch v-model="item.enabled" style="margin-right: 8px;"/>
            <el-button link type="danger" @click="removeCustomPrompt(index)">{{
                $t('settings.selection.deleteButton')
              }}
            </el-button>
          </div>
        </div>
        <el-input
            v-model="item.prompt"
            :rows="3"
            :placeholder="$t('settings.selection.promptTemplatePlaceholder')"
            type="textarea"
            style="margin-top: 8px;"
            :disabled="!item.enabled"
            @input="validatePrompt(item)"
        />
        <div v-if="item.prompt && !item.prompt.includes('{text}')" class="prompt-warning">
          <el-icon style="color: #E6A23C; margin-right: 4px;">
            <Warning/>
          </el-icon>
          <span>{{ $t('settings.selection.promptWarning') }}</span>
        </div>
      </div>

      <el-button plain style="margin-top: 12px;" type="primary" @click="addCustomPrompt">
        {{ $t('settings.selection.addCustomButton') }}
      </el-button>
    </el-card>
  </el-form>
</template>

<script setup>
import {ref} from 'vue'
import {useI18n} from 'vue-i18n'
import * as ElementPlusIconsVue from '@element-plus/icons-vue'
import {Warning} from '@element-plus/icons-vue'

const {t} = useI18n()

const props = defineProps({
  form: {
    type: Object,
    required: true
  },
  onFeatureToggle: {
    type: Function,
    default: null
  }
})

const pendingToggles = ref({})

const toggleFeature = async (fieldName, value) => {
  if (pendingToggles.value[fieldName]) return
  if (!props.onFeatureToggle) {
    props.form[fieldName] = value
    return
  }
  pendingToggles.value = {...pendingToggles.value, [fieldName]: value ? 'enabling' : 'disabling'}
  const ok = await props.onFeatureToggle(fieldName, value)
  pendingToggles.value = {...pendingToggles.value, [fieldName]: undefined}
}

// 动态获取图标组件（仅支持 Element Plus）
const getIconComponent = (iconName) => {
  if (!iconName) return ElementPlusIconsVue.Star

  const icon = ElementPlusIconsVue[iconName]
  if (icon) {
    return icon
  }

  // 默认返回 Star
  return ElementPlusIconsVue.Star
}

// 获取所有可用的 Element Plus 图标名称
const availableIcons = Object.keys(ElementPlusIconsVue).filter(name => {
  const excluded = ['Loading', 'Loading2']
  return !excluded.includes(name)
}).sort()

// 颜色预设
const colorPresets = [
  '#409EFF', // 蓝色
  '#67C23A', // 绿色
  '#E6A23C', // 黄色
  '#F56C6C', // 红色
  '#909399', // 灰色
  '#8B5CF6', // 紫色
  '#EC4899', // 粉色
  '#14B8A6', // 青色
]

// 背景颜色预设
const bgColorPresets = [
  'rgba(64, 158, 255, 0.1)', // 蓝色半透明
  'rgba(103, 194, 58, 0.1)', // 绿色半透明
  'rgba(230, 162, 60, 0.1)', // 黄色半透明
  'rgba(245, 108, 108, 0.1)', // 红色半透明
  'rgba(144, 147, 153, 0.1)', // 灰色半透明
  'rgba(139, 92, 246, 0.1)', // 紫色半透明
  'rgba(236, 72, 153, 0.1)', // 粉色半透明
  'rgba(20, 184, 166, 0.1)', // 青色半透明
  'rgba(255, 255, 255, 0.1)', // 白色半透明（默认）
  'rgba(255, 255, 255, 0.2)', // 更亮的白色
]

// 解析背景颜色，使用纯色半透明
const parseBackground = (bgColor) => {
  if (!bgColor) return 'rgba(255, 255, 255, 0.06)'
  return bgColor
}

// 调整颜色的透明度
const adjustOpacity = (color, opacityFactor) => {
  if (color.startsWith('rgba')) {
    const match = color.match(/rgba\((\d+),\s*(\d+),\s*(\d+),\s*([\d.]+)\)/)
    if (match) {
      const [, r, g, b] = match
      const newOpacity = (parseFloat(match[4]) * opacityFactor).toFixed(2)
      return `rgba(${r}, ${g}, ${b}, ${newOpacity})`
    }
  }
  return color
}

const DEFAULT_TRANSLATION_PROMPT_TEMPLATE = '你是专业翻译助手。任务：将用户文本翻译为{target_language}。\n要求：\n1) 自动识别源语言（如已提供{source_language}且不是"自动识别"，按其处理）。\n2) 忠实原意，不遗漏、不杜撰。\n3) 保留专有名词、代码、变量、URL、邮箱、数字与单位。\n4) 保持原文段落与换行结构。\n5) 只输出译文，不要任何说明。\n\n待翻译文本：\n{text}'
const DEFAULT_EXPLANATION_PROMPT_TEMPLATE = '你是清晰易懂的讲解助手。请使用{target_language}解释下列内容。\n要求：\n1) 先给一句话总结，再分点说明关键点。\n2) 面向普通用户，术语给简短释义。\n3) 保持准确，不编造；不确定时直接说明。\n4) 控制在180字以内。\n5) 仅输出解释内容。\n\n待解释文本：\n{text}'

const resetTranslationPromptTemplate = () => {
  props.form.translationPromptTemplate = DEFAULT_TRANSLATION_PROMPT_TEMPLATE
}

const resetExplanationPromptTemplate = () => {
  props.form.explanationPromptTemplate = DEFAULT_EXPLANATION_PROMPT_TEMPLATE
}

const addCustomPrompt = () => {
  if (!props.form.selectionCustomPrompts) {
    props.form.selectionCustomPrompts = []
  }
  const newPrompt = {
    name: '',
    prompt: '',
    icon: 'Star',
    color: '#909399',
    bg_color: 'rgba(255, 255, 255, 0.1)',
    enabled: true
  }
  props.form.selectionCustomPrompts.push(newPrompt)
}

const removeCustomPrompt = (index) => {
  props.form.selectionCustomPrompts.splice(index, 1)
}

// 验证提示词是否包含 {text} 占位符
const validatePrompt = (item) => {
  // 只在用户输入后验证，不在初始化时验证
  if (item.prompt && !item.prompt.includes('{text}')) {
    console.warn(`[Settings] Prompt "${item.name || '未命名'}" is missing {text} placeholder`)
  }
}
</script>

<style scoped>
.form-hint {
  font-size: 12px;
  color: var(--fy-text-muted);
  margin-top: 4px;
}

.section-title {
  font-size: 15px;
  font-weight: 600;
}

.form-actions {
  margin-top: 8px;
}

.custom-prompt-item {
  border: 1px solid var(--fy-border-light);
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 12px;
  background-color: var(--el-fill-color-light);
  transition: opacity 0.3s ease;
}

.custom-prompt-item.disabled {
  opacity: 0.6;
}

.prompt-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
}

.prompt-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.prompt-warning {
  display: flex;
  align-items: center;
  margin-top: 6px;
  padding: 6px 10px;
  background-color: var(--fy-warning-bg);
  border: 1px solid var(--fy-warning);
  border-radius: 4px;
  font-size: 12px;
  color: var(--fy-warning);
  line-height: 1.5;
}

.prompt-style-controls {
  display: flex;
  align-items: center;
  gap: 8px;
}

.icon-picker-grid {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 8px;
  max-height: 300px;
  overflow-y: auto;
  padding: 8px;
}

.icon-picker-item {
  width: 30px;
  height: 30px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s ease;
  color: var(--fy-text-secondary);
}

.icon-picker-item:hover {
  background-color: var(--fy-bg-surface);
  color: var(--fy-accent);
}

.icon-picker-item.selected {
  background-color: var(--fy-accent-bg);
  color: var(--fy-accent);
  border: 1px solid var(--fy-accent);
}

.preview-button-wrapper {
  margin-left: 8px;
}

.preview-button {
  border: none;
  width: 56px;
  height: 42px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 18px;
  transition: all 0.2s cubic-bezier(0.2, 0.8, 0.2, 1);
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  overflow: hidden;
}

.preview-button:hover {
  transform: translateY(-2px);
  box-shadow: var(--fy-shadow);
  background: var(--fy-bg-hover);
}

.preview-button:active {
  transform: translateY(0) scale(0.95);
}

.preview-button .btn-icon {
  opacity: 1;
  transform: translateY(0);
  transition: all 0.2s cubic-bezier(0.2, 0.8, 0.2, 1);
}

.preview-button .btn-text {
  position: absolute;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.5px;
  opacity: 0;
  transform: translateY(12px);
  transition: all 0.2s cubic-bezier(0.2, 0.8, 0.2, 1);
}

.preview-button:hover .btn-icon {
  opacity: 0;
  transform: translateY(-12px);
}

.preview-button:hover .btn-text {
  opacity: 1;
  transform: translateY(0);
}
</style>
