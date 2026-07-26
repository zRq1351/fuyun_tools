use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

/// 热键更新结果
pub(crate) enum HotkeyUpdateResult {
    /// 无需更新（新旧值相同）
    NoChange,
    /// 更新成功
    Updated,
}

/// 热键注册回调类型
pub(crate) type RegisterFn = Box<dyn Fn(&AppHandle, &str) -> Result<(), String> + Send + Sync>;

/// 通用热键更新辅助函数
///
/// # 参数
/// * `app` - Tauri应用句柄
/// * `new_key` - 新的热键值
/// * `old_key` - 旧的热键值
/// * `is_enabled` - 功能是否启用
/// * `feature_name` - 功能名称（用于日志和错误消息）
/// * `register_fn` - 注册新热键的回调函数
/// * `conflict_keys` - 需要检查冲突的其他热键列表
///
/// # 返回
/// * `Ok(HotkeyUpdateResult::Updated)` - 更新成功
/// * `Ok(HotkeyUpdateResult::NoChange)` - 无需更新
/// * `Err(String)` - 更新失败
#[allow(dead_code)]
pub(crate) fn update_hotkey(
    app: &AppHandle,
    new_key: &str,
    old_key: &str,
    is_enabled: bool,
    feature_name: &str,
    register_fn: &RegisterFn,
    conflict_keys: &[&str],
) -> Result<HotkeyUpdateResult, String> {
    if new_key == old_key {
        return Ok(HotkeyUpdateResult::NoChange);
    }

    // 检查是否与其他热键冲突
    for conflict_key in conflict_keys {
        if new_key == *conflict_key {
            return Err(format!(
                "{}快捷键 '{}' 与其他快捷键冲突",
                feature_name, new_key
            ));
        }
    }

    // 检查是否已注册
    if app.global_shortcut().is_registered(new_key) {
        return Err(format!(
            "{}快捷键 '{}' 已被其他应用注册",
            feature_name, new_key
        ));
    }

    // 注销旧快捷键
    if let Err(e) = app.global_shortcut().unregister(old_key) {
        log::warn!(
            "注销旧{}快捷键 '{}' 失败 (可能从未注册成功): {}",
            feature_name,
            old_key,
            e
        );
    }

    // 注册新快捷键
    if is_enabled {
        if let Err(e) = register_fn(app, new_key) {
            log::warn!(
                "注册新{}快捷键 '{}' 失败, 尝试恢复旧快捷键: {}",
                feature_name,
                new_key,
                e
            );
            // 尝试恢复旧快捷键
            let _ = register_fn(app, old_key);
            return Err(e);
        }
    }

    Ok(HotkeyUpdateResult::Updated)
}

/// 通用热键启用/禁用辅助函数
#[allow(dead_code)]
pub(crate) fn toggle_hotkey(
    app: &AppHandle,
    enabled: bool,
    hot_key: &str,
    feature_name: &str,
    register_fn: &RegisterFn,
) -> Result<(), String> {
    if enabled {
        if !app.global_shortcut().is_registered(hot_key) {
            register_fn(app, hot_key)?;
        }
    } else if let Err(e) = app.global_shortcut().unregister(hot_key) {
        log::warn!(
            "注销{}快捷键 '{}' 失败: {}",
            feature_name,
            hot_key,
            e
        );
    }
    Ok(())
}
