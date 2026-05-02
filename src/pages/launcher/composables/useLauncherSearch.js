import {invoke} from '@tauri-apps/api/core'

export function useLauncherSearch() {
    // 内置命令
    const builtinCommands = [
        {prefix: ':settings', title: '打开设置', icon: 'setting', action: 'open_settings', type: '命令'},
        {prefix: ':clipboard', title: '打开剪贴板', icon: 'clipboard', action: 'open_clipboard', type: '命令'},
        {prefix: ':screenshot', title: '启动截图', icon: 'search', action: 'start_screenshot', type: '命令'},
        {prefix: ':record', title: '启动录屏', icon: 'monitor', action: 'start_recording', type: '命令'},
        {prefix: ':calc', title: '计算器', icon: 'calculator', action: 'calculator', type: '命令'}
    ]

    // 自定义命令缓存
    let customCommandsCache = []

    const isMathExpression = (text) => {
        const mathPattern = /^[\d\s+\-*/().%^]+$/
        return mathPattern.test(text) && /[+\-*/^%]/.test(text)
    }

    // 加载自定义命令
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
        } catch (error) {
            console.error('加载自定义命令失败:', error)
            customCommandsCache = []
        }
    }

    const findCommand = (text) => {
        const lowerText = text.toLowerCase()
        // 合并内置命令和自定义命令
        const allCommands = [...builtinCommands, ...customCommandsCache]
        return allCommands.filter(cmd =>
            cmd.prefix.startsWith(lowerText) || lowerText.startsWith(cmd.prefix)
        )
    }

    const searchAppsAndFiles = async (query) => {
        try {
            return await invoke('search_launcher_items', {query, limit: 10})
        } catch (error) {
            console.error('Search error:', error)
            return []
        }
    }

    const calculateExpression = async (expr) => {
        try {
            return await invoke('calculate_expression', {expr})
        } catch (error) {
            console.error('Calculation error:', error)
            return null
        }
    }

    const search = async (query) => {
        if (!query.trim()) return []

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
                commandType: cmd.commandType  // 保留 commandType 字段
            })))
        }

        if (isMathExpression(query)) {
            const calcResult = await calculateExpression(query)
            if (calcResult !== null) {
                results.unshift({
                    id: 'calc-result',
                    title: `= ${calcResult}`,
                    description: `计算 ${query} 的结果`,
                    icon: 'calculator',
                    type: '计算',
                    action: 'copy_result',
                    result: calcResult
                })
            }
        }

        const appResults = await searchAppsAndFiles(query)
        results.push(...appResults)

        return results
    }

    const executeAction = async (item) => {
        const action = item.action || 'launch_app'
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
            case 'calculator':
            case 'copy_result':
                if (item.result) {
                    await invoke('copy_to_clipboard', {text: item.result})
                }
                break
            case 'custom_command':
                // 执行自定义命令
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
    }

    // 执行自定义命令
    const executeCustomCommand = async (item) => {
        const cmdType = item.commandType
        if (!cmdType) {
            console.error('[自定义命令] commandType 为空')
            return
        }

        try {
            if (cmdType.OpenWindow) {
                // 打开窗口
                await invoke('show_standard_window_command', {label: cmdType.OpenWindow.label})
            } else if (cmdType.ExecuteAction) {
                // 执行操作
                const action = cmdType.ExecuteAction.action
                switch (action) {
                    case 'screenshot':
                        await invoke('start_screenshot_command')
                        break
                    case 'recording':
                        await invoke('toggle_recording_command')
                        break
                    default:
                        console.warn('未知的自定义动作:', action)
                }
            } else if (cmdType.CopyText) {
                // 复制文本
                await invoke('copy_to_clipboard', {text: cmdType.CopyText.text})
            } else if (cmdType.RunProgram) {
                // 运行程序
                console.log('[自定义命令] RunProgram 完整信息:', {
                    path: cmdType.RunProgram.path,
                    args: cmdType.RunProgram.args,
                    pathType: typeof cmdType.RunProgram.path,
                    pathLength: cmdType.RunProgram.path?.length
                })

                if (!cmdType.RunProgram.path) {
                    throw new Error('程序路径为空')
                }

                const hasArgs = cmdType.RunProgram.args && cmdType.RunProgram.args.trim() !== ''
                if (hasArgs) {
                    // 有参数时使用新命令
                    console.log('[自定义命令] 使用 launch_app_with_args')
                    await invoke('launch_app_with_args', {
                        appId: item.id || 'custom_command',
                        path: cmdType.RunProgram.path,
                        args: cmdType.RunProgram.args
                    })
                } else {
                    // 无参数时使用原有命令（更简单）
                    console.log('[自定义命令] 使用 launch_app, path:', cmdType.RunProgram.path)
                    await invoke('launch_app', {
                        appId: item.id || 'custom_command',
                        path: cmdType.RunProgram.path
                    })
                }
            } else {
                console.error('[自定义命令] 未知的命令类型:', cmdType)
                throw new Error('未知的命令类型')
            }
        } catch (error) {
            console.error('[自定义命令] 执行失败:', error)
            // 显示错误提示
            if (typeof ElMessage !== 'undefined') {
                ElMessage({
                    message: `启动失败: ${error.message || error}`,
                    type: 'error',
                    duration: 5000
                })
            }
            throw error
        }
    }

    return {search, executeAction, commands: builtinCommands, loadCustomCommands}
}
