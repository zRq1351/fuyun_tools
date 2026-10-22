import {computed, onBeforeUnmount, ref, watch} from 'vue'
import {ElLoading, ElMessage, ElMessageBox} from 'element-plus'
import {useI18n} from 'vue-i18n'
import {listen} from '@tauri-apps/api/event'
import {AISettingsService, RecordingService} from '../../../services/ipc.js'

/**
 * useSettingsPersistence — 设置持久化域
 * 负责：450ms 防抖自动保存（queuePersistSettings/persistSettings）、
 * 脏值追踪（buildFormSnapshot/getChangedFields/saveInitialFormState 快照 diff）、
 * 保存状态提示（autoSaveState/autoSaveText）、表单深度监听与卸载前冲刷。
 * 功能开关联动（featureLabels/revertFeatureToggle/handleFeatureToggle）同在本域：
 * 开关的回退与重建排队直接读写防抖队列内部状态（saveTimer/pendingPersistSnapshot/
 * suppressNextAutoSave）并更新保存基线（initialFormState），与持久化引擎共用同一闭包，
 * 单独拆文件需改写这些内部状态访问，故按原样合并在本 composable 内。
 * 跨域依赖由页面层注入：form（设置表单）、aiSettingsRef（AI 子组件实例）、
 * shortcutConflictMessage（热键冲突提示）、activeTab（导航当前页）、
 * isDark（主题）、showVcRuntimeMissingWarning（VC 运行时缺失提示）。
 */
export function useSettingsPersistence({
    form,
    aiSettingsRef,
    shortcutConflictMessage,
    activeTab,
    isDark,
    showVcRuntimeMissingWarning
}) {
    const {t} = useI18n()

    let saveTimer = null
    let autoSaveStateResetTimer = null
    let pendingPersistSnapshot = null
    let pendingPersistVersion = 0
    let formMutationVersion = 0
    const isInitializing = ref(true)
    const isAutoSaving = ref(false)
    const suppressNextAutoSave = ref(false)
    const autoSaveState = ref('idle')
    // 设置加载失败时禁止自动保存，避免把表单默认值写回后端
    const settingsLoadFailed = ref(false)
    // 保存初始状态用于差异比较
    const initialFormState = ref(null)
    // 阻止初始化后的第一次 watch 触发
    const skipNextWatch = ref(false)

    const autoSaveText = computed(() => {
        const states = {
            idle: t('settings.autoSave.idle'),
            pending: t('settings.autoSave.pending'),
            saving: t('settings.autoSave.saving'),
            saved: t('settings.autoSave.saved'),
            error: t('settings.autoSave.error')
        }
        return states[autoSaveState.value] || t('settings.autoSave.idle')
    })

    const buildFormSnapshot = () => ({
        textMaxItems: form.textMaxItems,
        imageMaxItems: form.imageMaxItems,
        imageDiskLimitMb: form.imageDiskLimitMb,
        textClipboardEnabled: form.textClipboardEnabled,
        imageClipboardEnabled: form.imageClipboardEnabled,
        screenshotEnabled: form.screenshotEnabled,
        recordingEnabled: form.recordingEnabled,
        groupedItemsProtectedFromLimit: form.groupedItemsProtectedFromLimit,
        toggleShortcut: form.toggleShortcut,
        imageToggleShortcut: form.imageToggleShortcut,
        screenshotToggleShortcut: form.screenshotToggleShortcut,
        recordingToggleShortcut: form.recordingToggleShortcut,
        recordingMicToggleShortcut: form.recordingMicToggleShortcut,
        recordingDefaultFps: form.recordingDefaultFps,
        recordingDefaultVideoBitrateKbps: form.recordingDefaultVideoBitrateKbps,
        recordingDefaultAudioBitrateKbps: form.recordingDefaultAudioBitrateKbps,
        recordingCaptureCursor: form.recordingCaptureCursor,
        recordingCaptureSystemAudio: form.recordingCaptureSystemAudio,
        recordingCaptureMicrophone: form.recordingCaptureMicrophone,
        recordingMicrophoneDeviceId: form.recordingMicrophoneDeviceId,
        recordingOutputDir: form.recordingOutputDir,
        recordingAutoOpenFolder: form.recordingAutoOpenFolder,
        recordingToolbarContentProtected: form.recordingToolbarContentProtected,
        recordingMaxDurationMinutes: form.recordingMaxDurationMinutes,
        recordingFileNameTemplate: form.recordingFileNameTemplate,
        recordingWindowAudioSyncAdvanceMs: form.recordingWindowAudioSyncAdvanceMs,
      devForceFfmpegWindowCapture: form.devForceFfmpegWindowCapture,
        aiProvider: form.aiProvider,
        apiUrl: form.apiUrl,
        modelName: form.modelName,
        apiKey: form.apiKey,
        selectionEnabled: form.selectionEnabled,
        selectionModifierKey: form.selectionModifierKey,
        selectionCustomPrompts: form.selectionCustomPrompts,
        selectionWebSearchEnabled: form.selectionWebSearchEnabled,
        selectionWebSearchEngine: form.selectionWebSearchEngine,
        translationPromptTemplate: form.translationPromptTemplate,
        explanationPromptTemplate: form.explanationPromptTemplate,
      imageFillVerifyMode: form.imageFillVerifyMode,
      ocrEngine: form.ocrEngine,
      launcherEnabled: form.launcherEnabled,
      launcherHotKey: form.launcherHotKey,
      docManagerEnabled: form.docManagerEnabled,
      docManagerHotKey: form.docManagerHotKey,
      docManagerWidgetEnabled: form.docManagerWidgetEnabled,
      loggingEnabled: form.loggingEnabled
    })

    // 保存初始状态快照
    const saveInitialFormState = (snapshot = buildFormSnapshot()) => {
        // 使用深拷贝，避免响应式对象的引用问题
        initialFormState.value = JSON.parse(JSON.stringify(snapshot))
    }

    // 获取变化的字段
    const getChangedFields = (snapshot = buildFormSnapshot()) => {
        if (!initialFormState.value) {
            return null
        }

        const changedFields = {}
        const initial = initialFormState.value
        const source = snapshot


        if (source.textMaxItems !== initial.textMaxItems) {
            changedFields.textMaxItems = source.textMaxItems
        }
        if (source.imageMaxItems !== initial.imageMaxItems) {
            changedFields.imageMaxItems = source.imageMaxItems
        }
        if (source.imageDiskLimitMb !== initial.imageDiskLimitMb) {
            changedFields.imageDiskLimitMb = source.imageDiskLimitMb
        }
        if (source.textClipboardEnabled !== initial.textClipboardEnabled) {
            changedFields.textClipboardEnabled = source.textClipboardEnabled
        }
        if (source.imageClipboardEnabled !== initial.imageClipboardEnabled) {
            changedFields.imageClipboardEnabled = source.imageClipboardEnabled
        }
        if (source.screenshotEnabled !== initial.screenshotEnabled) {
            changedFields.screenshotEnabled = source.screenshotEnabled
        }
        if (source.recordingEnabled !== initial.recordingEnabled) {
            changedFields.recordingEnabled = source.recordingEnabled
        }
        if (source.groupedItemsProtectedFromLimit !== initial.groupedItemsProtectedFromLimit) {
            changedFields.groupedItemsProtectedFromLimit = source.groupedItemsProtectedFromLimit
        }
        if (source.toggleShortcut !== initial.toggleShortcut) {
            changedFields.hotKey = source.toggleShortcut
        }
        if (source.imageToggleShortcut !== initial.imageToggleShortcut) {
            changedFields.imageHotKey = source.imageToggleShortcut
        }
        if (source.screenshotToggleShortcut !== initial.screenshotToggleShortcut) {
            changedFields.screenshotHotKey = source.screenshotToggleShortcut
        }
        if (source.recordingToggleShortcut !== initial.recordingToggleShortcut) {
            changedFields.recordingHotKey = source.recordingToggleShortcut
        }
        if (source.recordingMicToggleShortcut !== initial.recordingMicToggleShortcut) {
            changedFields.recordingMicToggleHotKey = source.recordingMicToggleShortcut
        }
        if (source.recordingDefaultFps !== initial.recordingDefaultFps) {
            changedFields.recordingDefaultFps = source.recordingDefaultFps
        }
        if (source.recordingDefaultVideoBitrateKbps !== initial.recordingDefaultVideoBitrateKbps) {
            changedFields.recordingDefaultVideoBitrateKbps = source.recordingDefaultVideoBitrateKbps
        }
        if (source.recordingDefaultAudioBitrateKbps !== initial.recordingDefaultAudioBitrateKbps) {
            changedFields.recordingDefaultAudioBitrateKbps = source.recordingDefaultAudioBitrateKbps
        }
        if (source.recordingCaptureCursor !== initial.recordingCaptureCursor) {
            changedFields.recordingCaptureCursor = source.recordingCaptureCursor
        }
        if (source.recordingCaptureSystemAudio !== initial.recordingCaptureSystemAudio) {
            changedFields.recordingCaptureSystemAudio = source.recordingCaptureSystemAudio
        }
        if (source.recordingCaptureMicrophone !== initial.recordingCaptureMicrophone) {
            changedFields.recordingCaptureMicrophone = source.recordingCaptureMicrophone
        }
        if (source.recordingMicrophoneDeviceId !== initial.recordingMicrophoneDeviceId) {
            changedFields.recordingMicrophoneDeviceId = source.recordingMicrophoneDeviceId
        }
        if (source.recordingOutputDir !== initial.recordingOutputDir) {
            changedFields.recordingOutputDir = source.recordingOutputDir
        }
        if (source.recordingAutoOpenFolder !== initial.recordingAutoOpenFolder) {
            changedFields.recordingAutoOpenFolder = source.recordingAutoOpenFolder
        }
        if (source.recordingToolbarContentProtected !== initial.recordingToolbarContentProtected) {
            changedFields.recordingToolbarContentProtected = source.recordingToolbarContentProtected
        }
        if (source.recordingMaxDurationMinutes !== initial.recordingMaxDurationMinutes) {
            changedFields.recordingMaxDurationMinutes = source.recordingMaxDurationMinutes
        }
        if (source.recordingFileNameTemplate !== initial.recordingFileNameTemplate) {
            changedFields.recordingFileNameTemplate = source.recordingFileNameTemplate
        }
        if (source.recordingWindowAudioSyncAdvanceMs !== initial.recordingWindowAudioSyncAdvanceMs) {
            changedFields.recordingWindowAudioSyncAdvanceMs = source.recordingWindowAudioSyncAdvanceMs
        }
        if (source.devForceFfmpegWindowCapture !== initial.devForceFfmpegWindowCapture) {
            changedFields.devForceFfmpegWindowCapture = source.devForceFfmpegWindowCapture
        }


        if (source.aiProvider !== initial.aiProvider) {
            changedFields.aiProvider = source.aiProvider
        }

        if (source.apiUrl !== initial.apiUrl) {
            changedFields.aiApiUrl = source.apiUrl
        }
        if (source.modelName !== initial.modelName) {
            changedFields.aiModelName = source.modelName
        }
        if (source.apiKey !== initial.apiKey) {
            changedFields.aiApiKey = source.apiKey
        }
        if (source.selectionEnabled !== initial.selectionEnabled) {
            changedFields.selectionEnabled = source.selectionEnabled
        }
        if (source.selectionModifierKey !== initial.selectionModifierKey) {
            changedFields.selectionModifierKey = source.selectionModifierKey
        }
        if (JSON.stringify(source.selectionCustomPrompts) !== JSON.stringify(initial.selectionCustomPrompts)) {
            changedFields.selectionCustomPrompts = source.selectionCustomPrompts
        }
        if (source.selectionWebSearchEnabled !== initial.selectionWebSearchEnabled) {
            changedFields.selectionWebSearchEnabled = source.selectionWebSearchEnabled
        }
        if (source.selectionWebSearchEngine !== initial.selectionWebSearchEngine) {
            changedFields.selectionWebSearchEngine = source.selectionWebSearchEngine
        }
        if (source.translationPromptTemplate !== initial.translationPromptTemplate) {
            changedFields.translationPromptTemplate = source.translationPromptTemplate
        }
        if (source.explanationPromptTemplate !== initial.explanationPromptTemplate) {
            changedFields.explanationPromptTemplate = source.explanationPromptTemplate
        }
        if (source.imageFillVerifyMode !== initial.imageFillVerifyMode) {
            changedFields.imageFillVerifyMode = source.imageFillVerifyMode
        }
        if (source.ocrEngine !== initial.ocrEngine) {
            changedFields.ocrEngine = source.ocrEngine
        }
        if (source.launcherEnabled !== initial.launcherEnabled) {
            changedFields.launcherEnabled = source.launcherEnabled
        }
        if (source.launcherHotKey !== initial.launcherHotKey) {
            changedFields.launcherHotKey = source.launcherHotKey
        }
        if (source.docManagerEnabled !== initial.docManagerEnabled) {
            changedFields.docManagerEnabled = source.docManagerEnabled
        }
        if (source.docManagerHotKey !== initial.docManagerHotKey) {
            changedFields.docManagerHotKey = source.docManagerHotKey
        }
        if (source.docManagerWidgetEnabled !== initial.docManagerWidgetEnabled) {
            changedFields.docManagerWidgetEnabled = source.docManagerWidgetEnabled
        }
        if (source.loggingEnabled !== initial.loggingEnabled) {
            changedFields.loggingEnabled = source.loggingEnabled
        }

        return Object.keys(changedFields).length > 0 ? changedFields : null
    }

    const queuePersistSettings = (silent = true, delay = 450) => {
        pendingPersistSnapshot = buildFormSnapshot()
        pendingPersistVersion = ++formMutationVersion
        if (saveTimer) {
            clearTimeout(saveTimer)
        }
        saveTimer = window.setTimeout(() => {
            const snapshot = pendingPersistSnapshot
            const version = pendingPersistVersion
            pendingPersistSnapshot = null
            pendingPersistVersion = 0
            saveTimer = null
            void persistSettings(silent, snapshot, version)
        }, delay)
    }

    const persistSettings = async (
        silent = false,
        snapshot = buildFormSnapshot(),
        persistVersion = ++formMutationVersion
    ) => {
        if (isInitializing.value) {
            return
        }
        if (settingsLoadFailed.value) {
            autoSaveState.value = 'error'
            return
        }
        if (isAutoSaving.value) {
            // 合并快照：不要用 null 覆盖已有 pending，也不要丢掉新变更
            if (snapshot) {
                pendingPersistSnapshot = snapshot
            }
            pendingPersistVersion = Math.max(pendingPersistVersion, persistVersion)
            return
        }
        if (!snapshot) {
            // 防抖定时器与 in-flight finally 竞态：无快照时跳过
            return
        }

        const changedFields = getChangedFields(snapshot)


        if (!changedFields) {
            if (!pendingPersistSnapshot && getChangedFields() === null) {
                autoSaveState.value = 'idle'
            }
            return
        }

        try {
            isAutoSaving.value = true
            if (autoSaveStateResetTimer) {
                clearTimeout(autoSaveStateResetTimer)
                autoSaveStateResetTimer = null
            }
            autoSaveState.value = 'saving'

            if (changedFields.screenshotEnabled === true) {
                const runtimeStatus = await AISettingsService.checkVcRuntimeDependencies()
                const missing = Array.isArray(runtimeStatus?.missing) ? runtimeStatus.missing : []
                if (missing.length > 0) {

                    suppressNextAutoSave.value = true
                    form.screenshotEnabled = false
                    snapshot.screenshotEnabled = false
                    changedFields.screenshotEnabled = false
                    await showVcRuntimeMissingWarning(runtimeStatus)
                }
            }

            if (changedFields.recordingEnabled === true) {
                const ffmpegStatus = await RecordingService.checkFfmpeg()
                if (!ffmpegStatus?.exists) {

                    suppressNextAutoSave.value = true
                    form.recordingEnabled = false
                    snapshot.recordingEnabled = false
                    changedFields.recordingEnabled = false
                    try {
                        await ElMessageBox.confirm(
                            t('settings.recording.ffmpegNotFound', {path: ffmpegStatus.ffmpegPath}),
                            t('settings.recording.needDownloadFfmpeg'),
                            {
                                confirmButtonText: t('settings.recording.confirmDownload'),
                                cancelButtonText: t('common.cancel'),
                                type: 'warning',
                                closeOnClickModal: false,
                                closeOnPressEscape: true
                            }
                        )
                    } catch {
                        autoSaveState.value = 'idle'
                        ElMessage.info(t('settings.recording.cancelEnableRecording'))
                        return
                    }
                    const downloading = ElLoading.service({
                        lock: true,
                        text: t('settings.recording.downloadingFfmpeg', {percent: '0'}),
                        background: 'rgba(0, 0, 0, 0.35)'
                    })
                    let unlistenProgress = null
                    try {
                        unlistenProgress = await listen('recording-ffmpeg-download-progress', (event) => {
                            const payload = event?.payload || {}
                            const percent = typeof payload.progressPercent === 'number' ? payload.progressPercent : null
                            if (percent !== null) {
                                downloading.setText(t('settings.recording.downloadingFfmpeg', {percent: `${percent}`}))
                                return
                            }
                            const downloaded = Number(payload.downloadedBytes || 0)
                            const total = Number(payload.totalBytes || 0)
                            if (total > 0) {
                                const computed = Math.min(100, Math.floor(downloaded * 100 / total))
                                downloading.setText(t('settings.recording.downloadingFfmpeg', {percent: `${computed}`}))
                            }
                        })
                        await RecordingService.downloadFfmpeg(ffmpegStatus.downloadUrl || null)
                    } finally {
                        if (unlistenProgress) {
                            unlistenProgress()
                        }
                        downloading.close()
                    }
                    suppressNextAutoSave.value = true
                    form.recordingEnabled = true
                    snapshot.recordingEnabled = true
                    changedFields.recordingEnabled = true
                    ElMessage.success(t('settings.recording.ffmpegDownloaded'))
                }
            }


            if (changedFields.recordingEnabled === false) {
                // 禁用录屏会由后端取消并删除当前录制，若正在录制需先确认
                let recordingActive = false
                try {
                    const rs = await RecordingService.getState()
                    recordingActive = rs && (rs.state === 'recording' || rs.state === 'paused' || rs.state === 'starting')
                } catch (_e) {
                }
                if (recordingActive) {
                    suppressNextAutoSave.value = true
                    try {
                        await ElMessageBox.confirm(
                            t('settings.recording.confirmDisableWhileRecording'),
                            t('settings.recording.disabledHint'),
                            {
                                confirmButtonText: t('common.confirm'),
                                cancelButtonText: t('common.cancel'),
                                type: 'warning',
                                closeOnClickModal: false,
                                closeOnPressEscape: true
                            }
                        )
                    } catch {
                        // 取消禁用：还原开关，但**继续保存同批次其余字段**
                        form.recordingEnabled = true
                        snapshot.recordingEnabled = true
                        delete changedFields.recordingEnabled
                        if (!changedFields || Object.keys(changedFields).length === 0) {
                            autoSaveState.value = 'idle'
                            return
                        }
                    }
                }
            }

            await AISettingsService.saveSettings(changedFields)


            if (changedFields.aiProvider) {
                if (aiSettingsRef.value) {
                    await aiSettingsRef.value.loadAiProviders()
                }
            }

            shortcutConflictMessage.value = ''
            if (!silent) {
                ElMessage.success(t('settings.autoSaved'))
            }
            autoSaveState.value = 'saved'


            saveInitialFormState(snapshot)

            autoSaveStateResetTimer = window.setTimeout(() => {
                if (autoSaveState.value === 'saved') {
                    autoSaveState.value = 'idle'
                }
                autoSaveStateResetTimer = null
            }, 1500)
        } catch (error) {
            let raw
            let errorCode = null
            let errorParams
              // 优先使用 ipcInvoke 包装的 originalError，保留后端 E_* JSON 格式
              if (error && typeof error === 'object' && error.originalError !== undefined) {
                raw = typeof error.originalError === 'string'
                  ? error.originalError
                  : String(error.originalError || '')
              } else if (typeof error === 'object' && error !== null) {
                raw = error.message || JSON.stringify(error)
              } else {
                raw = String(error || '')
              }
              try {
                const parsed = JSON.parse(raw)
                if (parsed && typeof parsed.code === 'string' && parsed.code.startsWith('E_')) {
                    errorCode = parsed.code
                    errorParams = parsed.params || null
                    const i18nKey = `errorCodes.${errorCode}`
                    const translated = t(i18nKey, errorParams || {})
                    raw = translated === i18nKey ? (parsed.message || raw) : translated
                } else if (parsed && parsed.message) {
                    raw = parsed.message
                }
              } catch (e) {}

            if (errorCode && (errorCode.includes('HOTKEY_CONFLICT') || errorCode.includes('HOTKEY_REGISTER'))) {
                shortcutConflictMessage.value = raw
                if (errorCode.includes('RECORDING') || raw.includes('麦克风')) {
                    activeTab.value = 'recording'
                } else if (errorCode.includes('SCREENSHOT') || raw.includes('截图')) {
                    activeTab.value = 'screenshot'
                } else {
                    activeTab.value = 'clipboard'
                }
                // 回滚失败的热键字段，避免 dirty 永久循环
                const init = initialFormState.value
                if (init) {
                    form.toggleShortcut = init.toggleShortcut
                    form.imageToggleShortcut = init.imageToggleShortcut
                    form.screenshotToggleShortcut = init.screenshotToggleShortcut
                    form.recordingToggleShortcut = init.recordingToggleShortcut
                    form.recordingMicToggleShortcut = init.recordingMicToggleShortcut
                    form.launcherHotKey = init.launcherHotKey
                    form.docManagerHotKey = init.docManagerHotKey
                    saveInitialFormState()
                }
            } else if (!errorCode && (raw.includes('快捷键被占用') || raw.includes('shortcut') || raw.includes('hotkey'))) {
                shortcutConflictMessage.value = raw.replace(/^Error:\s*/i, '')
                if (raw.includes('录屏') || raw.includes('麦克风')) {
                    activeTab.value = 'recording'
                } else if (raw.includes('截图')) {
                    activeTab.value = 'screenshot'
                } else {
                    activeTab.value = 'clipboard'
                }
              }
            if (errorCode) {
                ElMessage.error(raw)
            } else {
                ElMessage.error(t('settings.saveFailed', {error: raw}))
            }
              autoSaveState.value = 'error'
            } finally {
            isAutoSaving.value = false
            if (pendingPersistSnapshot && pendingPersistVersion > persistVersion) {
                const retrySnapshot = pendingPersistSnapshot
                const retryVersion = pendingPersistVersion
                pendingPersistSnapshot = null
                pendingPersistVersion = 0
                window.setTimeout(() => {
                    void persistSettings(true, retrySnapshot, retryVersion)
                }, 0)
            }
        }
    }

    const featureLabels = computed(() => ({
        textClipboardEnabled: t('settings.featureLabels.textClipboardEnabled'),
        imageClipboardEnabled: t('settings.featureLabels.imageClipboardEnabled'),
        screenshotEnabled: t('settings.featureLabels.screenshotEnabled'),
        recordingEnabled: t('settings.featureLabels.recordingEnabled'),
        selectionEnabled: t('settings.featureLabels.selectionEnabled'),
        launcherEnabled: t('settings.featureLabels.launcherEnabled'),
        docManagerEnabled: t('settings.featureLabels.docManagerEnabled'),
    }))

    // 开关保存失败/取消时回退表单值：仅在值真正变化时设置抑制标志，
    // 否则没有 watch 事件消费 suppressNextAutoSave，会吞掉用户的下一次修改
    const revertFeatureToggle = (fieldName, targetValue) => {
        if (form[fieldName] === targetValue) return
        suppressNextAutoSave.value = true
        form[fieldName] = targetValue
        if (saveTimer || pendingPersistSnapshot) {
            // 已排队的快照可能记录了开关的错误值，按当前表单重建
            queuePersistSettings(true, 450)
        }
    }

    const handleFeatureToggle = async (fieldName, newValue) => {
        const label = featureLabels.value[fieldName] || fieldName
        const action = newValue ? t('common.enable') : t('common.disable')
        const actionVerb = newValue ? t('common.enabling') : t('common.disabling')
        const loading = ElLoading.service({
            lock: true,
            text: t('settings.enablingFeature', {action: actionVerb, feature: label}),
            background: isDark.value ? 'rgba(0, 0, 0, 0.55)' : 'rgba(255, 255, 255, 0.55)'
        })
        const minUntil = Date.now() + 1000
        const payload = {}
        payload[fieldName] = newValue
        try {
            if (fieldName === 'screenshotEnabled' && newValue) {
                const runtimeStatus = await AISettingsService.checkVcRuntimeDependencies()
                const missing = Array.isArray(runtimeStatus?.missing) ? runtimeStatus.missing : []
                if (missing.length > 0) {
                    loading.close()
                    await showVcRuntimeMissingWarning(runtimeStatus)
                    revertFeatureToggle(fieldName, !newValue)
                    return false
                }
            }
            if (fieldName === 'recordingEnabled' && newValue) {
                const ffmpegStatus = await RecordingService.checkFfmpeg()
                if (!ffmpegStatus?.exists) {
                    loading.close()
                    try {
                        await ElMessageBox.confirm(
                            t('settings.recording.ffmpegNotFound', {path: ffmpegStatus.ffmpegPath}),
                            t('settings.recording.needDownloadFfmpeg'),
                            {
                                confirmButtonText: t('settings.recording.confirmDownload'),
                                cancelButtonText: t('common.cancel'),
                                type: 'warning',
                                closeOnClickModal: false,
                                closeOnPressEscape: true
                            }
                        )
                    } catch {
                        ElMessage.info(t('settings.recording.cancelEnableRecording'))
                        revertFeatureToggle(fieldName, !newValue)
                        return false
                    }
                    const dl = ElLoading.service({
                        lock: true,
                        text: t('settings.recording.downloadingFfmpeg', {percent: '0'}),
                        background: isDark.value ? 'rgba(0, 0, 0, 0.55)' : 'rgba(255, 255, 255, 0.55)'
                    })
                    let unlistenProgress = null
                    try {
                        unlistenProgress = await listen('recording-ffmpeg-download-progress', (event) => {
                            const p = event?.payload || {}
                            const percent = typeof p.progressPercent === 'number' ? p.progressPercent : null
                            if (percent !== null) {
                                dl.setText(t('settings.recording.downloadingFfmpeg', {percent: `${percent}`}));
                                return
                            }
                            const downloaded = Number(p.downloadedBytes || 0)
                            const total = Number(p.totalBytes || 0)
                            if (total > 0) dl.setText(t('settings.recording.downloadingFfmpeg', {percent: `${Math.min(100, Math.floor(downloaded * 100 / total))}`}))
                        })
                        await RecordingService.downloadFfmpeg(ffmpegStatus.downloadUrl || null)
                    } finally {
                        if (unlistenProgress) unlistenProgress()
                        dl.close()
                    }
                    ElMessage.success(t('settings.recording.ffmpegDownloaded'))
                }
            }
            loading.setText(t('settings.enablingFeature', {action: actionVerb, feature: label}))
            await AISettingsService.saveSettings(payload)
            // 只把已持久化的开关字段并入基线。全量 buildFormSnapshot() 重置会把
            // 尚未保存的其他字段一并标为基线（修改被吞），且与已排队的陈旧快照
            // diff 后会把开关旧值写回后端
            if (initialFormState.value) {
                initialFormState.value = {...initialFormState.value, [fieldName]: newValue}
            }
            if (form[fieldName] !== newValue) {
                suppressNextAutoSave.value = true
                form[fieldName] = newValue
            }
            if (saveTimer || pendingPersistSnapshot) {
                // 重建 pending 快照（含最新表单值）并重排防抖定时器
                queuePersistSettings(true, 450)
            }
            return true
        } catch (error) {
            ElMessage.error(t('common.operationFailed', {error: String(error)}))
            revertFeatureToggle(fieldName, !newValue)
            return false
        } finally {
            const remaining = minUntil - Date.now()
            if (remaining > 0) {
                await new Promise(resolve => setTimeout(resolve, remaining))
            }
            loading.close()
        }
    }

    watch(form, () => {
        if (isInitializing.value) return
        if (skipNextWatch.value) {
            skipNextWatch.value = false
            return
        }
        if (suppressNextAutoSave.value) {
            suppressNextAutoSave.value = false
            return
        }
        if (!isAutoSaving.value) {
            if (autoSaveStateResetTimer) {
                clearTimeout(autoSaveStateResetTimer)
                autoSaveStateResetTimer = null
            }
            autoSaveState.value = 'pending'
        }
        if (saveTimer) {
            clearTimeout(saveTimer)
        }
        queuePersistSettings(true, 450)
    }, {deep: true})

    onBeforeUnmount(() => {
        // 窗口关闭前冲刷未保存更改：
        // - 空闲：立刻 persist
        // - 在途：把最新表单写入 pending，供 in-flight finally 的重试带上
        if (isAutoSaving.value) {
            pendingPersistSnapshot = buildFormSnapshot()
            pendingPersistVersion = formMutationVersion
        } else if (pendingPersistSnapshot) {
            const snapshot = pendingPersistSnapshot
            pendingPersistSnapshot = null
            pendingPersistVersion = 0
            void persistSettings(true, snapshot, formMutationVersion)
        }
        if (saveTimer) {
            clearTimeout(saveTimer)
            saveTimer = null
        }
        if (autoSaveStateResetTimer) {
            clearTimeout(autoSaveStateResetTimer)
            autoSaveStateResetTimer = null
        }
    })

    return {
        autoSaveState,
        autoSaveText,
        isInitializing,
        settingsLoadFailed,
        skipNextWatch,
        saveInitialFormState,
        handleFeatureToggle
    }
}
