<template>
  <el-form :model="form" label-position="top">
    <el-card class="setting-section-card" shadow="never">
      <template #header>
        <div class="section-title">{{ $t('settings.ai.serviceConnection') }}</div>
      </template>

      <el-form-item :label="$t('settings.ai.provider')">
        <div style="display: flex; gap: 8px; width: 100%; margin-bottom: 8px;">
          <el-input v-model="newProviderName" :placeholder="$t('settings.ai.newProvider')" style="flex:1"
                    @keyup.enter="addNewProvider"/>
          <el-button :disabled="!newProviderName.trim()" type="primary" @click="addNewProvider">
            {{ $t('settings.ai.add') }}
          </el-button>
        </div>
        <el-select v-model="form.aiProvider" :placeholder="$t('settings.ai.selectProvider')"
                   style="width: 100%" @change="handleProviderChange">
          <el-option v-for="p in providers" :key="p.value" :label="p.label" :value="p.value">
            <div class="provider-option-row">
              <span>{{ p.label }}</span>
              <el-button class="provider-option-del" link type="danger" @click.stop.prevent="removeProvider(p.value)">
                <el-icon>
                  <CloseBold/>
                </el-icon>
              </el-button>
            </div>
          </el-option>
        </el-select>
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
        <el-input v-model="form.apiKey" :placeholder="$t('settings.ai.apiKeyPlaceholder')" show-password
                  type="password"/>
      </el-form-item>

    </el-card>
  </el-form>
</template>

<script setup>
import {onMounted} from 'vue'
import {CloseBold, Connection} from '@element-plus/icons-vue'
import {useAIProvider} from '../composables/useAIProvider'

const props = defineProps({form: {type: Object, required: true}})

const {
  providers,
  testingConnection,
  newProviderName,
  isRemovableProvider,
  loadAiProviders,
  addNewProvider,
  handleProviderChange,
  applyCurrentProviderConfig,
  removeProvider,
  testConnection,
} = useAIProvider(props.form)

defineExpose({loadAiProviders, applyCurrentProviderConfig})

onMounted(() => loadAiProviders())
</script>

<style scoped>
.setting-section-card + .setting-section-card {
  margin-top: 16px;
}

.section-title {
  font-size: 15px;
  font-weight: 600;
}

.provider-option-row {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.provider-option-del {
  padding: 2px;
  margin-left: 8px;
}
</style>
