# iOS 0.8.5 自签名构建验证

日期：2026-10-29。产品为吉行影记；技术栈保持 Tauri 2 + React + Rust，iOS 平台接口使用 Swift 系统框架。

## 交付

- `output/jixing-yingji-0.8.5-ios-arm64-unsigned.ipa`
- 大小：6,584,011 字节（6.28 MiB）。
- SHA-256：`2daab7d2042b4b7df657f99d952afd7289bd0424ff78b362148f628f2c5e68c9`。
- Bundle ID：`com.leishuo.galaxyrecorder`；版本 0.8.5，build 8005；最低 iOS 15.0。
- 真机 arm64、LC_BUILD_VERSION platform=2、cryptid=0。无签名、无 embedded.mobileprovision、无额外热点或组播 entitlement；由用户自行签名。
- `tools/package_ios.py` 校验目标平台、架构、权限文案和 ZIP 完整性后生成包与 JSON。

## 适配与范围

补充 iOS 工程、配置、权限说明与 Swift 桥接。Photos 保存后用 `ph://` 索引管理，回看/分享按需生成临时缓存；Rust 媒体代理支持照片库资源。元数据使用 AVFoundation；剪辑遮挡使用不透明 Core Image 黑块并转换上下坐标。播放器沿用 React 自定义控件，通过平台接口控制系统音量、亮度和常亮。

重用现有记录仪协议、设备信息、录像列表、断点续传和用户偏好。iOS 采用系统设置连接热点；RTSP 实时预览明确提示暂不支持。不能保证 iOS 挂起后继续后台下载。

首轮 iOS 模拟器启动在 Wry 0.57.1 的 `NSBundle::bundleWithIdentifier` 中崩溃。将同版本源码复制到 `src-tauri/vendor/wry`，仅在 iOS 上改用公开的 `bundleForClass(WKWebView::class())` 读取真实框架版本，并避免卸载系统框架。未修改全局 Cargo 注册表；其他平台保留上游实现。原许可证和变更原因见该目录的 PATCH.md。

## 验证

- Xcode 26.3 / iPhoneOS SDK 26.2 Release 真机构建成功。
- `npm run build` 成功，前端 36 项测试通过。
- 最终依赖补丁后的 Rust 70 项测试通过。
- iOS 26.3.1 iPhone 17 模拟器安装及启动成功，确认设备页和 01 版安装图标；截图为 `analysis/ios-0.8.5-launch.png`。
- IPA 的 Info.plist、真机架构、加密标志、未签名状态、ZIP CRC、文件哈希已检查。
- 未使用个人证书，未上传 App Store，未向真实手机安装。

尚未验证：真实 iPhone 与车机连接、实际录像解码、系统音量硬件表现、照片库完整链路及剪辑成品。这些需要用户自签名装机后联调，不能以模拟器启动和单元测试替代。

## 后续修正：禁止手机端界面缩放

锁定页面 viewport，触摸区域仅保留横纵滚动；iOS 关闭 WKWebView 的页面捏合缩放与缩放回弹，Android 关闭 WebView 缩放支持与原生缩放控件。播放器自己的全屏画面双指缩放和拖动继续保留。

更新后的 iOS IPA 已重新编译并校验，前端 36 项测试通过；39 项界面检查通过，包含手机双指不改变 visualViewport.scale、单指页面滚动、全屏视频缩放/拖动。记录见 `analysis/mobile-zoom-verification.json`。Android 原生设置已修改，本次离线编译因 documentfile 等依赖未缓存而未完成；未生成新的 Android APK。
