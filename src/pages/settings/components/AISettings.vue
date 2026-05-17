<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.ai.serviceConnection') }}</div>
      </template>
      <el-form-item :label="$t('settings.ai.provider')">
        <el-select v-model="form.aiProvider" :placeholder="$t('settings.ai.selectProvider')" class="provider-select"
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
          <el-option :label="$t('settings.ai.custom')" value="custom"/>
        </el-select>
      </el-form-item>

      <el-form-item v-if="form.aiProvider === 'custom'" :label="$t('settings.ai.customProviderName')">
        <el-input v-model="form.customProviderName" :placeholder="$t('settings.ai.customNamePlaceholder')"/>
      </el-form-item>

      <el-form-item :label="$t('settings.ai.apiUrl')">
        <el-input v-model="form.apiUrl" :placeholder="$t('settings.ai.apiUrlPlaceholder')">
          <template #append>
            <el-button :loading="testingConnection" @click="testConnection">
              <el-icon>
                <Connection/>
              </el-icon>
            </el-button>
          </template>
        </el-input>
      </el-form-item>

      <el-form-item :label="$t('settings.ai.modelName')">
        <el-input v-model="form.modelName" :placeholder="$t('settings.ai.modelNamePlaceholder')"/>
      </el-form-item>

      <el-form-item :label="$t('settings.ai.apiKey')">
        <el-input
            v-model="form.apiKey"
            :placeholder="$t('settings.ai.apiKeyPlaceholder')"
            show-password
            type="password"
        />
      </el-form-item>
    </el-card>

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
.setting-section-card + .setting-section-card {
  margin-top: 16px;
}

.section-title {
  font-size: 15px;
  font-weight: 600;
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
