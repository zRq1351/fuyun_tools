package com.fuyun.clipboard.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.content.FileProvider
import com.fuyun.clipboard.MainActivity
import com.fuyun.clipboard.R
import com.fuyun.clipboard.data.SyncClient
import com.fuyun.clipboard.data.SyncItem
import com.fuyun.clipboard.data.TokenStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener

/**
 * 同步前台服务：保持与 PC 的 WS 长连接，收到推送后写入剪贴板并通知。
 *
 * 连接状态与最近内容通过 [uiState] 暴露给 UI 观察。
 */
class SyncService : Service() {

    companion object {
        private const val TAG = "SyncService"
        private const val CHANNEL_SYNC = "sync_service"
        private const val CHANNEL_RECEIVED = "sync_received"
        private const val NOTIFICATION_ID = 1001
        private const val MAX_RECENT = 100

        /** UI 可观察的同步状态 */
        data class UiState(
            val connected: Boolean = false,
            val connecting: Boolean = false,
            val pcName: String? = null,
            val recent: List<SyncItem> = emptyList(),
            val lastError: String? = null,
        )

        private val _uiState = MutableStateFlow(UiState())
        val uiState: StateFlow<UiState> = _uiState.asStateFlow()

        fun start(context: Context) {
            val intent = Intent(context, SyncService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, SyncService::class.java))
        }
    }

    private lateinit var tokenStore: TokenStore
    private lateinit var client: SyncClient
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    private var webSocket: WebSocket? = null
    private var reconnectJob: Job? = null
    private var reconnectAttempt = 0
    private var lastItemId = 0L

    override fun onCreate() {
        super.onCreate()
        tokenStore = TokenStore(this)
        client = SyncClient()
        createChannels()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForegroundNotification()
        if (!tokenStore.isPaired) {
            Log.w(TAG, "未配对，停止服务")
            stopSelf()
            return START_NOT_STICKY
        }
        connect()
        return START_STICKY
    }

    // ---------- 连接管理 ----------

    private fun connect() {
        val host = tokenStore.host ?: return
        val port = tokenStore.port
        val token = tokenStore.token ?: return
        reconnectJob?.cancel()
        _uiState.value = _uiState.value.copy(
            connecting = true,
            pcName = tokenStore.pcName,
            lastError = null,
        )

        val listener = object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                Log.i(TAG, "WS 已连接")
                reconnectAttempt = 0
                _uiState.value = _uiState.value.copy(connected = true, connecting = false)
                backfill(host, port, token)
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                handleMessage(text)
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                Log.w(TAG, "WS 失败: ${t.message}")
                _uiState.value = _uiState.value.copy(
                    connected = false,
                    connecting = false,
                    lastError = t.message,
                )
                scheduleReconnect()
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                Log.i(TAG, "WS 关闭: $code $reason")
                _uiState.value = _uiState.value.copy(connected = false, connecting = false)
                if (code != 1000) scheduleReconnect()
            }
        }
        webSocket = client.newWebSocket(host, port, token, listener)
    }

    /** 指数退避重连（1s → 2s → 4s … 上限 30s） */
    private fun scheduleReconnect() {
        if (reconnectJob?.isActive == true) return
        reconnectAttempt++
        val delayMs = minOf(1_000L shl minOf(reconnectAttempt, 5), 30_000L)
        Log.i(TAG, "将在 ${delayMs}ms 后第 $reconnectAttempt 次重连")
        reconnectJob = scope.launch {
            delay(delayMs)
            connect()
        }
    }

    /** 重连后补拉历史（lastItemId 之后的新条目） */
    private fun backfill(host: String, port: Int, token: String) {
        scope.launch(Dispatchers.IO) {
            try {
                val items = client.history(host, port, token, lastItemId)
                for (item in items) handleItem(item, notify = false)
            } catch (e: Exception) {
                Log.w(TAG, "历史补拉失败: ${e.message}")
            }
        }
    }

    // ---------- 消息处理 ----------

    private fun handleMessage(text: String) {
        try {
            handleItem(client.parsePushFrame(text), notify = true)
        } catch (e: Exception) {
            Log.e(TAG, "解析推送帧失败", e)
        }
    }

    /** 处理一条内容：更新状态 → 写剪贴板 → 通知 */
    private fun handleItem(item: SyncItem, notify: Boolean) {
        if (item.id <= lastItemId) return
        lastItemId = item.id
        _uiState.value = _uiState.value.copy(recent = listOf(item) + _uiState.value.recent.take(MAX_RECENT - 1))
        when (item) {
            is SyncItem.Text -> writeTextToClipboard(item.text)
            is SyncItem.Image -> downloadAndWriteImage(item)
        }
        if (notify) notifyReceived(item)
    }

    private fun writeTextToClipboard(text: String) {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("fuyun", text))
        Log.i(TAG, "文本已写入剪贴板")
    }

    /** 下载图片 → 存缓存 → FileProvider URI → 剪贴板 */
    private fun downloadAndWriteImage(item: SyncItem.Image) {
        scope.launch(Dispatchers.IO) {
            try {
                val bytes = client.downloadImage(
                    tokenStore.host ?: return@launch,
                    tokenStore.port,
                    tokenStore.token ?: return@launch,
                    item.id,
                )
                val dir = File(cacheDir, "images").apply { mkdirs() }
                val file = File(dir, "sync_${item.id}.png")
                file.writeBytes(bytes)
                val uri = FileProvider.getUriForFile(
                    this@SyncService,
                    "$packageName.fileprovider",
                    file,
                )
                val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                cm.setPrimaryClip(ClipData.newUri(contentResolver, "image", uri))
                Log.i(TAG, "图片已写入剪贴板: ${file.absolutePath}")
            } catch (e: Exception) {
                Log.e(TAG, "图片下载/写入失败: ${e.message}", e)
            }
        }
    }

    // ---------- 通知 ----------

    private fun createChannels() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_SYNC,
                getString(R.string.sync_channel_name),
                NotificationManager.IMPORTANCE_LOW,
            ).apply { description = getString(R.string.sync_channel_desc) }
        )
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_RECEIVED,
                getString(R.string.received_channel_name),
                NotificationManager.IMPORTANCE_DEFAULT,
            ).apply { description = getString(R.string.received_channel_desc) }
        )
    }

    private fun startForegroundNotification() {
        val notification = buildSyncNotification()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun buildSyncNotification(): Notification {
        val intent = Intent(this, MainActivity::class.java)
        val pending = PendingIntent.getActivity(
            this,
            0,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CHANNEL_SYNC)
            .setContentTitle(getString(R.string.sync_service_notification_title))
            .setContentText(getString(R.string.sync_service_notification_text))
            .setSmallIcon(R.drawable.ic_stat_sync)
            .setOngoing(true)
            .setContentIntent(pending)
            .build()
    }

    private fun notifyReceived(item: SyncItem) {
        val manager = getSystemService(NotificationManager::class.java)
        val intent = Intent(this, MainActivity::class.java)
        val pending = PendingIntent.getActivity(
            this,
            1,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val (title, text) = when (item) {
            is SyncItem.Text ->
                getString(R.string.received_text_title) to item.text.take(60)
            is SyncItem.Image ->
                getString(R.string.received_image_title) to "${item.width}×${item.height}"
        }
        val notification = NotificationCompat.Builder(this, CHANNEL_RECEIVED)
            .setContentTitle(title)
            .setContentText(text)
            .setSmallIcon(R.drawable.ic_stat_sync)
            .setAutoCancel(true)
            .setContentIntent(pending)
            .build()
        manager.notify(NOTIFICATION_ID + 1, notification)
    }

    override fun onDestroy() {
        reconnectJob?.cancel()
        webSocket?.cancel()
        scope.cancel()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
