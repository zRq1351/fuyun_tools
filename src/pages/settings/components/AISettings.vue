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
</script>

<style scoped>
.form-hint {
  font-size: 12px;
  color: #909399;
  margin-top: 4px;
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
