import {ref} from 'vue'
import {ElMessageBox} from 'element-plus'
import {check} from '@tauri-apps/plugin-updater'
import {relaunch} from '@tauri-apps/plugin-process'

export function useUpdater(currentVersion) {
    const checkingUpdate = ref(false)
    const updateStatus = ref(null)
    const updateProgress = ref(0)
    const showUpdateProgress = ref(false)

    const checkUpdate = async () => {
        checkingUpdate.value = true
        updateStatus.value = {message: '正在检查更新...', type: 'info'}
        showUpdateProgress.value = false
        updateProgress.value = 0

        try {
            const update = await check()
            if (update) {
                updateStatus.value = null

                try {
                    await ElMessageBox.confirm(
                        `发现新版本 ${update.version}，是否立即更新？\n\n更新内容:\n${update.body || '暂无更新说明'}`,
                        '发现更新',
                        {
                            confirmButtonText: '立即更新',
                            cancelButtonText: '稍后提醒',
                            type: 'info',
                        }
                    )

                    showUpdateProgress.value = true
                    updateStatus.value = {message: '正在下载更新...', type: 'info'}

                    let contentLength = 0
                    let downloaded = 0

                    await update.downloadAndInstall((event) => {
                        if (event.event === 'Started') {
                            contentLength = event.data.contentLength || 0
                            downloaded = 0
                            updateProgress.value = 0
                        } else if (event.event === 'Progress') {
                            downloaded += event.data.chunkLength
                            if (contentLength > 0) {
                                updateProgress.value = Math.round((downloaded / contentLength) * 100)
                            }
                        } else if (event.event === 'Finished') {
                            updateProgress.value = 100
                        }
                    })

                    updateStatus.value = {message: '更新下载完成', type: 'success'}

                    await ElMessageBox.confirm(
                        '更新已下载完成，是否立即重启应用以应用更新？',
                        '更新完成',
                        {
                            confirmButtonText: '立即重启',
                            cancelButtonText: '稍后重启',
                            type: 'success',
                        }
                    )

                    await relaunch()

                } catch (action) {
                    if (action === 'cancel') {
                        updateStatus.value = {message: '已取消更新', type: 'info'}
                    }
                }
            } else {
                updateStatus.value = {message: '已是最新版本', type: 'success'}
            }
        } catch (error) {
            if (error !== 'cancel') {
                updateStatus.value = {message: '网络连接失败，请检查您的网络设置后重试', type: 'error'}
            }
        } finally {
            checkingUpdate.value = false
        }
    }

    return {
        checkingUpdate,
        updateStatus,
        updateProgress,
        showUpdateProgress,
        checkUpdate
    }
}
