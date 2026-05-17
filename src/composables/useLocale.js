import {computed, onMounted, onUnmounted, ref} from 'vue'
import {getAvailableLocales, getLocale, setLocale, watchLocaleChange, watchLocaleStorage} from '@/utils/localeManager'
import zhCn from 'element-plus/dist/locale/zh-cn'
import en from 'element-plus/dist/locale/en'

const elementLocales = {
    'zh-CN': zhCn,
    'en-US': en
}

export function useLocale(options = {}) {
    const {syncStorage = true, onChange} = options

    const currentLocale = ref(getLocale())
    const locales = getAvailableLocales()

    const isZh = computed(() => currentLocale.value === 'zh-CN')

    const elLocale = computed(() => elementLocales[currentLocale.value] || zhCn)

    let cleanupFunctions = []

    function changeLocale(locale) {
        setLocale(locale)
        currentLocale.value = locale
        onChange?.(locale)
    }

    function handleChange(locale) {
        currentLocale.value = locale
        onChange?.(locale)
    }

    onMounted(() => {
        currentLocale.value = getLocale()
        cleanupFunctions.push(watchLocaleChange(handleChange))
        if (syncStorage) {
            cleanupFunctions.push(watchLocaleStorage(handleChange))
        }
    })

    onUnmounted(() => {
        cleanupFunctions.forEach(fn => fn())
        cleanupFunctions = []
    })

    return {
        currentLocale,
        isZh,
        elLocale,
        locales,
        changeLocale
    }
}
