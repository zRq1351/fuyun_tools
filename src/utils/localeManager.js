import {createI18n} from 'vue-i18n'

const LOCALE_KEY = 'fuyun-locale'
const SUPPORTED_LOCALES = ['zh-CN', 'en-US']
const FALLBACK_LOCALE = 'zh-CN'

let i18nInstance = null

export function getLocale() {
    const saved = localStorage.getItem(LOCALE_KEY)
    if (SUPPORTED_LOCALES.includes(saved)) return saved
    const navLang = navigator.language || ''
    if (navLang.toLowerCase().startsWith('zh')) return 'zh-CN'
    if (navLang) return 'en-US'
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
    window.dispatchEvent(new CustomEvent('locale-change', {detail: {locale}}))
    try {
        window.dispatchEvent(new StorageEvent('storage', {
            key: LOCALE_KEY,
            newValue: locale,
            oldValue: null,
            storageArea: localStorage
        }))
    } catch (e) {
    }
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
