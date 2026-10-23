package com.fuyun.clipboard.data

import android.content.Context
import android.content.SharedPreferences

/**
 * 本地保存配对信息（服务器地址 + 令牌）。
 * 令牌持久化后重启 App 无需重新配对。
 */
class TokenStore(context: Context) {

    private val prefs: SharedPreferences =
        context.getSharedPreferences("fuyun_sync", Context.MODE_PRIVATE)

    var host: String?
        get() = prefs.getString(KEY_HOST, null)
        set(value) = prefs.edit().putString(KEY_HOST, value).apply()

    var port: Int
        get() = prefs.getInt(KEY_PORT, 0)
        set(value) = prefs.edit().putInt(KEY_PORT, value).apply()

    var token: String?
        get() = prefs.getString(KEY_TOKEN, null)
        set(value) = prefs.edit().putString(KEY_TOKEN, value).apply()

    var pcName: String?
        get() = prefs.getString(KEY_PC_NAME, null)
        set(value) = prefs.edit().putString(KEY_PC_NAME, value).apply()

    val isPaired: Boolean
        get() = !token.isNullOrBlank() && !host.isNullOrBlank() && port > 0

    fun save(host: String, port: Int, token: String, pcName: String) {
        prefs.edit()
            .putString(KEY_HOST, host)
            .putInt(KEY_PORT, port)
            .putString(KEY_TOKEN, token)
            .putString(KEY_PC_NAME, pcName)
            .apply()
    }

    fun clear() {
        prefs.edit().clear().apply()
    }

    private companion object {
        const val KEY_HOST = "host"
        const val KEY_PORT = "port"
        const val KEY_TOKEN = "token"
        const val KEY_PC_NAME = "pc_name"
    }
}
