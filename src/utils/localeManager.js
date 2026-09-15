import {createI18n} from 'vue-i18n'
import {invoke} from '@tauri-apps/api/core'

const LOCALE_KEY = 'fuyun-locale'
const SUPPORTED_LOCALES = ['zh-CN', 'en-US']
const FALLBACK_LOCALE = 'zh-CN'

let i18nInstance = null
let pendingLocaleSave = false
let localeSaveSeq = 0

export function getLocale() {
    const saved = localStorage.getItem(LOCALE_KEY)
    if (SUPPORTED_LOCALES.includes(saved)) return saved
    const navLang = navigator.language || ''
    if (navLang.toLowerCase().startsWith('zh')) return 'zh-CN'
    if (navLang.toLowerCase().startsWith('en')) return 'en-US'
    return FALLBACK_LOCALE
}

export function setLocale(locale) {
    if (!SUPPORTED_LOCALES.includes(locale)) {
        console.warn(`[LocaleManager] Invalid locale: ${locale}`)
        return
    }
    localStorage.setItem(LOCALE_KEY, locale)
    if (i18nInstance && i18nInstance.global) {
        i18nInstance.global.locale.value = locale
    }
    document.documentElement.setAttribute('lang', locale)
    const seq = ++localeSaveSeq
    pendingLocaleSave = true
    invoke('set_locale', {locale}).then(() => {
        if (seq === localeSaveSeq) pendingLocaleSave = false
    }).catch(err => {
        console.warn('[LocaleManager] 保存语言到后端失败:', err)
        if (seq === localeSaveSeq) pendingLocaleSave = false
    })
    window.dispatchEvent(new CustomEvent('locale-change', {detail: {locale}}))
}

/** 从后端同步语言；本地有在途保存时不覆盖，并返回本地值 */
export async function fetchLocale() {
    try {
        const backendLocale = await invoke('get_locale')
        if (SUPPORTED_LOCALES.includes(backendLocale)) {
            if (pendingLocaleSave) {
                return getLocale()
            }
            localStorage.setItem(LOCALE_KEY, backendLocale)
            return backendLocale || getLocale()
        }
    } catch (e) {
        console.debug('[LocaleManager] 从后端获取语言失败，使用缓存:', e?.message || e)
    }
    return getLocale()
}

export function createI18nInstance(messages) {
    i18nInstance = createI18n({
        legacy: false,
        locale: getLocale(),
        fallbackLocale: FALLBACK_LOCALE,
        messages,
        silentTranslationWarn: false,
        missingWarn: false,
        fallbackWarn: false
    })
    document.documentElement.setAttribute('lang', getLocale())
    // 异步与后端对齐（备份恢复/重装后可恢复语言）
    fetchLocale().then(backendLocale => {
        if (backendLocale && backendLocale !== getLocale()) {
            if (i18nInstance && i18nInstance.global) {
                i18nInstance.global.locale.value = backendLocale
            }
            localStorage.setItem(LOCALE_KEY, backendLocale)
            document.documentElement.setAttribute('lang', backendLocale)
            window.dispatchEvent(new CustomEvent('locale-change', {detail: {locale: backendLocale}}))
        }
    }).catch(() => {})
    return i18nInstance
}

export function getI18nInstance() {
    return i18nInstance
}

export function watchLocaleChange(callback) {
    const handler = (e) => {
        callback?.(e.detail.locale)
    }
    window.addEventListener('locale-change', handler)
    return () => window.removeEventListener('locale-change', handler)
}

export function watchLocaleStorage(callback) {
    const handler = (e) => {
        if (e.key === LOCALE_KEY) {
            const locale = e.newValue || getLocale()
            if (i18nInstance && i18nInstance.global) {
                i18nInstance.global.locale.value = locale
            }
            document.documentElement.setAttribute('lang', locale)
            callback?.(locale)
        }
    }
    window.addEventListener('storage', handler)
    return () => window.removeEventListener('storage', handler)
}

export function getAvailableLocales() {
    return [
        {value: 'zh-CN', label: '中文'},
        {value: 'en-US', label: 'English'}
    ]
}

export function getSupportedLocales() {
    return SUPPORTED_LOCALES
}
