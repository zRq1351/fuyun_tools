任务目标

- 在现有 Tauri v2 项目（repo: zRq1351/fuyun_tools）上实现：在设置页面前端显示更新下载进度条，下载完成后显示“重启并安装”按钮，用户点击后调用后端执行安装并重启。
- 主要改动文件：
    - src-tauri/src/ui/commands.rs — 后端：在检查/下载更新时通过事件推送进度，新增 install_update 命令供前端触发安装/重启。
    - src/settings.html — 前端：新增“重启并安装”按钮，监听后端事件 update-progress / update-downloaded / update-error，并更新
      UI。

前置条件

- 项目使用 Tauri v2（src-tauri/Cargo.toml 中 tauri = "2" 且 tauri-plugin-updater = "2" 已存在）。
- 项目已有前端 listen/invoke（本仓库已有 listen('result-update',...) 与 invoke(...) 的使用）。

修改具体内容（请逐文件替换/追加）

1) 后端：更新 src-tauri/src/ui/commands.rs（替换现有 check_for_updates 函数实现并新增 install_update 命令）

```rust name=src-tauri/src/ui/commands.rs
// 替换或更新文件中的 check_for_updates 实现为以下内容，并在同一文件添加 install_update 命令。
// 注意：此代码基于 tauri-plugin-updater v2 的常见 API；如本地编译报错请参考下方“替代实现”中的说明。

use tauri::{AppHandle, Manager};
use serde_json::json;

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<bool, String> {
    match app.updater().map_err(|e| e.to_string()) {
        Ok(updater) => match updater.check().await {
            Ok(update_option) => {
                if let Some(update) = update_option {
                    let should_update = app
                        .dialog()
                        .message(format!(
                            "发现新版本 {}，是否立即更新？\n\n更新内容:\n{}",
                            update.version,
                            update.body.as_ref().unwrap_or(&"".to_string())
                        ))
                        .title("发现更新")
                        .blocking_show();

                    if should_update {
                        let app_handle = app.clone();
                        let mut downloaded: u64 = 0;

                        update
                            .download_and_install(
                                move |chunk_length, content_length| {
                                    downloaded += chunk_length;
                                    let percent = content_length.map(|total| {
                                        ((downloaded as f64 / total as f64) * 100.0).min(100.0)
                                    });

                                    let payload = json!({
                                        "downloaded": downloaded,
                                        "total": content_length,
                                        "percent": percent
                                    });

                                    // 广播事件给所有窗口，前端监听 update-progress
                                    let _ = app_handle.emit_all("update-progress", payload);
                                },
                                || {
                                    // 下载结束
                                    let _ = app.emit_all("update-downloaded", json!({"message": "download_finished"}));
                                    println!("下载结束");
                                },
                            )
                            .await
                            .map_err(|e| {
                                let err_str = e.to_string();
                                let _ = app.emit_all("update-error", json!({"message": err_str.clone()}));
                                err_str
                            })?;
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                } else {
                    Ok(false)
                }
            }
            Err(e) => {
                let err = e.to_string();
                let _ = app.emit_all("update-error", json!({"message": err.clone()}));
                Err(err)
            }
        },
        Err(e) => Err(e.to_string()),
    }
}

/// 由前端在用户点击“重启并安装”时调用
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    // 尝试通过 Updater 执行安装并重启（不同版本 API 可能有差异）
    match app.updater().map_err(|e| e.to_string()) {
        Ok(updater) => {
            // 优先尝试常见的 install_and_restart 接口
            #[allow(unused_must_use)]
            {
                if let Err(e) = updater.install_and_restart().await {
                    let err = e.to_string();
                    let _ = app.emit_all("update-error", json!({ "message": err.clone() }));
                    return Err(err);
                }
            }
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}
```

2) 前端：更新 src/settings.html（在 update-progress 区添加按钮，并添加事件监听脚本）

A. 在 updateProgress 区的 action-buttons 中新增 restartUpdateBtn（找到 id="updateActionButtons" 区并替换内部）：

```html name=src/settings.html

<div class="action-buttons hidden" id="updateActionButtons">
    <button class="btn btn-secondary" id="closeUpdateBtn">
        <i class="fas fa-times btn-icon"></i> 关闭
    </button>
    <button class="btn btn-primary hidden" id="restartUpdateBtn">
        <i class="fas fa-redo btn-icon"></i> 重启并安装
    </button>
</div>
```

B. 在页面脚本中新增监听和按钮行为（将下面脚本插入到 settings.html 的现有脚本区，紧接 checkUpdateBtn 的逻辑之后或页面底部）：

```html name=src/settings.html

<script>
    // 监听后端推送的进度事件
    listen('update-progress', (event) => {
        const payload = event.payload || {};
        const percent = payload.percent ?? 0;
        const percentText = percent ? Math.round(percent) : 0;
        const statusText = document.getElementById('statusText');
        if (statusText) statusText.textContent = `下载中：${percentText}%`;

        const fill = document.querySelector('.progress-fill');
        if (fill) fill.style.width = `${percentText}%`;

        document.getElementById('updateActionButtons')?.classList.remove('hidden');
        document.getElementById('updateResult').style.display = 'none';
    });

    // 下载完成事件：显示“重启并安装”
    listen('update-downloaded', (event) => {
        document.getElementById('statusText').textContent = '下载完成，点击重启并安装以应用更新';
        const fill = document.querySelector('.progress-fill');
        if (fill) fill.style.width = '100%';
        document.getElementById('restartUpdateBtn')?.classList.remove('hidden');
        document.getElementById('updateActionButtons')?.classList.remove('hidden');
    });

    // 错误处理
    listen('update-error', (event) => {
        const payload = event.payload || {};
        const msg = payload.message || '更新过程中发生错误';
        window.showErrorToast?.(`更新出错: ${msg}`);
        showUpdateResult(`更新出错: ${msg}`, 'error');
        hideUpdateProgress();
    });

    // 点击重启并安装
    document.getElementById('restartUpdateBtn').addEventListener('click', async () => {
        const btn = document.getElementById('restartUpdateBtn');
        try {
            btn.disabled = true;
            btn.innerHTML = '<i class="fas fa-spinner fa-spin btn-icon"></i> 重启中...';
            await invoke('install_update'); // 调用后端命令
        } catch (err) {
            console.error('install_update 调用失败', err);
            window.showErrorToast?.('重启安装失败，请手动重启');
            btn.disabled = false;
            btn.innerHTML = '<i class="fas fa-redo btn-icon"></i> 重启并安装';
        }
    });

    // 重置进度 UI（确保匹配你页面里已有的 resetUpdateProgress）
    function resetUpdateProgress() {
        const fill = document.querySelector('.progress-fill');
        if (fill) fill.style.width = '0';
        document.getElementById('statusText').textContent = '正在检查更新...';
        document.getElementById('restartUpdateBtn').classList.add('hidden');
        document.getElementById('updateActionButtons').classList.add('hidden');
    }
</script>
```