# 吉行影记 0.8.0：界面更新、改名与减重

用户已确认名称“吉行影记”，并授权打包。技术栈保持 Tauri 2 + React + TypeScript + Rust，Android 媒体与系统能力由 Kotlin 插件提供；Logo 沿用原图标。

安装名称、窗口标题、应用内默认名称、传输通知标题及日志名称同步改名。旧默认名称“银河记录仪”升级为“吉行影记”，用户自定义名称保留。包名 `com.leishuo.galaxyrecorder`、原文件索引和签名沿用旧版，未清除录像或设置。

## 安装包

- 文件：`output/jixing-yingji-0.8.0-ui-lite-arm64-test.apk`
- 版本：0.8.0，versionCode 8000；Android 8+ / ARM64。
- 大小：12,833,784 字节（12.24 MiB），旧 0.7.0 为 19,884,384 字节（18.96 MiB）；减少 7,050,600 字节，即 **35.46%**。
- SHA-256：`6575DE641054FB9D0A8BE84B3E6A5CAFA39B6543B1D96188821604E718167BC7`。
- 签名证书 SHA-256：`c05531d9229ed474d85cbb2c32f8bfe8ee775a5e3711c448315490a3ce345c0e`；与 0.7.0 及同签名旧测试版一致，versionCode 递增，可覆盖安装。
- apksigner、zipalign、ZIP CRC、原生库内容匹配、前端构建指纹及全部 ELF LOAD 段 16 KB 对齐检查均通过。

## 功能

- 设备页保留原记录仪图片，车型、年款、配置、连接状态集中展示。
- 标题、车辆显示名称和设备图片支持自定义、持久保存和分别恢复默认；车辆别名独立于自动识别车型。
- 连接方案改为完整车型标签卡片，保留全部协议、扫码、相册识码、Wi-Fi 搜索、自动连接和下拉重连。
- 移除演示模式及传输页右上角 Wi-Fi 按钮。
- 播放器去掉停止按钮，最高 4 倍速；保留全屏滑动调进度、亮度、音量及双指缩放拖动。
- 原有相册保存、文件抽屉、断点续传和删除任务等功能保留。

## 减重方式

1. 在用背景与设备图片从 PNG 无损转换为 WebP；使用 `cwebp -exact -lossless`，逐像素 RGBA 比较差异均为 0。原记录仪的外观、透明效果和分辨率保持，原 PNG 归档到 `design/source-assets/`。
2. 未使用的早期设备图移出产品目录；演示封面与测试视频放在 `tools/fixtures/`，不随前端发布。
3. 发布配置使用 `opt-level = "s"`、LTO、单代码生成单元、符号剥离及 `panic = "abort"`。视频解码仍由 Android 媒体引擎执行。配置依据见 [Tauri 体积优化文档](https://v2.tauri.app/concept/size/)。
4. 只构建 ARM64，保持旧包对应架构。原生库继续采用未压缩存储，没有通过安装时额外解压原生库来缩小 APK。
5. 在 build.rs 的最终链接步骤加入 Android 16 KB 页对齐参数，并核对 ELF LOAD 段和 APK ZIP 对齐。直接在 Cargo 配置中设置的参数会被本机 Tauri 构建覆盖，因此使用最终链接参数。参见 [Android 文档](https://developer.android.com/guide/practices/page-sizes) 与 [Tauri 对应问题记录](https://github.com/tauri-apps/tauri/issues/14895)。

## 验证范围

- TypeScript/Vite 前端生产构建、Android ARM64 Release 构建通过。
- Rust 68 项，前端 33 项，Android JVM 15 项；媒体界面 34 项、设备/方案界面 18 项、Android 播放桥接界面 9 项、连接状态界面 11 项、工作流界面 13 项，共 201 项检查。
- 界面检查的记录仪及 Android 系统调用使用模拟桥接；媒体检查实际播放本地 MP4。未将本次构建安装到手机，也未实车验证。
- 校验安装名称、版本、签名、ZIP 完整性与对齐、ARM64 ELF LOAD 对齐、APK 中原生库和本机构建产物一致、前端构建指纹一致，以及 R8 后插件类和回调保留。
- 旧 0.7.0 包保留不变。最终文件大小、哈希和签名见 `output/build-info-0.8.0.json`；构建信息中的 Installed、LaunchVerified、VehicleVerified 均为 false。
