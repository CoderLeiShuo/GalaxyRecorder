package com.leishuo.galaxyrecorder
import org.junit.Test
import org.junit.Assert.*
import java.net.InetAddress
class RecorderAddressesTest {
 @Test fun originalFactoryWifiSubnetsAreUsable(){assertTrue(RecorderAddresses.local(InetAddress.getByName("193.168.0.20")));assertTrue(RecorderAddresses.local(InetAddress.getByName("192.72.1.23")));assertTrue(RecorderAddresses.local(InetAddress.getByName("192.168.43.20")))}
 @Test fun arbitraryPublicAndIpv6NetworksAreNotRecorderWifi(){assertFalse(RecorderAddresses.local(InetAddress.getByName("8.8.8.8")));assertFalse(RecorderAddresses.local(InetAddress.getByName("::1")))}
}
