import {describe, it, expect, vi, beforeEach} from 'vitest'

// 模拟浏览器环境
const storage = new Map()
globalThis.localStorage = {
    getItem: (k) => (storage.has(k) ? storage.get(k) : null),
    setItem: (k, v) => storage.set(k, String(v)),
    removeItem: (k) => storage.delete(k),
    clear: () => storage.clear(),
}

class FakeI18n {
    constructor(locale) {
        this.global = {locale: {value: locale}}
    }
}

let currentLang = 'zh-CN'
Object.defineProperty(globalThis.navigator, 'language', {
    get: () => currentLang,
    configurable: true,
})

// 模拟 document/window 事件
const langListeners = new Set()
globalThis.document = {
    documentElement: {
        setAttribute: vi.fn(),
    },
}
globalThis.window = {
    dispatchEvent: vi.fn(),
    addEventListener: (type, handler) => {
        if (type === 'locale-change') langListeners.add(handler)
    },
    removeEventListener: (type, handler) => {
        if (type === 'locale-change') langListeners.delete(handler)
    },
}

vi.mock('vue-i18n', () => ({
    createI18n: (options) => new FakeI18n(options.locale),
}))

import {
    getLocale,
    setLocale,
    createI18nInstance,
    getI18nInstance,
    watchLocaleChange,
    getSupportedLocales,
} from '../utils/localeManager.js'

describe('localeManager', () => {
    beforeEach(() => {
        storage.clear()
        langListeners.clear()
        currentLang = 'zh-CN'
        vi.clearAllMocks()
    })

    it('无保存记录时按浏览器语言选择', () => {
        expect(getLocale()).toBe('zh-CN')
        currentLang = 'en-US'
        expect(getLocale()).toBe('en-US')
    })

    it('不支持的浏览器语言回退默认 zh-CN', () => {
        currentLang = 'fr-FR'
        expect(getLocale()).toBe('zh-CN')
    })

    it('有保存记录时优先使用', () => {
        storage.set('fuyun-locale', 'en-US')
        currentLang = 'zh-CN'
        expect(getLocale()).toBe('en-US')
    })

    it('setLocale 保存并触发事件', () => {
        setLocale('en-US')
        expect(storage.get('fuyun-locale')).toBe('en-US')
        expect(window.dispatchEvent).toHaveBeenCalled()
        expect(document.documentElement.setAttribute).toHaveBeenCalledWith('lang', 'en-US')
    })

    it('setLocale 拒绝不支持的语言', () => {
        const warn = vi.spyOn(console, 'warn').mockImplementation(() => {
        })
        setLocale('fr-FR')
        expect(storage.has('fuyun-locale')).toBe(false)
        warn.mockRestore()
    })

    it('createI18nInstance 设置全局实例', () => {
        const inst = createI18nInstance({})
        expect(getI18nInstance()).toBe(inst)
        expect(inst.global.locale.value).toBe('zh-CN')
    })

    it('watchLocaleChange 注册并解绑', () => {
        const cb = vi.fn()
        const unsub = watchLocaleChange(cb)
        expect(langListeners.size).toBe(1)
        // 触发事件
        const handler = [...langListeners][0]
        handler({detail: {locale: 'en-US'}})
        expect(cb).toHaveBeenCalledWith('en-US')
        unsub()
        expect(langListeners.size).toBe(0)
    })

    it('getSupportedLocales 返回支持列表', () => {
        expect(getSupportedLocales()).toEqual(['zh-CN', 'en-US'])
    })
})
