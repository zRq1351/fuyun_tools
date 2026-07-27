import {ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage, ElMessageBox} from 'element-plus'
import {AISettingsService} from '../../../services/ipc'
import {handleAppError} from '../../../utils/errorHandler'

export function useAIProvider(form) {
    const {t} = useI18n()
    const providers = ref([])
    const testingConnection = ref(false)
    const newProviderName = ref('')

    const isRemovableProvider = (_provider) => true

    const loadAiProviders = async () => {
        try {
            const settings = await AISettingsService.getSettings()
            const configs = settings.provider_configs || {}
            providers.value = Object.keys(configs).map(key => ({
                value: key,
                label: key,
            }))
        } catch (error) {
            handleAppError(error, t('settings.ai.loadProviderFailed'))
        }
    }

    const addNewProvider = async () => {
        const name = newProviderName.value.trim()
        if (!name) {
            ElMessage.warning(t('settings.ai.providerNameRequired'))
            return
        }
        const settings = await AISettingsService.getSettings()
        if ((settings.provider_configs || {})[name]) {
            ElMessage.warning(t('settings.ai.providerExists'))
            return
        }
        newProviderName.value = ''
        await AISettingsService.saveSettings({
            aiProvider: name,
            aiApiUrl: '',
            aiModelName: '',
            aiApiKey: '',
        })
        form.aiProvider = name
        form.apiUrl = ''
        form.modelName = ''
        form.apiKey = ''
        await loadAiProviders()
    }

    const handleProviderChange = async (provider) => {
        if (!provider) return
        try {
            const settings = await AISettingsService.getSettings()
            const configs = settings.provider_configs || {}
            if (configs[provider]) {
                form.apiUrl = configs[provider].api_url || ''
                form.modelName = configs[provider].model_name || ''
                form.apiKey = configs[provider].api_key || ''
            } else {
                form.apiUrl = ''
                form.modelName = ''
                form.apiKey = ''
            }
        } catch (error) {
            handleAppError(error, t('settings.ai.loadConfigFailed'))
        }
    }

    const applyCurrentProviderConfig = (settings) => {
        form.aiProvider = settings.ai_provider || ''
        const configs = settings.provider_configs || {}
        const cfg = configs[form.aiProvider]
        if (cfg) {
            form.apiUrl = cfg.api_url || ''
            form.modelName = cfg.model_name || ''
            form.apiKey = cfg.api_key || ''
        } else {
            form.apiUrl = ''
            form.modelName = ''
            form.apiKey = ''
        }
    }

    const removeProvider = async (provider) => {
        if (!provider) return
        try {
            await ElMessageBox.confirm(
                t('settings.ai.deleteProviderConfirm', {provider}),
                t('settings.ai.deleteProviderTitle'),
                {
                    confirmButtonText: t('common.delete'),
                    cancelButtonText: t('common.cancel'),
                    type: 'warning',
                }
            )
            await AISettingsService.removeProvider(provider)
            await loadAiProviders()
            if (form.aiProvider === provider) {
                form.aiProvider = ''
                form.apiUrl = ''
                form.modelName = ''
                form.apiKey = ''
            }
            ElMessage.success(t('settings.ai.providerDeleted', {provider}))
        } catch (error) {
            if (error !== 'cancel') {
                handleAppError(error, t('settings.ai.deleteFailed'))
            }
        }
    }

    const testConnection = async () => {
        if (!form.apiUrl || !form.modelName || !form.apiKey) {
            ElMessage.warning(t('settings.ai.fillAllInfo'))
            return
        }
        testingConnection.value = true
        try {
            const result = await AISettingsService.testConnection({
                aiProvider: form.aiProvider,
                aiApiUrl: form.apiUrl,
                aiModelName: form.modelName,
                aiApiKey: form.apiKey
            })
            ElMessage.success(result)
        } catch (error) {
            handleAppError(error, t('settings.ai.testFailed'))
        } finally {
            testingConnection.value = false
        }
    }

    return {
        providers,
        testingConnection,
        newProviderName,
        isRemovableProvider,
        loadAiProviders,
        addNewProvider,
        handleProviderChange,
        applyCurrentProviderConfig,
        removeProvider,
        testConnection
    }
}
