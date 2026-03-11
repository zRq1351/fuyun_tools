<template>
  <el-form :model="form" label-position="top">
    <el-form-item label="AI服务提供商">
      <el-select v-model="form.aiProvider" class="provider-select" placeholder="请选择提供商"
                 @change="handleProviderChange">
        <el-option
            v-for="provider in providers"
            :key="provider.value"
            :label="provider.label"
            :value="provider.value"
        >
          <div class="provider-option-row">
            <span class="provider-option-label">{{ provider.label }}</span>
            <el-button
                v-if="isRemovableProvider(provider.value)"
                class="provider-option-delete"
                link
                type="danger"
                @click.stop.prevent="removeProvider(provider.value)"
            >
              <el-icon>
                <CloseBold/>
              </el-icon>
            </el-button>
          </div>
        </el-option>
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

    <el-form-item label="划词功能">
      <el-switch v-model="form.selectionEnabled" active-text="启用" inactive-text="关闭"/>
      <div class="form-hint">关闭后不再触发划词工具栏与AI功能</div>
    </el-form-item>

    <el-form-item label="翻译提示词模板">
      <el-input
          v-model="form.translationPromptTemplate"
          :rows="4"
          placeholder="可使用变量：{text}、{source_language}、{target_language}"
          type="textarea"
      />
      <div class="form-actions">
        <el-button size="small" @click="resetTranslationPromptTemplate">默认</el-button>
      </div>
      <div class="form-hint">用于划词翻译，可通过变量控制提示词格式</div>
    </el-form-item>

    <el-form-item label="解释提示词模板">
      <el-input
          v-model="form.explanationPromptTemplate"
          :rows="4"
          placeholder="可使用变量：{text}、{target_language}"
          type="textarea"
      />
      <div class="form-actions">
        <el-button size="small" @click="resetExplanationPromptTemplate">默认</el-button>
      </div>
      <div class="form-hint">用于划词解释，可通过变量控制输出风格</div>
    </el-form-item>
  </el-form>
</template>

<script setup>
import {onMounted} from 'vue'
import {CloseBold, Connection} from '@element-plus/icons-vue'
import {useAIProvider} from '../composables/useAIProvider'

const props = defineProps({
  form: {
    type: Object,
    required: true
  }
})

const {
  providers,
  testingConnection,
  isRemovableProvider,
  loadAiProviders,
  handleProviderChange,
  applyCurrentProviderConfig,
  removeProvider,
  testConnection
} = useAIProvider(props.form)

defineExpose({
  loadAiProviders,
  applyCurrentProviderConfig
})

onMounted(() => {
  loadAiProviders()
})

const DEFAULT_TRANSLATION_PROMPT_TEMPLATE = '你是专业翻译助手。任务：将用户文本翻译为{target_language}。\n要求：\n1) 自动识别源语言（如已提供{source_language}且不是“自动识别”，按其处理）。\n2) 忠实原意，不遗漏、不杜撰。\n3) 保留专有名词、代码、变量、URL、邮箱、数字与单位。\n4) 保持原文段落与换行结构。\n5) 只输出译文，不要任何说明。\n\n待翻译文本：\n{text}'
const DEFAULT_EXPLANATION_PROMPT_TEMPLATE = '你是清晰易懂的讲解助手。请使用{target_language}解释下列内容。\n要求：\n1) 先给一句话总结，再分点说明关键点。\n2) 面向普通用户，术语给简短释义。\n3) 保持准确，不编造；不确定时直接说明。\n4) 控制在180字以内。\n5) 仅输出解释内容。\n\n待解释文本：\n{text}'

const resetTranslationPromptTemplate = () => {
  props.form.translationPromptTemplate = DEFAULT_TRANSLATION_PROMPT_TEMPLATE
}

const resetExplanationPromptTemplate = () => {
  props.form.explanationPromptTemplate = DEFAULT_EXPLANATION_PROMPT_TEMPLATE
}
</script>

<style scoped>
.form-hint {
  font-size: 12px;
  color: #909399;
  margin-top: 4px;
}

.form-actions {
  margin-top: 8px;
}

.provider-select {
  flex: 1;
}

.provider-option-row {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.provider-option-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.provider-option-delete {
  padding: 2px;
}
</style>
