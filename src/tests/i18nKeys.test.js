import {describe, it, expect} from 'vitest'
import zh from '../locales/zh-CN.json'
import en from '../locales/en-US.json'

// 展平嵌套 JSON：只保留叶子键路径（仓库 locale 值均为非空字符串）
const flatten = (obj, prefix = '') =>
    Object.entries(obj).flatMap(([key, value]) =>
        value && typeof value === 'object' && !Array.isArray(value)
            ? flatten(value, `${prefix}${key}.`)
            : [`${prefix}${key}`]
    )

const zhKeys = flatten(zh)
const enKeys = flatten(en)

const getPath = (obj, path) =>
    path.split('.').reduce((acc, key) => (acc == null ? acc : acc[key]), obj)

describe('i18n 双语键完整性', () => {
    it('zh-CN 与 en-US 键集合完全一致', () => {
        expect(enKeys.filter((k) => !zhKeys.includes(k))).toEqual([])
        expect(zhKeys.filter((k) => !enKeys.includes(k))).toEqual([])
    })

    it('所有键的取值均为非空字符串', () => {
        const badZh = zhKeys.filter((k) => typeof getPath(zh, k) !== 'string' || !getPath(zh, k))
        const badEn = enKeys.filter((k) => typeof getPath(en, k) !== 'string' || !getPath(en, k))
        expect(badZh).toEqual([])
        expect(badEn).toEqual([])
    })

    // F15：这 4 个 key 曾在两种语言下同时缺失，界面直接显示原始 key
    it('F15 关键 key 在两种语言下均存在', () => {
        const required = [
            'recordingToolbar.windowNoLongerAvailable',
            'recordingToolbar.mic',
            'documentManager.confirmDelete',
            'common.settings',
        ]
        for (const key of required) {
            expect(typeof getPath(zh, key), `${key} in zh-CN`).toBe('string')
            expect(typeof getPath(en, key), `${key} in en-US`).toBe('string')
        }
    })

    // F2：删除分类/命令/应用的确认文案
    it('F2 删除确认 key 在两种语言下均存在', () => {
        const required = [
            'launcher.deleteCategoryConfirm',
            'launcher.deleteCommandConfirm',
            'launcher.removeAppConfirm',
            'common.confirmDelete',
            'common.ok',
            'common.cancel',
        ]
        for (const key of required) {
            expect(typeof getPath(zh, key), `${key} in zh-CN`).toBe('string')
            expect(typeof getPath(en, key), `${key} in en-US`).toBe('string')
        }
    })

    it('F2 分类删除文案包含「归属将一并移除」提示', () => {
        expect(getPath(zh, 'launcher.deleteCategoryConfirm')).toContain('归属')
        expect(getPath(en, 'launcher.deleteCategoryConfirm')).toContain('assignments')
    })
})
