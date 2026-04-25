<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">划词能力</div>
      </template>
      <el-form-item label="划词功能">
        <el-switch v-model="form.selectionEnabled" active-text="启用" inactive-text="关闭"/>
        <div class="form-hint">关闭后不再触发划词工具栏与AI功能</div>
      </el-form-item>

      <el-form-item label="划词触发辅助键">
        <el-select v-model="form.selectionModifierKey" placeholder="请选择">
          <el-option label="无 (禁用 Ctrl 键触发)" value="" />
          <el-option label="Ctrl 键" value="Ctrl" />
        </el-select>
        <div class="form-hint">设置必须按下特定辅助键并划词才触发，防止误触</div>
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
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">网页搜索入口</div>
      </template>
      <el-form-item label="启用搜索入口">
        <el-switch v-model="form.selectionWebSearchEnabled" active-text="启用" inactive-text="关闭"/>
        <div class="form-hint">在划词工具栏中提供搜索引擎的快捷按钮</div>
      </el-form-item>
      <el-form-item label="默认搜索引擎" v-if="form.selectionWebSearchEnabled">
        <el-select v-model="form.selectionWebSearchEngine" placeholder="请选择搜索引擎">
          <el-option label="Bing" value="bing" />
          <el-option label="Google" value="google" />
          <el-option label="Baidu" value="baidu" />
          <el-option label="DuckDuckGo" value="duckduckgo" />
        </el-select>
      </el-form-item>
    </el-card>

    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">自定义 AI 按钮</div>
      </template>
      <div class="form-hint" style="margin-bottom: 12px;">在工具栏上增加额外的功能按钮，如“总结”、“查文档”等。</div>
      
      <div v-for="(item, index) in form.selectionCustomPrompts" :key="index" class="custom-prompt-item">
        <div class="prompt-header">
          <el-input v-model="item.name" placeholder="按钮名称（例如：总结）" style="width: 200px;" />
          <el-button type="danger" link @click="removeCustomPrompt(index)">删除</el-button>
        </div>
        <el-input
            v-model="item.prompt"
            :rows="3"
            placeholder="提示词模板，必须包含 {text} 变量"
            type="textarea"
            style="margin-top: 8px;"
        />
      </div>

      <el-button type="primary" plain @click="addCustomPrompt" style="margin-top: 12px;">+ 添加自定义按钮</el-button>
    </el-card>
  </el-form>
</template>

<script setup>
const props = defineProps({
  form: {
    type: Object,
    required: true
  }
})

const DEFAULT_TRANSLATION_PROMPT_TEMPLATE = '你是专业翻译助手。任务：将用户文本翻译为{target_language}。\n要求：\n1) 自动识别源语言（如已提供{source_language}且不是“自动识别”，按其处理）。\n2) 忠实原意，不遗漏、不杜撰。\n3) 保留专有名词、代码、变量、URL、邮箱、数字与单位。\n4) 保持原文段落与换行结构。\n5) 只输出译文，不要任何说明。\n\n待翻译文本：\n{text}'
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
  props.form.selectionCustomPrompts.push({
    name: '',
    prompt: ''
  })
}

const removeCustomPrompt = (index) => {
  props.form.selectionCustomPrompts.splice(index, 1)
}
</script>

<style scoped>
.form-hint {
  font-size: 12px;
  color: #909399;
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
  border: 1px solid var(--el-border-color-light);
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 12px;
  background-color: var(--el-fill-color-light);
}

.prompt-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
</style>
