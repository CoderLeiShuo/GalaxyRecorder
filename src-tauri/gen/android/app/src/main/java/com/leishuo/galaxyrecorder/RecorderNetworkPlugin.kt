package com.leishuo.galaxyrecorder

import android.app.Activity
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.LinkProperties
import android.net.wifi.WifiManager
import android.util.Log
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import androidx.appcompat.app.AppCompatActivity
import org.json.JSONArray
import org.json.JSONObject
import java.net.Inet4Address

// Explicit QR Wi-Fi requests can be local-only while cellular remains the default.
object RecorderWifiSelection { @Volatile var requested: Network? = null }

@InvokeArg
class PrepareNetworkArgs { var multicast: Boolean = false }

@TauriPlugin
class RecorderNetworkPlugin(private val activity: Activity) : Plugin(activity) {
    private val connectivity = activity.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
    private val wifi = activity.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
    @Volatile private var selectedNetwork: Network? = null
    private var multicastLock: WifiManager.MulticastLock? = null
    private var callbackRegistered = false
    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onLost(network: Network) {
            if (RecorderWifiSelection.requested == network) RecorderWifiSelection.requested = null
            synchronized(this@RecorderNetworkPlugin) {
                if (selectedNetwork == network) {
                    if (connectivity.boundNetworkForProcess == network) connectivity.bindProcessToNetwork(null)
                    selectedNetwork = null
                    releaseMulticast()
                    Log.i("GalaxyNetwork", "selected Wi-Fi disconnected")
                    emit("lost", network, "车机 Wi-Fi 已断开，请重新连接记录仪")
                }
            }
        }
        override fun onLinkPropertiesChanged(network: Network, properties: LinkProperties) {
            if (selectedNetwork == network && properties.linkAddresses.none { RecorderAddresses.local(it.address) }) onLost(network)
        }
    }

    private fun emit(kind: String, network: Network? = selectedNetwork, reason: String? = null) {
        trigger("network", JSObject().apply {
            put("kind", kind); put("networkId", network?.networkHandle?.toString() ?: JSONObject.NULL)
            if (reason != null) put("reason", reason)
        })
    }
    override fun onPause(activity: AppCompatActivity) { emit("pause") }
    override fun onResume(activity: AppCompatActivity) { emit("resume") }

    private fun link(network: Network): JSObject? {
        val capabilities = connectivity.getNetworkCapabilities(network) ?: return null
        if (!capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) return null
        val properties = connectivity.getLinkProperties(network) ?: return null
        val address = properties.linkAddresses.firstOrNull { RecorderAddresses.local(it.address) } ?: return null
        val gateway = properties.routes.firstOrNull { it.isDefaultRoute && it.gateway is Inet4Address }?.gateway?.hostAddress
        return JSObject().apply {
            put("id", network.networkHandle.toString())
            put("interfaceName", properties.interfaceName ?: "wifi")
            put("ip", address.address.hostAddress)
            put("prefixLength", address.prefixLength)
            put("gateway", gateway ?: JSONObject.NULL)
            val identity=try{
                val info=if(android.os.Build.VERSION.SDK_INT>=29)capabilities.transportInfo as? android.net.wifi.WifiInfo else wifi.connectionInfo
                val bssid=info?.bssid
                if(bssid!=null && bssid!="02:00:00:00:00:00" && bssid!="00:00:00:00:00:00")java.security.MessageDigest.getInstance("SHA-256").digest(bssid.lowercase().toByteArray()).joinToString(""){"%02x".format(it)}else null
            }catch(_:Exception){null}
            put("fingerprint",identity?:JSONObject.NULL)
        }
    }

    @Suppress("DEPRECATION")
    private fun candidates(): List<Pair<Network, JSObject>> = connectivity.allNetworks.mapNotNull { network -> link(network)?.let { network to it } }

    @Suppress("DEPRECATION")
    private fun currentWifiIp(): String? = try {
        val value=wifi.connectionInfo.ipAddress
        if(value==0)null else (0..3).joinToString(".") { ((value ushr (it*8)) and 255).toString() }
    } catch (_:Exception) { null }

    private fun defaultTransport(): String {
        val cap = connectivity.activeNetwork?.let { connectivity.getNetworkCapabilities(it) } ?: return "none"
        return when {
            cap.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "vpn"
            cap.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
            cap.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
            else -> "other"
        }
    }

    private fun info(): JSObject {
        val links = candidates()
        val active = connectivity.activeNetwork
        val currentIp=currentWifiIp()
        val chosen = links.firstOrNull { it.first == RecorderWifiSelection.requested }
            ?: links.firstOrNull { it.first == active }
            ?: links.firstOrNull { currentIp!=null && it.second.optString("ip")==currentIp }
            ?: links.firstOrNull { it.first == selectedNetwork }
            ?: links.firstOrNull { it.first == connectivity.boundNetworkForProcess }
            ?: links.firstOrNull()
        return JSObject().apply {
            put("platform", "android")
            put("available", chosen != null)
            put("selected", chosen?.second ?: JSONObject.NULL)
            put("networks", JSONArray().apply { links.forEach { put(it.second) } })
            put("bound", chosen != null && connectivity.boundNetworkForProcess == chosen.first)
            put("defaultTransport", defaultTransport())
            put("message", if (chosen == null) "未连接可用 Wi-Fi，请先在系统中连接车机热点" else "已读取当前 Wi-Fi，可自动查找记录仪")
        }
    }

    private fun releaseMulticast() {
        multicastLock?.let { if (it.isHeld) it.release() }
        multicastLock = null
    }

    @Command
    fun inspect(invoke: Invoke) {
        try { invoke.resolve(info()) } catch (error: Exception) { invoke.reject("读取 Wi-Fi 网络失败：${error.message}") }
    }

    @Command
    fun prepare(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(PrepareNetworkArgs::class.java)
            val links = candidates()
            val active = connectivity.activeNetwork
            val currentIp=currentWifiIp()
            val chosen = links.firstOrNull { it.first == RecorderWifiSelection.requested }
                ?: links.firstOrNull { it.first == active }
                ?: links.firstOrNull { currentIp!=null && it.second.optString("ip")==currentIp }
                ?: links.firstOrNull { it.first == selectedNetwork }
                ?: links.firstOrNull { it.first == connectivity.boundNetworkForProcess }
                ?: links.firstOrNull()
                ?: throw IllegalStateException("未连接可用 Wi-Fi，请先连接车机热点")
            check(connectivity.bindProcessToNetwork(chosen.first)) { "手机未允许应用使用当前 Wi-Fi" }
            selectedNetwork = chosen.first
            if (!callbackRegistered) {
                val request = NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI).build()
                connectivity.registerNetworkCallback(request, callback)
                callbackRegistered = true
            }
            if (args.multicast && multicastLock?.isHeld != true) {
                multicastLock = wifi.createMulticastLock("galaxy-recorder-discovery").apply { setReferenceCounted(false); acquire() }
            }
            val result = info()
            Log.i("GalaxyNetwork", "Wi-Fi prepared; bound=${result.optBoolean("bound")}; interface=${chosen.second.optString("interfaceName")}; prefix=${chosen.second.optInt("prefixLength")}")
            invoke.resolve(result)
        } catch (error: Exception) { invoke.reject("无法准备记录仪 Wi-Fi：${error.message}") }
    }

    @Command
    fun endDiscovery(invoke: Invoke) { releaseMulticast(); invoke.resolve(JSObject()) }

    @Command
    fun release(invoke: Invoke) {
        try {
            releaseMulticast()
            connectivity.bindProcessToNetwork(null)
            selectedNetwork = null
            if (callbackRegistered) { connectivity.unregisterNetworkCallback(callback); callbackRegistered = false }
            invoke.resolve(JSObject())
        } catch (error: Exception) { invoke.reject(error.message) }
    }
}
