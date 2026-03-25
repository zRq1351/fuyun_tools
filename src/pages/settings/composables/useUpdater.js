import {ref} from 'vue'
import {ElMessageBox} from 'element-plus'
import {check} from '@tauri-apps/plugin-updater'
import {relaunch} from '@tauri-apps/plugin-process'
import {marked} from 'marked'

// 配置marked选项
marked.setOptions({
    breaks: false, // 不将单个换行符转换为<br>
    gfm: true, // 启用GitHub风格的Markdown
})

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
                    const bodyHtml = update.body ? marked(update.body) : '<p>暂无更新说明</p>'
                    const messageHtml = `
                        <div class="update-dialog">
                            <div class="update-dialog__hero">
                                <div class="update-dialog__tag">新版本可用</div>
                                <h3 class="update-dialog__title">v${update.version}</h3>
                            </div>
                            <div class="update-body-content">
                                ${bodyHtml}
                            </div>
                            <p class="update-dialog__hint">是否立即更新？</p>
                        </div>
                    `
                    
                    await ElMessageBox.confirm(
                        messageHtml,
                        '发现更新',
                        {
                            confirmButtonText: '立即更新',
                            cancelButtonText: '稍后提醒',
                            type: 'primary',
                            dangerouslyUseHTMLString: true,
                            customClass: 'update-message-box'
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
