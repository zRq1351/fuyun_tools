import {invoke} from '@tauri-apps/api/core'
import {ElMessage} from 'element-plus'
import {useI18n} from 'vue-i18n'

export function useLauncherSearch() {
    const {t} = useI18n()
    const builtinCommands = [
        {prefix: ':settings', title: '打开设置', icon: 'setting', action: 'open_settings', type: '命令'},
        {prefix: ':clipboard', title: '打开剪贴板', icon: 'clipboard', action: 'open_clipboard', type: '命令'},
        {prefix: ':screenshot', title: '启动截图', icon: 'search', action: 'start_screenshot', type: '命令'},
        {prefix: ':record', title: '启动录屏', icon: 'monitor', action: 'start_recording', type: '命令'}
    ]

    let customCommandsCache = []
    let customCommandsLoaded = false

    const loadCustomCommands = async () => {
        try {
            const config = await invoke('get_launcher_config')
            customCommandsCache = (config.custom_commands || [])
                .filter(cmd => cmd.enabled)
                .map(cmd => ({
                    id: cmd.id,
                    prefix: cmd.prefix,
                    title: cmd.title,
                    description: cmd.description,
                    icon: cmd.icon,
                    type: '自定义',
                    action: 'custom_command',
                    commandType: cmd.command_type,
                    shortcut: cmd.prefix
                }))
            customCommandsLoaded = true
        } catch (error) {
            console.error('加载自定义命令失败:', error)
            customCommandsCache = []
        }
    }

    const findCommand = (text) => {
        const lowerText = text.toLowerCase()
        const allCommands = [...builtinCommands, ...customCommandsCache]
        return allCommands.filter(cmd => {
            const prefix = cmd.prefix.toLowerCase()
            // Check if query matches prefix (prefix starts with query OR query starts with prefix)
            return prefix.startsWith(lowerText) || lowerText.startsWith(prefix)
        })
    }

    const searchApps = (query, allApps) => {
        const lowerQuery = query.toLowerCase()
        const filtered = allApps.filter(a =>
            a.title.toLowerCase().includes(lowerQuery)
        )
        return filtered.slice(0, 10).map(app => ({
            id: app.id,
            title: app.title,
            description: app.path,
            icon: app.icon_base64 || 'app',
            item_type: '应用',
            action: 'launch_app',
            path: app.path,
            shortcut: null,
            result: null
        }))
    }

    const search = async (query, allApps) => {
        if (!query.trim()) return []

        // Ensure custom commands are loaded
        if (!customCommandsLoaded) {
            await loadCustomCommands()
        }

        const results = []

        const matchedCommands = findCommand(query)
        if (matchedCommands.length > 0) {
            results.push(...matchedCommands.map(cmd => ({
                id: cmd.action,
                title: cmd.title,
                icon: cmd.icon,
                type: cmd.type,
                action: cmd.action,
                shortcut: cmd.prefix,
                commandType: cmd.commandType
            })))
        }

        const appResults = searchApps(query, allApps)
        results.push(...appResults)

        return results
    }

    const executeAction = async (item) => {
        const action = item.action || 'launch_app'
        try {
            switch (action) {
                case 'open_settings':
                    await invoke('show_standard_window_command', {label: 'settings'})
                    break
                case 'open_clipboard':
                    await invoke('show_clipboard_window_command')
                    break
                case 'start_screenshot':
                    await invoke('start_screenshot_command')
                    break
                case 'start_recording':
                    await invoke('toggle_recording_command')
                    break
                case 'custom_command':
                    await executeCustomCommand(item)
                    break
                case 'launch_app':
                    if (item.path) {
                        await invoke('launch_app', {appId: item.id, path: item.path})
                    }
                    break
                case 'open_file':
                    if (item.path) {
                        await invoke('open_file', {path: item.path})
                    }
                    break
                default:
                    if (item.path) {
                        await invoke('launch_app', {appId: item.id, path: item.path})
                    }
            }
        } catch (error) {
            console.error('[Custom command] Execution failed:', error)
            ElMessage({
                message: t('launcher.actionFailed', {error: error.message || String(error)}),
                type: 'error',
                duration: 3000
            })
        }
    }

    const executeCustomCommand = async (item) => {
        const cmdType = item.commandType
        if (!cmdType) {
            console.error(t('launcher.customCommandTypeEmpty'))
            return
        }

        try {
            if (cmdType.OpenWindow) {
                await invoke('show_standard_window_command', {label: cmdType.OpenWindow.label})
            } else if (cmdType.ExecuteAction) {
                const action = cmdType.ExecuteAction.action
                switch (action) {
                    case 'screenshot':
                        await invoke('start_screenshot_command')
                        break
                    case 'recording':
                        await invoke('toggle_recording_command')
                        break
                    default:
                        console.warn(t('launcher.unknownCustomAction'), action)
                }
            } else if (cmdType.CopyText) {
                await invoke('copy_to_clipboard', {text: cmdType.CopyText.text})
            } else if (cmdType.RunProgram) {
                if (!cmdType.RunProgram.path) {
                    throw new Error(t('launcher.emptyProgramPath'))
                }

                const hasArgs = cmdType.RunProgram.args && cmdType.RunProgram.args.trim() !== ''
                if (hasArgs) {
                    await invoke('launch_app_with_args', {
                        appId: item.id || 'custom_command',
                        path: cmdType.RunProgram.path,
                        args: cmdType.RunProgram.args
                    })
                } else {
                    await invoke('launch_app', {
                        appId: item.id || 'custom_command',
                        path: cmdType.RunProgram.path
                    })
                }
            } else {
                console.error(t('launcher.commandTypeUnknown'), cmdType)
                throw new Error(t('launcher.unknownCommandType'))
            }
        } catch (error) {
            console.error(t('launcher.commandExecutionFailed'), error)
            ElMessage({
                message: t('launcher.launchFailed', {error: error.message || String(error)}),
                type: 'error',
                duration: 5000
            })
            throw error
        }
    }

    const reloadCustomCommands = async () => {
        customCommandsLoaded = false
        await loadCustomCommands()
    }

    return {search, executeAction, commands: builtinCommands, loadCustomCommands, reloadCustomCommands}
}
