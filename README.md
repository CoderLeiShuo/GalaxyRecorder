# 银河记录仪

React + Tauri 2 手机与电脑客户端，优先适配 2026 款星越 L。当前界面采用用户选定的 A 版「冰川银」浅色设计。

## 0.1.2 连接修正

针对“原版可连、复刻版无法连接”的反馈，逐条核对原 APK 的实际方法后，修复以下兼容缺口：

- Android 原生读取 Wi-Fi 的 IPv4、网关和掩码，将本应用的连接与下载绑定到 Wi-Fi，避免无互联网的车机热点被移动数据分流。
- 自动查找无需输入 IP，同时监听 UDP 49142 / 53296 和执行有界 HTTP 识别；支持实际 /23 网段及记住的自定义端口。
- A7 识别接受 HTTP 200 的简短 `OK` 响应，不再额外要求包含 `DVR` 或属性名；继续拒绝 HTML。
- 星越 L 的 A7 普通/紧急录像目录改为 `DCIM / EVENT`。
- 新增查找进度、停止查找、网络自检、可复制的连接诊断。自检只检查本应用使用 Wi-Fi 的能力，不扫描网络。

原包证据及验证范围见 [连接修正说明](analysis/CONNECTION-0.1.2.md)。本版尚未通过实车验收，不能把模拟协议测试等同于连接车辆成功。

## 手机测试

1. 开启车机热点，在车机上打开一次原厂记录仪应用。
2. 手机连接车机热点，即使系统显示“无互联网”也保留连接。
3. 打开本应用，点击「连接记录仪 → 自动查找记录仪」，最多约 35 秒。
4. 如仍失败，打开「连接诊断 → 复制诊断」，保留文本供后续分析。已知原版记录仪地址时，也可展开手动地址入口。

安装包：`output/galaxy-recorder-0.1.2-connection-fix-arm64-test.apk`，Android 8+、ARM64，使用测试签名。安装与实车验证状态以 `output/build-info.json` 为准。

## 开发与验证

```text
npm install
npm run dev
npm run desktop:dev
npm run build
npm test
cargo test --manifest-path src-tauri/Cargo.toml
node tools/check_ui.cjs
node tools/check_connection_ui.cjs
```

最后两项检查要求本地开发页面运行于 1420 端口，并使用当前工作站的 Chrome/Playwright 路径。连接页检查使用模拟 Tauri 桥接，不会查找真实设备。

Android 环境需要 Android SDK/NDK、JDK 和 Rust Android target。Windows PowerShell 测试构建：

```powershell
$env:GALAXY_TEST_SIGNING = '1'
npm run tauri -- android build --target aarch64 --apk --ci
```

原 APK 静态报告位于 `analysis/APK-analysis.md`；批准的设计及实现截图位于 `design/glacier-a/`。演示数据明确标识，实时画面、在线回看、片段导出、车架号打码、系统相册导出及其他车型仍未完成。
