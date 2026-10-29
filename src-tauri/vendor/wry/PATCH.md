# Local iOS startup compatibility patch

Upstream: `wry` 0.57.1 from crates.io, checksum `759c50ddad0ef9913bb5496cb9d509a50433a3b78cb0238d61105526589cf0e9`.

Only `src/wkwebview/mod.rs::platform_webview_version` is changed. On iOS it uses the public `NSBundle::bundleForClass(WKWebView::class())` API to read the actual WebKit bundle version and does not unload that system bundle. Other platforms retain the upstream implementation.

The unmodified lookup by identifier crashes on this machine's iOS 26.3.1 simulator before the app creates a window. The crash is a SIGTRAP in `CFRelease` via `__CFBundleCopyFrameworkURLForExecutablePath`, `_CFBundleGetBundleWithIdentifier`, `+[NSBundle bundleWithIdentifier:]`, and `wry::webview_version`. Pre-registering WebKit's bundle in `main.mm` did not avoid it. The direct class lookup avoids enumerating unrelated loaded images and still returns the actual framework version.

The original MIT/Apache-2.0 licenses are retained. Remove this Cargo patch when the upstream lookup handles this runtime safely. Do not modify the user's global Cargo registry copy.
