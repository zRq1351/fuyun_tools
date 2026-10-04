import {describe, it, expect, vi, beforeEach} from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({invoke: vi.fn()}))
vi.mock('element-plus', () => ({ElMessage: vi.fn()}))
vi.mock('vue-i18n', () => ({useI18n: () => ({t: (key) => key})}))

import {invoke} from '@tauri-apps/api/core'
import {ElMessage} from 'element-plus'
import {useLauncherSearch} from '../pages/launcher/composables/useLauncherSearch.js'

const appItem = {action: 'launch_app', id: 'app-1', path: 'C:\\gone\\app.exe'}
const commandItem = {
    action: 'custom_command',
    id: 'cmd-1',
    commandType: {RunProgram: {path: 'C:\\gone\\app.exe', args: null}}
}

describe('executeAction APP_NOT_FOUND 传播（F17）', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        vi.spyOn(console, 'error').mockImplementation(() => {})
    })

    it('launch_app 后端 reject 字符串 APP_NOT_FOUND 时 rethrow，不 toast', async () => {
        invoke.mockRejectedValue('APP_NOT_FOUND')
        const {executeAction} = useLauncherSearch()
        await expect(executeAction(appItem)).rejects.toBe('APP_NOT_FOUND')
        expect(invoke).toHaveBeenCalledWith('launch_app', {appId: 'app-1', path: 'C:\\gone\\app.exe'})
        expect(ElMessage).not.toHaveBeenCalled()
    })

    it('错误为含 APP_NOT_FOUND 子串的对象时同样 rethrow（稳健匹配）', async () => {
        invoke.mockRejectedValue(new Error('launch failed: APP_NOT_FOUND'))
        const {executeAction} = useLauncherSearch()
        await expect(executeAction(appItem)).rejects.toThrow(/APP_NOT_FOUND/)
        expect(ElMessage).not.toHaveBeenCalled()
    })

    it('其他错误保持既有行为：toast 后不抛', async () => {
        invoke.mockRejectedValue(new Error('boom'))
        const {executeAction} = useLauncherSearch()
        await expect(executeAction(appItem)).resolves.toBeUndefined()
        expect(ElMessage).toHaveBeenCalledTimes(1)
    })

    it('open_settings 失败仍 toast 不抛（无关路径行为不变）', async () => {
        invoke.mockRejectedValue('window error')
        const {executeAction} = useLauncherSearch()
        await expect(executeAction({action: 'open_settings'})).resolves.toBeUndefined()
        expect(ElMessage).toHaveBeenCalledTimes(1)
    })

    it('自定义命令 launch_app 的 APP_NOT_FOUND 保持既有 toast 行为，不 rethrow', async () => {
        invoke.mockRejectedValue('APP_NOT_FOUND')
        const {executeAction} = useLauncherSearch()
        await expect(executeAction(commandItem)).resolves.toBeUndefined()
        expect(ElMessage).toHaveBeenCalled()
    })
})
