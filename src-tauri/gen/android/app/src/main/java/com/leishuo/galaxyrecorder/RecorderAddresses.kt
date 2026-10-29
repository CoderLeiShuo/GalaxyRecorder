package com.leishuo.galaxyrecorder
import java.net.Inet4Address
internal object RecorderAddresses {
    // The original L6/NPD factory networks are outside RFC1918.
    fun local(address:java.net.InetAddress):Boolean = address is Inet4Address && (address.isSiteLocalAddress || address.hostAddress?.startsWith("193.168.0.")==true || address.hostAddress?.startsWith("192.72.1.")==true)
}
