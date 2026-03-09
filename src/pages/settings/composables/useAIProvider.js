import {ref} from 'vue'
import {ElMessage, ElMessageBox} from 'element-plus'
import {AISettingsService} from '../../../services/ipc'
import {handleAppError} from '../../../utils/errorHandler'

export function useAIProvider(form) {
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
            handleAppError(error, '加载AI提供商失败')
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
            handleAppError(error, '加载提供商配置失败')
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
                `确定删除自定义提供商 "${provider}" 吗？`,
                '删除提供商',
                {
                    confirmButtonText: '删除',
                    cancelButtonText: '取消',
                    type: 'warning',
                }
            )

            await AISettingsService.removeProvider(provider)
            await loadAiProviders()
            const settings = await AISettingsService.getSettings()
            applyCurrentProviderConfig(settings)
            ElMessage.success(`已删除提供商 "${provider}"`)
        } catch (error) {
            if (error !== 'cancel') {
                handleAppError(error, '删除失败')
            }
        }
    }

    const testConnection = async () => {
        if (!form.apiUrl || !form.modelName || !form.apiKey) {
            ElMessage.warning('请填写完整信息后再测试')
            return
        }
        testingConnection.value = true
        try {
            const result = await AISettingsService.testConnection({
                aiApiUrl: form.apiUrl,
                aiModelName: form.modelName,
                aiApiKey: form.apiKey
            })
            ElMessage.success(result)
        } catch (error) {
            handleAppError(error, '连接测试失败')
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
