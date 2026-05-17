import {ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage, ElMessageBox} from 'element-plus'
import {AISettingsService} from '../../../services/ipc'
import {handleAppError} from '../../../utils/errorHandler'

export function useAIProvider(form) {
    const {t} = useI18n()
    const providers = ref([])
    const testingConnection = ref(false)

    const builtinProviders = new Set(['deepseek', 'qwen', 'xiaomimimo'])
    const isRemovableProvider = (provider) => !!provider && provider !== 'custom' && !builtinProviders.has(provider)

    const loadAiProviders = async () => {
        try {
            const result = await AISettingsService.getAllConfiguredProviders()
            if (Array.isArray(result)) {
                providers.value = result.map(([value, label]) => ({value, label}))
            }
        } catch (error) {
            handleAppError(error, t('settings.ai.loadProviderFailed'))
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
            const settings = await AISettingsService.getSettings()
            const providerConfigs = settings.provider_configs || {}

            if (providerConfigs[provider]) {
                const config = providerConfigs[provider]
                form.apiUrl = config.api_url || ''
                form.modelName = config.model_name || ''
                form.apiKey = config.api_key || ''
            } else {
                const configResult = await AISettingsService.getProviderConfig(provider)
                if (Array.isArray(configResult) && configResult.length >= 2) {
                    const [url, model] = configResult
                    form.apiUrl = url || ''
                    form.modelName = model || ''
                    form.apiKey = ''
                }
            }
        } catch (error) {
            handleAppError(error, t('settings.ai.loadConfigFailed'))
        }
    }

    const applyCurrentProviderConfig = (settings) => {
        form.aiProvider = settings.ai_provider || ''
        form.customProviderName = ''
        const currentProvider = form.aiProvider
        const providerConfigs = settings.provider_configs || {}

        if (currentProvider && providerConfigs[currentProvider]) {
            const config = providerConfigs[currentProvider]
            form.apiUrl = config.api_url || ''
            form.modelName = config.model_name || ''
            form.apiKey = config.api_key || ''
        } else {
            form.apiUrl = ''
            form.modelName = ''
            form.apiKey = ''
        }
    }

    const removeProvider = async (provider) => {
        if (!isRemovableProvider(provider)) {
            return
        }

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
            const settings = await AISettingsService.getSettings()
            applyCurrentProviderConfig(settings)
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
            let provider = form.aiProvider
            if (provider === 'custom') {
                provider = form.customProviderName
            }
            const result = await AISettingsService.testConnection({
                aiProvider: provider,
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
        isRemovableProvider,
        loadAiProviders,
        handleProviderChange,
        applyCurrentProviderConfig,
        removeProvider,
        testConnection
    }
}
