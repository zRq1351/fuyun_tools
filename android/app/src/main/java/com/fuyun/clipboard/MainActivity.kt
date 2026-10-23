package com.fuyun.clipboard

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import com.fuyun.clipboard.data.TokenStore
import com.fuyun.clipboard.ui.HomeScreen
import com.fuyun.clipboard.ui.PairingScreen
import com.fuyun.clipboard.ui.theme.FuyunClipboardTheme

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val tokenStore = TokenStore(applicationContext)

        setContent {
            FuyunClipboardTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background,
                ) {
                    // 配对状态驱动页面切换；用 mutableStateOf 让解除配对后回到配对页
                    val paired = mutableStateOf(tokenStore.isPaired)
                    if (paired.value) {
                        HomeScreen(
                            tokenStore = tokenStore,
                            onUnpaired = { paired.value = false },
                        )
                    } else {
                        PairingScreen(
                            tokenStore = tokenStore,
                            onPaired = { paired.value = true },
                        )
                    }
                }
            }
        }
    }
}
