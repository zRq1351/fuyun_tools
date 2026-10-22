import {computed, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {
    Camera,
    Cpu,
    DocumentCopy,
    Folder,
    FolderOpened,
    InfoFilled,
    Operation,
    Setting,
    VideoCamera,
    WarningFilled
} from '@element-plus/icons-vue'

/**
 * useSettingsNav — 设置中心导航域
 * 负责：左侧导航当前页（activeTab）、菜单配置（sections，开发者面板项按 isDevMode 条件追加）、
 * 当前分区信息（currentSection）与 Tab 切换/定位（handleNavigateTab）。
 * activeTab 还会被快捷键冲突提示、跨页跳转事件（navigate-settings-tab）与
 * 保存失败时的热键回退逻辑直接赋值，故以 ref 形式返回，由页面层继续使用。
 */
export function useSettingsNav() {
    const {t} = useI18n()

    const activeTab = ref('clipboard')
    const isDevMode = __DEV_PANEL__

    const sections = computed(() => {
        const baseSections = [
            {
                key: 'clipboard',
                label: t('settings.nav.clipboard'),
                description: t('settings.nav.clipboardDesc'),
                icon: DocumentCopy
            },
            {
                key: 'screenshot',
                label: t('settings.nav.screenshot'),
                description: t('settings.nav.screenshotDesc'),
                icon: Camera
            },
            {
                key: 'recording',
                label: t('settings.nav.recording'),
                description: t('settings.nav.recordingDesc'),
                icon: VideoCamera
            },
            {
                key: 'selection',
                label: t('settings.nav.selection'),
                description: t('settings.nav.selectionDesc'),
                icon: Setting
            },
            {
                key: 'launcher',
                label: t('settings.nav.launcher'),
                description: t('settings.nav.launcherDesc'),
                icon: Operation
            },
            {
                key: 'doc_manager',
                label: t('settings.nav.docManager'),
                description: t('settings.nav.docManagerDesc'),
                icon: Folder
            },
            {
                key: 'ai',
                label: t('settings.nav.ai'),
                description: t('settings.nav.aiDesc'),
                icon: Cpu
            },
            {
                key: 'backup',
                label: t('settings.nav.backup'),
                description: t('settings.nav.backupDesc'),
                icon: FolderOpened
            },
            {
                key: 'diagnostic',
                label: t('settings.nav.diagnostic'),
                description: t('settings.nav.diagnosticDesc'),
                icon: WarningFilled
            }
        ]
        if (isDevMode) {
            baseSections.push({
                key: 'developer',
                label: t('settings.nav.developer'),
                description: t('settings.nav.developerDesc'),
                icon: Setting
            })
        }
        baseSections.push({
            key: 'about',
            label: t('settings.nav.about'),
            description: t('settings.nav.aboutDesc'),
            icon: InfoFilled
        })
        return baseSections
    })

    const currentSection = computed(() => sections.value.find((section) => section.key === activeTab.value) || sections.value[0])

    const handleNavigateTab = (tab) => {
        if (typeof tab === 'string' && sections.value.some((section) => section.key === tab)) {
            activeTab.value = tab
        }
    }

    return {
        activeTab,
        isDevMode,
        sections,
        currentSection,
        handleNavigateTab
    }
}
