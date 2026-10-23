package com.fuyun.clipboard.data

import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/** 一条同步内容（对应 PC 端 SyncMessage） */
sealed class SyncItem {
    abstract val id: Long

    data class Text(
        override val id: Long,
        val text: String,
    ) : SyncItem()

    data class Image(
        override val id: Long,
        val path: String,
        val width: Int,
        val height: Int,
    ) : SyncItem()
}

/** 配对结果 */
data class PairResult(
    val token: String,
    val pcName: String,
)

/** PC 状态 */
data class PcStatus(
    val pcName: String,
    val paired: Boolean,
    val protocolVersion: String,
)

/**
 * 与 PC 端同步服务的通信客户端。
 *
 * 协议（局域网 HTTP + WebSocket）：
 * - POST /api/pair        {code} → {token, pcName, protocolVersion}
 * - GET  /api/status      → {pcName, paired, protocolVersion}
 * - POST /api/clip        {kind:"text", text} | {kind:"image", imageBase64}
 * - GET  /api/history     ?since=<id> → [{id, kind:"text", text} | {id, kind:"image", path, width, height}]
 * - GET  /api/image       ?id=<id> → 图片二进制
 * - WS   /api/ws          ?token=<token> → 实时推送
 */
class SyncClient {

    private val http: OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(5, TimeUnit.SECONDS)
        .readTimeout(30, TimeUnit.SECONDS)
        .writeTimeout(60, TimeUnit.SECONDS)
        .pingInterval(25, TimeUnit.SECONDS)
        .build()

    private fun base(host: String, port: Int) = "http://$host:$port"

    // ---------- 配对与状态 ----------

    /** 用配对码换设备令牌。失败抛异常。 */
    fun pair(host: String, port: Int, code: String): PairResult {
        val body = JSONObject().put("code", code)
            .toString()
            .toRequestBody(JSON_MEDIA_TYPE)
        val request = Request.Builder()
            .url("${base(host, port)}/api/pair")
            .post(body)
            .build()
        http.newCall(request).execute().use { response ->
            val text = response.body?.string().orEmpty()
            if (response.code == 403) error("配对码无效或已过期")
            if (!response.isSuccessful) error("配对失败（${response.code}）")
            val json = JSONObject(text)
            return PairResult(
                token = json.getString("token"),
                pcName = json.optString("pcName", "我的电脑"),
            )
        }
    }

    /** 查询 PC 状态（免认证，配对前探测连接用） */
    fun status(host: String, port: Int): PcStatus {
        val request = Request.Builder()
            .url("${base(host, port)}/api/status")
            .get()
            .build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("无法连接电脑（${response.code}）")
            val json = JSONObject(response.body?.string().orEmpty())
            return PcStatus(
                pcName = json.optString("pcName", "我的电脑"),
                paired = json.optBoolean("paired", false),
                protocolVersion = json.optString("protocolVersion", "1"),
            )
        }
    }

    // ---------- 手机 → PC ----------

    /** 发送文本到 PC 剪贴板 */
    fun pushText(host: String, port: Int, token: String, text: String) {
        val body = JSONObject()
            .put("kind", "text")
            .put("text", text)
            .toString()
            .toRequestBody(JSON_MEDIA_TYPE)
        postAuthed(host, port, token, "/api/clip", body)
    }

    /** 发送图片（原始字节）到 PC 剪贴板 */
    fun pushImage(host: String, port: Int, token: String, imageBytes: ByteArray) {
        val base64 = android.util.Base64.encodeToString(imageBytes, android.util.Base64.NO_WRAP)
        val body = JSONObject()
            .put("kind", "image")
            .put("imageBase64", base64)
            .toString()
            .toRequestBody(JSON_MEDIA_TYPE)
        postAuthed(host, port, token, "/api/clip", body)
    }

    private fun postAuthed(
        host: String,
        port: Int,
        token: String,
        path: String,
        body: RequestBody,
    ) {
        val request = Request.Builder()
            .url("${base(host, port)}$path")
            .header("Authorization", "Bearer $token")
            .post(body)
            .build()
        http.newCall(request).execute().use { response ->
            if (response.code == 401) error("认证失败，请重新配对")
            if (!response.isSuccessful) error("发送失败（${response.code}）")
        }
    }

    // ---------- PC → 手机 ----------

    /** 拉取历史（since 之后的新条目，用于重连补漏） */
    fun history(host: String, port: Int, token: String, since: Long): List<SyncItem> {
        val request = Request.Builder()
            .url("${base(host, port)}/api/history?since=$since")
            .header("Authorization", "Bearer $token")
            .get()
            .build()
        http.newCall(request).execute().use { response ->
            if (response.code == 401) error("认证失败，请重新配对")
            if (!response.isSuccessful) error("拉取历史失败（${response.code}）")
            return parseHistory(response.body?.string().orEmpty())
        }
    }

    /** 下载图片二进制 */
    fun downloadImage(host: String, port: Int, token: String, id: Long): ByteArray {
        val request = Request.Builder()
            .url("${base(host, port)}/api/image?id=$id")
            .header("Authorization", "Bearer $token")
            .get()
            .build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) error("下载图片失败（${response.code}）")
            return response.body?.bytes() ?: error("图片内容为空")
        }
    }

    /** 建立 WS 实时推送连接 */
    fun newWebSocket(host: String, port: Int, token: String, listener: WebSocketListener): WebSocket {
        val request = Request.Builder()
            .url("ws://$host:$port/api/ws?token=$token")
            .build()
        return http.newWebSocket(request, listener)
    }

    // ---------- 解析 ----------

    private fun parseHistory(json: String): List<SyncItem> {
        val array = JSONArray(json)
        val items = ArrayList<SyncItem>(array.length())
        for (i in 0 until array.length()) {
            items.add(parseItem(array.getJSONObject(i)))
        }
        return items
    }

    private fun parseItem(json: JSONObject): SyncItem {
        val id = json.getLong("id")
        return when (json.getString("kind")) {
            "image" -> SyncItem.Image(
                id = id,
                path = json.optString("path", ""),
                width = json.optInt("width", 0),
                height = json.optInt("height", 0),
            )
            else -> SyncItem.Text(
                id = id,
                text = json.optString("text", ""),
            )
        }
    }

    /** 解析 WS 推送帧 */
    fun parsePushFrame(text: String): SyncItem = parseItem(JSONObject(text))

    private companion object {
        val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
    }
}
