package com.fuyun.clipboard.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Phone
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import com.fuyun.clipboard.data.Discovery
import com.fuyun.clipboard.data.SyncClient
import com.fuyun.clipboard.data.TokenStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * 配对页：自动发现电脑（mDNS）或手动输入 IP → 输入配对码 → 完成配对。
 */
@Composable
fun PairingScreen(
    tokenStore: TokenStore,
    onPaired: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snackbarHostState = remember { SnackbarHostState() }
    val client = remember { SyncClient() }

    var ipInput by remember { mutableStateOf(TextFieldValue("")) }
    var codeInput by remember { mutableStateOf("") }
    var discovered by remember { mutableStateOf<List<Discovery.DiscoveredServer>>(emptyList()) }
    var pairing by remember { mutableStateOf(false) }
    var selectedHost by remember { mutableStateOf<String?>(null) }
    var selectedPort by remember { mutableStateOf(0) }

    // mDNS 自动发现（离开页面时停止）
    DisposableEffect(Unit) {
        val discovery = Discovery(context)
        discovery.start(
            onServer = { server ->
                if (discovered.none { it.host == server.host && it.port == server.port }) {
                    discovered = discovered + server
                }
            },
        )
        onDispose { discovery.stop() }
    }

    Scaffold(
        topBar = { TopAppBar(title = { Text("浮云剪贴板") }) },
        snackbarHost = { SnackbarHost(snackbarHostState) },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text("连接电脑", style = MaterialTheme.typography.headlineSmall)
            Text(
                "确保手机与电脑连接同一 Wi-Fi，并在电脑端「设置 → 手机同步」中启动同步服务。",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            // 发现的电脑
            if (discovered.isNotEmpty()) {
                Text("发现的电脑", style = MaterialTheme.typography.titleSmall)
                discovered.forEach { server ->
                    Card(
                        onClick = {
                            selectedHost = server.host
                            selectedPort = server.port
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Row(
                            modifier = Modifier.padding(16.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            Icon(
                                Icons.Filled.Phone,
                                contentDescription = null,
                            )
                            Column(modifier = Modifier.weight(1f)) {
                                Text(server.name, style = MaterialTheme.typography.bodyLarge)
                                Text(
                                    "${server.host}:${server.port}",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                            if (selectedHost == server.host) {
                                Text(
                                    "已选择",
                                    color = MaterialTheme.colorScheme.primary,
                                    style = MaterialTheme.typography.labelMedium,
                                )
                            }
                        }
                    }
                }
            }

            // 手动输入地址
            OutlinedTextField(
                value = ipInput,
                onValueChange = { ipInput = it },
                label = { Text("电脑地址（IP:端口）") },
                placeholder = { Text("192.168.1.100:45817") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                modifier = Modifier.fillMaxWidth(),
            )

            // 配对码
            OutlinedTextField(
                value = codeInput,
                onValueChange = { input ->
                    codeInput = input.filter { it.isDigit() }.take(6)
                },
                label = { Text("配对码") },
                placeholder = { Text("6 位数字") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
                modifier = Modifier.fillMaxWidth(),
            )

            Button(
                onClick = {
                    val target = selectedHost?.let { "$it:$selectedPort" } ?: ipInput.text.trim()
                    val parts = target.split(":")
                    if (parts.size != 2 || parts[1].toIntOrNull() == null) {
                        scope.launch { snackbarHostState.showSnackbar("地址格式应为 IP:端口") }
                        return@Button
                    }
                    if (codeInput.length != 6) {
                        scope.launch { snackbarHostState.showSnackbar("请输入 6 位配对码") }
                        return@Button
                    }
                    pairing = true
                    scope.launch {
                        try {
                            val result = withContext(Dispatchers.IO) {
                                client.pair(parts[0], parts[1].toInt(), codeInput)
                            }
                            tokenStore.save(
                                parts[0],
                                parts[1].toInt(),
                                result.token,
                                result.pcName,
                            )
                            onPaired()
                        } catch (e: Exception) {
                            snackbarHostState.showSnackbar(e.message ?: "配对失败")
                        } finally {
                            pairing = false
                        }
                    }
                },
                enabled = !pairing,
                modifier = Modifier.fillMaxWidth(),
            ) {
                if (pairing) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(20.dp),
                        strokeWidth = 2.dp,
                    )
                } else {
                    Text("配对")
                }
            }
        }
    }
}
