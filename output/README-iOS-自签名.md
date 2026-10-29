# 吉行影记 iOS 0.8.5 自签名测试版

安装文件：`jixing-yingji-0.8.5-ios-arm64-unsigned.ipa`。面向 iPhone / iPad 真机，最低 iOS 15.0，arm64。此 IPA 没有签名，也没有附带任何人的证书或描述文件；需要用户自己签名后安装。

## 使用 Apple ID 签名安装

1. 从 [Sideloadly 官网](https://sideloadly.io/) 安装电脑端工具，支持 macOS 和 Windows。
2. 用 USB 将 iPhone / iPad 连接到电脑，解锁并按系统提示信任电脑。
3. 将本目录的 IPA 拖入 Sideloadly，选择自己的设备，使用自己的 Apple ID 完成签名安装。账号和验证信息只在你选用的签名工具中填写。
4. 如系统要求，请按 [Apple 开发者模式说明](https://developer.apple.com/documentation/xcode/enabling-developer-mode-on-a-device) 开启开发者模式，并按安装工具指引信任开发者证书。
5. 打开吉行影记。在系统 Wi-Fi 设置中连接车机热点，打开车机记录仪，再返回本应用连接。首次请求时，允许访问本地网络；保存录像时允许照片权限。

Sideloadly 官方说明：免费 Apple ID 签名通常有效 7 天，之后需要重新签名或配置工具自动刷新。具体期限以实际签名配置为准。若已有自己的证书和描述文件，也可用你熟悉的 IPA 重签名工具安装。

## 本次 iOS 适配

- 保留 Tauri + React 界面、01 版相机图标、设备/车辆名称与图片自定义、连接方案选择和录像按日期分组。
- 复用记录仪协议、设备状态读取、录像列表、下载任务和断点续传逻辑。
- 自定义播放器按钮、进度、倍速和全屏交互；接入 iOS 的系统音量和屏幕常亮控制。
- 禁止手势缩放整个 App 界面，保留单指滚动及全屏视频画面的双指缩放、拖动。
- 录像保存到系统照片库，保存本地索引供离线回看、分享和删除。回看或分享时的临时缓存由应用清理，不另存永久应用内副本。
- 接入 iOS AVFoundation 剪辑和不透明车架号遮挡。相册不提供公开文件重命名接口，应用中的重命名仅修改本应用的录像显示名。

## 当前限制与测试边界

- 这是首次 iOS 测试版，未在真实 iPhone 和车机上完成联调。包结构、编译和自动测试不能代替真机连接验证。
- RTSP 实时预览尚未适配；可使用录像页回看和下载。视频格式及解码能力受 iOS 系统支持范围限制。
- 采用先在系统设置连接 Wi-Fi 的方式。当前不开放扫码自动入网、后台搜索附近 Wi-Fi，避免依赖普通自签名不一定具备的 Hotspot Configuration 权限。
- 原协议中的 UDP 广播/组播发现需要额外 Apple multicast entitlement。此包不请求该权限；保留已知地址、HTTP 网段发现及手动填写记录仪地址的连接方式。
- 下载时请保持应用在前台。iOS 可能挂起后台网络任务；任务进度会保留，返回应用后可继续，不能保证 Android 前台服务那样持续后台运行。
- 如果系统照片权限设为“部分照片”，需包含已保存的录像；从相册手动删除录像后，应用列表会在重新读取时同步。

## 构建与校验

在 macOS 上准备 Xcode、Rust 的 `aarch64-apple-ios` 编译目标和 Tauri 的 iOS 构建工具后，从项目根目录执行 `npm run ios:build`。该脚本先生成未签名真机 Archive，再校验并封装 IPA；不签名、不上传、不安装到真实设备。

同名 JSON 文件记录安装包大小、SHA-256、最低系统版本和架构。安装包更新后，以对应 JSON 为准。

参考：[Tauri iOS 构建与分发](https://v2.tauri.app/distribute/app-store/)、[Apple 本地网络权限](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy)、[Apple 热点配置权限](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.networking.hotspotconfiguration)。
