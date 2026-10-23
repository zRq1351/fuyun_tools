package com.fuyun.clipboard.data

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.util.Log
import java.util.concurrent.Executor

/**
 * 通过 Android 原生 NSD（mDNS）发现局域网内的 PC 同步服务。
 *
 * PC 端广播服务类型 `_fuyun._tcp.`（见 Rust discovery.rs）。
 * API 34+ 使用 registerServiceInfoCallback（旧 resolveService 已废弃），
 * 注销时必须传同一 callback 实例，故用注册表持有引用。
 */
class Discovery(context: Context) {

    data class DiscoveredServer(
        val host: String,
        val port: Int,
        val name: String,
    )

    private val nsd: NsdManager =
        context.getSystemService(Context.NSD_SERVICE) as NsdManager

    private var discoveryListener: NsdManager.DiscoveryListener? = null
    /** serviceName → resolve callback（API 34+ 注销需同一实例） */
    private val resolveCallbacks = HashMap<String, NsdManager.ServiceInfoCallback>()
    private val mainExecutor: Executor = Executor { command -> command.run() }

    /** 开始发现；onServer 每发现一台 PC 回调一次 */
    fun start(onServer: (DiscoveredServer) -> Unit, onError: ((String) -> Unit)? = null) {
        stop()
        val listener = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String?) {
                Log.i(TAG, "NSD 发现已启动: $serviceType")
            }

            override fun onServiceFound(serviceInfo: NsdServiceInfo) {
                if (serviceInfo.serviceType != SERVICE_TYPE) return
                Log.i(TAG, "发现服务: ${serviceInfo.serviceName}")
                resolve(serviceInfo, onServer)
            }

            override fun onServiceLost(serviceInfo: NsdServiceInfo) {
                unregisterResolve(serviceInfo.serviceName)
            }

            override fun onDiscoveryStopped(serviceType: String?) {
                Log.i(TAG, "NSD 发现已停止")
            }

            override fun onStartDiscoveryFailed(serviceType: String?, errorCode: Int) {
                Log.e(TAG, "NSD 启动失败: $errorCode")
                onError?.invoke("服务发现启动失败（$errorCode）")
            }

            override fun onStopDiscoveryFailed(serviceType: String?, errorCode: Int) {
                Log.e(TAG, "NSD 停止失败: $errorCode")
            }

            override fun onResolutionFailed(serviceInfo: NsdServiceInfo?, errorCode: Int) {
                Log.e(TAG, "NSD 解析失败: $errorCode")
                serviceInfo?.let { unregisterResolve(it.serviceName) }
            }
        }
        discoveryListener = listener
        try {
            nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, listener)
        } catch (e: Exception) {
            Log.e(TAG, "NSD discoverServices 异常", e)
            onError?.invoke(e.message ?: "服务发现异常")
        }
    }

    private fun resolve(serviceInfo: NsdServiceInfo, onServer: (DiscoveredServer) -> Unit) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            resolveNew(serviceInfo, onServer)
        } else {
            resolveLegacy(serviceInfo, onServer)
        }
    }

    /** API 34+：新回调 API（注销需同一 callback 实例） */
    private fun resolveNew(serviceInfo: NsdServiceInfo, onServer: (DiscoveredServer) -> Unit) {
        val name = serviceInfo.serviceName
        val callback = object : NsdManager.ServiceInfoCallback {
            override fun onServiceInfoCallbackResolved(resolved: NsdServiceInfo) {
                unregisterResolve(name)
                val host = resolved.host?.hostAddress
                if (host.isNullOrBlank()) {
                    Log.w(TAG, "解析结果无地址: $name")
                    return
                }
                Log.i(TAG, "解析成功: $host:${resolved.port}")
                onServer(DiscoveredServer(host, resolved.port, resolved.serviceName))
            }

            override fun onServiceInfoCallbackRegistrationFailed(
                serviceInfo: NsdServiceInfo?,
                errorCode: Int,
            ) {
                Log.e(TAG, "resolve 注册失败: $errorCode")
                unregisterResolve(name)
            }

            override fun onServiceInfoCallbackUnregistered(serviceInfo: NsdServiceInfo?) = Unit
        }
        resolveCallbacks[name] = callback
        try {
            nsd.registerServiceInfoCallback(serviceInfo, mainExecutor, callback)
        } catch (e: Exception) {
            Log.e(TAG, "registerServiceInfoCallback 异常", e)
            unregisterResolve(name)
        }
    }

    /** API 26-33：旧同步解析 API（34 起废弃但功能等价） */
    @Suppress("DEPRECATION")
    private fun resolveLegacy(serviceInfo: NsdServiceInfo, onServer: (DiscoveredServer) -> Unit) {
        val name = serviceInfo.serviceName
        val listener = object : NsdManager.ResolveListener {
            override fun onResolveFailed(info: NsdServiceInfo?, errorCode: Int) {
                Log.e(TAG, "resolveService 失败: $errorCode")
            }

            override fun onServiceResolved(resolved: NsdServiceInfo) {
                val host = resolved.host?.hostAddress
                if (!host.isNullOrBlank()) {
                    Log.i(TAG, "解析成功: $host:${resolved.port}")
                    onServer(DiscoveredServer(host, resolved.port, resolved.serviceName))
                }
            }
        }
        try {
            nsd.resolveService(serviceInfo, listener)
        } catch (e: Exception) {
            Log.e(TAG, "resolveService 异常", e)
        }
    }

    private fun unregisterResolve(name: String) {
        val callback = resolveCallbacks.remove(name) ?: return
        try {
            nsd.unregisterServiceInfoCallback(callback)
        } catch (_: Exception) {
        }
    }

    fun stop() {
        discoveryListener?.let {
            try {
                nsd.stopServiceDiscovery(it)
            } catch (_: Exception) {
            }
        }
        discoveryListener = null
        resolveCallbacks.keys.toList().forEach { unregisterResolve(it) }
    }

    private companion object {
        const val TAG = "SyncDiscovery"
        const val SERVICE_TYPE = "_fuyun._tcp."
    }
}
