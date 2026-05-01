import {invoke} from '@tauri-apps/api/core'

export function useLauncherSearch() {
    // 命令映射
    const commands = [
        {prefix: ':settings', title: '打开设置', icon: 'setting', action: 'open_settings', type: '命令'},
        {prefix: ':clipboard', title: '打开剪贴板', icon: 'clipboard', action: 'open_clipboard', type: '命令'},
        {prefix: ':screenshot', title: '启动截图', icon: 'search', action: 'start_screenshot', type: '命令'},
        {prefix: ':record', title: '启动录屏', icon: 'monitor', action: 'start_recording', type: '命令'},
        {prefix: ':calc', title: '计算器', icon: 'calculator', action: 'calculator', type: '命令'}
    ]

    // 检查是否是数学表达式
    const isMathExpression = (text) => {
        const mathPattern = /^[\d\s+\-*/().%^]+$/
        return mathPattern.test(text) && /[+\-*/^%]/.test(text)
    }

    // 检查是否是命令
    const findCommand = (text) => {
        const lowerText = text.toLowerCase()
        return commands.filter(cmd => cmd.prefix.startsWith(lowerText) || lowerText.startsWith(cmd.prefix))
    }

    // 搜索应用和文件
    const searchAppsAndFiles = async (query) => {
        try {
            const results = await invoke('search_launcher_items', {query, limit: 10})
            return results
        } catch (error) {
            console.error('Search error:', error)
            return []
        }
    }

    // 计算数学表达式
    const calculateExpression = async (expr) => {
        try {
            const result = await invoke('calculate_expression', {expr})
            return result
        } catch (error) {
            console.error('Calculation error:', error)
            return null
        }
    }

    // 主搜索函数
    const search = async (query) => {
        if (!query.trim()) return []

        const results = []

        // 1. 检查是否是命令
        const matchedCommands = findCommand(query)
        if (matchedCommands.length > 0) {
            results.push(...matchedCommands.map(cmd => ({
                id: cmd.action,
                title: cmd.title,
                icon: cmd.icon,
                type: cmd.type,
                action: cmd.action,
                shortcut: cmd.prefix
            })))
        }

        // 2. 检查是否是数学表达式
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

        // 3. 搜索应用和文件
        const appResults = await searchAppsAndFiles(query)
        results.push(...appResults)

        return results
    }

    // 执行操作
    const executeAction = async (item) => {
        switch (item.action) {
            case 'open_settings':
                await invoke('show_standard_window_by_label', {label: 'settings'})
                break
            case 'open_clipboard':
                await invoke('show_clipboard_window_command')
                break
            case 'start_screenshot':
                await invoke('start_screenshot_command')
                break
            case 'start_recording':
                await invoke('toggle_recording')
                break
            case 'calculator':
                // 复制计算结果
                if (item.result) {
                    await invoke('copy_to_clipboard', {text: item.result})
                }
                break
            case 'copy_result':
                if (item.result) {
                    await invoke('copy_to_clipboard', {text: item.result})
                }
                break
            case 'launch_app':
                if (item.path) {
                    await invoke('launch_app', {path: item.path})
                }
                break
            case 'open_file':
                if (item.path) {
                    await invoke('open_file', {path: item.path})
                }
                break
            default:
                console.warn('Unknown action:', item.action)
        }
    }

    return {
        search,
        executeAction,
        commands
    }
}
