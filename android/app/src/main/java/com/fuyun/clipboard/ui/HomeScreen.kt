package com.fuyun.clipboard.ui

import android.Manifest
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Send
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import com.fuyun.clipboard.R
import com.fuyun.clipboard.data.SyncClient
import com.fuyun.clipboard.data.SyncItem
import com.fuyun.clipboard.data.TokenStore
import com.fuyun.clipboard.service.SyncService
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * 主页：发送内容到电脑（文本/图片）+ 查看收到的内容。
 */
@Composable
fun HomeScreen(
    tokenStore: TokenStore,
    onUnpaired: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snackbarHostState = remember { SnackbarHostState() }
    val clipboardManager = LocalClipboardManager.current
    val client = remember { SyncClient() }
    val uiState by SyncService.uiState.collectAsState()

    var textInput by remember { mutableStateOf(TextFieldValue("")) }
    var sending by remember { mutableStateOf(false) }

    // 确保前台服务在跑（配对成功后进入本页即启动）
    LaunchedEffect(Unit) {
        SyncService.start(context)
    }

    // 通知权限（API 33+）
    val notificationPermission = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { /* 拒绝也能用，只是没有新内容通知 */ }
    LaunchedEffect(Unit) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            val granted = ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.POST_NOTIFICATIONS,
            ) == PackageManager.PERMISSION_GRANTED
            if (!granted) {
                notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
        }
    }

    // 图片选择
    val imagePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.PickVisualMedia(),
    ) { uri: Uri? ->
        if (uri != null) {
            sending = true
            scope.launch {
                try {
                    val bytes = withContext(Dispatchers.IO) {
                        context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                    } ?: error("无法读取所选图片")
                    withContext(Dispatchers.IO) {
                        client.pushImage(
                            tokenStore.host!!,
                            tokenStore.port,
                            tokenStore.token!!,
                            bytes,
                        )
                    }
                    snackbarHostState.showSnackbar("图片已发送到电脑")
                } catch (e: Exception) {
                    snackbarHostState.showSnackbar(e.message ?: "图片发送失败")
                } finally {
                    sending = false
                }
            }
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("浮云剪贴板") },
                actions = {
                    OutlinedButton(
                        onClick = {
                            SyncService.stop(context)
                            tokenStore.clear()
                            onUnpaired()
                        },
                        modifier = Modifier.padding(end = 12.dp),
                    ) { Text("断开") }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
    ) { padding ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            // 连接状态
            item {
                Card(modifier = Modifier.fillMaxWidth()) {
                    Row(
                        modifier = Modifier.padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        if (uiState.connected) {
                            Text(
                                "已连接：${uiState.pcName ?: "电脑"}",
                                color = MaterialTheme.colorScheme.primary,
                            )
                        } else {
                            CircularProgressIndicator(
                                modifier = Modifier.size(16.dp),
                                strokeWidth = 2.dp,
                            )
                            Text(
                                uiState.lastError?.let { "连接中断：$it" } ?: "正在连接…",
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }

            // 发送到电脑
            item {
                Card(modifier = Modifier.fillMaxWidth()) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        Text("发送到电脑", style = MaterialTheme.typography.titleMedium)
                        OutlinedTextField(
                            value = textInput,
                            onValueChange = { textInput = it },
                            label = { Text("输入文本") },
                            modifier = Modifier.fillMaxWidth(),
                            minLines = 2,
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Button(
                                onClick = {
                                    val text = textInput.text
                                    if (text.isBlank()) {
                                        scope.launch { snackbarHostState.showSnackbar("请输入内容") }
                                        return@Button
                                    }
                                    sending = true
                                    scope.launch {
                                        try {
                                            withContext(Dispatchers.IO) {
                                                client.pushText(
                                                    tokenStore.host!!,
                                                    tokenStore.port,
                                                    tokenStore.token!!,
                                                    text,
                                                )
                                            }
                                            textInput = TextFieldValue("")
                                            snackbarHostState.showSnackbar("已发送到电脑剪贴板")
                                        } catch (e: Exception) {
                                            snackbarHostState.showSnackbar(e.message ?: "发送失败")
                                        } finally {
                                            sending = false
                                        }
                                    }
                                },
                                enabled = !sending,
                            ) {
                                Icon(Icons.Filled.Send, contentDescription = null)
                                Text(" 发送文本 ")
                            }
                            FilledTonalButton(
                                onClick = {
                                    imagePicker.launch(
                                        PickVisualMediaRequest(
                                            ActivityResultContracts.PickVisualMedia.ImageOnly,
                                        ),
                                    )
                                },
                                enabled = !sending,
                            ) {
                                Icon(painterResource(R.drawable.ic_image), contentDescription = null)
                                Text(" 发送图片 ")
                            }
                        }
                    }
                }
            }

            // 收到的内容
            item {
                Text(
                    "收到的内容",
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.padding(top = 8.dp),
                )
            }

            if (uiState.recent.isEmpty()) {
                item {
                    Text(
                        "电脑复制的内容会实时出现在这里",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }

            items(uiState.recent, key = { it.id }) { item ->
                Card(modifier = Modifier.fillMaxWidth()) {
                    Row(
                        modifier = Modifier.padding(start = 16.dp, top = 12.dp, bottom = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            when (item) {
                                is SyncItem.Text -> Text(
                                    item.text,
                                    style = MaterialTheme.typography.bodyMedium,
                                    maxLines = 3,
                                )
                                is SyncItem.Image -> Text(
                                    "图片 ${item.width}×${item.height}",
                                    style = MaterialTheme.typography.bodyMedium,
                                )
                            }
                        }
                        IconButton(
                            onClick = {
                                when (item) {
                                    is SyncItem.Text ->
                                        clipboardManager.setText(AnnotatedString(item.text))
                                    is SyncItem.Image -> { /* 已自动写入剪贴板 */ }
                                }
                                scope.launch { snackbarHostState.showSnackbar("已复制") }
                            },
                        ) {
                            Icon(painterResource(R.drawable.ic_copy), contentDescription = "复制")
                        }
                    }
                }
            }
        }
    }
}
