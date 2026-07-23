import {ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {ElMessage} from 'element-plus'
import {invoke} from '@tauri-apps/api/core'

/**
 * Shared composable for app actions used by both AppList and AppGrid
 * Extracts duplicated logic for removeApp, removeFromCategory, and confirmAddCommand
 */
export function useAppActions(emit) {
    const {t} = useI18n()
    const showCommandDialog = ref(false)
    const commandForm = ref({prefix: ''})
    const ctxApp = ref(null)

    const openAppDirectory = async (app) => {
        if (!app || !app.path) return
        try {
            await invoke('open_app_directory', {path: app.path})
        } catch (error) {
            console.error('打开应用目录失败:', error)
        }
    }

    const removeApp = async (app) => {
        if (!app || !app.id) return
        try {
            await invoke('remove_app_record', {appId: app.id})
            emit('category-changed')
        } catch (error) {
            console.error('Remove app error:', error)
        }
    }

    const removeFromCategory = async (app) => {
        if (!app || !app.id) return
        try {
            await invoke('set_app_category', {appId: app.id, categoryId: ''})
            emit('category-changed')
        } catch (error) {
            console.error('Remove category error:', error)
        }
    }

    const showAddCommandDialog = (app) => {
        if (!app) return
        ctxApp.value = app
        // Auto-generate prefix from app name
        const prefix = app.title.toLowerCase().replace(/[^a-z0-9]/g, '').substring(0, 10)
        commandForm.value = {prefix}
        showCommandDialog.value = true
    }

    const closeCommandDialog = () => {
        showCommandDialog.value = false
        ctxApp.value = null
    }

    const confirmAddCommand = async () => {
        const app = ctxApp.value
        if (!app || !commandForm.value.prefix.trim()) return

        try {
            // Load config to check for existing commands
            const config = await invoke('get_launcher_config')
            const existingCommands = config.custom_commands || []

            // Check if app already has a command
            const existingCommand = existingCommands.find(cmd => {
                if (cmd.command_type?.RunProgram) {
                    return cmd.command_type.RunProgram.path === app.path
                }
                return false
            })

            if (existingCommand) {
                ElMessage({
                    message: `该应用已有命令 "${existingCommand.prefix}"，请勿重复添加`,
                    type: 'warning',
                    duration: 3000,
                    offset: 60
                })
                return
            }

            // Check if prefix is already used
            const finalPrefix = ':' + commandForm.value.prefix.trim()
            const prefixExists = existingCommands.some(cmd => cmd.prefix === finalPrefix)
            if (prefixExists) {
                ElMessage({
                    message: `命令前缀 "${finalPrefix}" 已被使用，请使用其他前缀`,
                    type: 'warning',
                    duration: 3000,
                    offset: 60
                })
                return
            }

            // Build command type - RunProgram
            const commandType = {
                RunProgram: {
                    path: app.path,
                    args: null
                }
            }

            await invoke('add_custom_command', {
                prefix: finalPrefix,
                title: app.title,
                description: `启动 ${app.title}`,
                icon: app.icon_base64 || 'Monitor',
                commandType: commandType
            })

            closeCommandDialog()
            emit('category-changed')
        } catch (error) {
            console.error('添加命令失败:', error)
            ElMessage({
                message: error,
                type: 'error',
                duration: 3000,
                offset: 60
            })
        }
    }

    return {
        showCommandDialog,
        commandForm,
        ctxApp,
        openAppDirectory,
        removeApp,
        removeFromCategory,
        showAddCommandDialog,
        closeCommandDialog,
        confirmAddCommand
    }
}
