import AVFoundation
import CoreImage
import Darwin
import MediaPlayer
import Photos
import Tauri
import UIKit
import WebKit

// The web player supplies our own controls. Native APIs handle Photos, encoding,
// system volume and power management; no signing-only Wi-Fi capability is used.
final class RecorderMediaPlugin: Plugin {
    private var exporter: AVAssetExportSession?
    private var exportEpoch = 0
    private var volumeView: MPVolumeView?
    private var originalBrightness: CGFloat?
    private var originalIdle: Bool?

    override func load(webview: WKWebView) {
        // Disable WebView page zoom without consuming the player's pointer events.
        webview.scrollView.pinchGestureRecognizer?.isEnabled = false
        webview.scrollView.bouncesZoom = false
    }

    @objc func execute(_ invoke: Invoke) throws {
        let args = try JSONSerialization.jsonObject(with: Data(invoke.getRawArgs().utf8)) as? [String: Any] ?? [:]
        let operation = args["operation"] as? String ?? ""
        let p = args["payload"] as? [String: Any] ?? [:]
        DispatchQueue.main.async {
            switch operation {
            case "network": self.reply(invoke, self.network())
            case "save": self.save(invoke, p)
            case "resolveSaved": self.resolveSaved(invoke, p)
            case "inspectSavedMany": self.inspectSaved(invoke, p)
            case "deleteSaved": self.deleteSaved(invoke, p)
            case "renameSaved":
                // Photos has no public rename API; the persistent app index stores the display name.
                self.reply(invoke, ["uri": p["path"] as? String ?? ""])
            case "shareMany": self.share(invoke, p)
            case "metadata": self.metadata(invoke, p)
            case "export": self.export(invoke, p)
            case "cancelExport": self.exportEpoch += 1; self.exporter?.cancelExport(); invoke.resolve()
            case "control": self.control(invoke, p)
            case "status": self.reply(invoke, ["volume": AVAudioSession.sharedInstance().outputVolume, "exportProgress": (self.exporter?.progress ?? 0) * 100])
            case "environment": self.reply(invoke, ["os": "iOS", "version": UIDevice.current.systemVersion, "model": UIDevice.current.model])
            case "shareLogs", "saveLogs": self.logs(invoke, p)
            case "openWifi":
                if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
                invoke.resolve()
            default: invoke.reject("此 iOS 测试版暂不支持这项操作")
            }
        }
    }

    private func reply(_ invoke: Invoke, _ value: [String: Any]) {
        invoke.resolve(JSTypes.coerceDictionaryToJSObject(value) ?? [:])
    }

    private func photos(_ invoke: Invoke, _ body: @escaping () -> Void) {
        PHPhotoLibrary.requestAuthorization(for: .readWrite) { status in
            DispatchQueue.main.async {
                guard status == .authorized || status == .limited else {
                    invoke.reject("请在系统设置中允许访问照片，以保存和回看录像")
                    return
                }
                body()
            }
        }
    }

    private func asset(_ path: String) -> PHAsset? {
        guard path.hasPrefix("ph://") else { return nil }
        let id = String(path.dropFirst(5))
        return PHAsset.fetchAssets(withLocalIdentifiers: [id], options: nil).firstObject
    }

    private func save(_ invoke: Invoke, _ p: [String: Any]) {
        guard let path = p["path"] as? String else { invoke.reject("缺少录像路径"); return }
        if path.hasPrefix("ph://") {
            photos(invoke) {
                guard self.asset(path) != nil else { invoke.reject("相册录像已删除或未授权访问"); return }
                self.reply(invoke, ["uri": path])
            }
            return
        }
        guard FileManager.default.fileExists(atPath: path) else { invoke.reject("原始录像不存在"); return }
        let url = URL(fileURLWithPath: path)
        photos(invoke) {
            var identifier: String?
            PHPhotoLibrary.shared().performChanges({
                let request = PHAssetCreationRequest.forAsset()
                let options = PHAssetResourceCreationOptions()
                options.originalFilename = p["name"] as? String ?? url.lastPathComponent
                request.addResource(with: .video, fileURL: url, options: options)
                identifier = request.placeholderForCreatedAsset?.localIdentifier
            }) { success, error in
                if success, let identifier = identifier { self.reply(invoke, ["uri": "ph://" + identifier]) }
                else { invoke.reject(error?.localizedDescription ?? "录像保存到相册失败") }
            }
        }
    }

    private func inspectSaved(_ invoke: Invoke, _ p: [String: Any]) {
        photos(invoke) {
            let items = (p["paths"] as? [String] ?? []).map { path -> [String: Any] in
                ["path": path, "exists": path.hasPrefix("ph://") ? self.asset(path) != nil : FileManager.default.fileExists(atPath: path)]
            }
            self.reply(invoke, ["items": items])
        }
    }

    private func deleteSaved(_ invoke: Invoke, _ p: [String: Any]) {
        guard let path = p["path"] as? String, path.hasPrefix("ph://") else { invoke.reject("无效相册录像"); return }
        photos(invoke) {
            guard let asset = self.asset(path) else { invoke.resolve(); return }
            PHPhotoLibrary.shared().performChanges({ PHAssetChangeRequest.deleteAssets([asset] as NSArray) }) { ok, error in
                if ok { invoke.resolve() } else { invoke.reject(error?.localizedDescription ?? "删除未完成") }
            }
        }
    }

    // Photos owns the permanent file. Only materialize a disposable playback/share cache.
    private func materialize(_ path: String, _ completion: @escaping (Result<URL, Error>) -> Void) {
        guard path.hasPrefix("ph://") else { completion(.success(URL(fileURLWithPath: path))); return }
        guard let asset = asset(path) else {
            completion(.failure(NSError(domain: "Recorder", code: 1, userInfo: [NSLocalizedDescriptionKey: "相册录像已删除或未授权访问"])))
            return
        }
        let resources = PHAssetResource.assetResources(for: asset)
        guard let resource = resources.first(where: { $0.type == .video }) ?? resources.first(where: { $0.type == .fullSizeVideo }) else {
            completion(.failure(NSError(domain: "Recorder", code: 2, userInfo: [NSLocalizedDescriptionKey: "无法读取相册中的视频资源"])))
            return
        }
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("RecorderPlayback", isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let oldFiles = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: [.contentModificationDateKey])) ?? []
            for file in oldFiles {
                if let date = try? file.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
                   date.timeIntervalSinceNow < -86400 { try? FileManager.default.removeItem(at: file) }
            }
        } catch { completion(.failure(error)); return }
        let name = asset.localIdentifier.replacingOccurrences(of: "/", with: "_")
        let url = directory.appendingPathComponent(name).appendingPathExtension("mp4")
        if FileManager.default.fileExists(atPath: url.path) { completion(.success(url)); return }
        // A unique partial prevents concurrent cover/player requests from colliding.
        let temporary = directory.appendingPathComponent(UUID().uuidString + ".part")
        let options = PHAssetResourceRequestOptions()
        options.isNetworkAccessAllowed = true
        PHAssetResourceManager.default().writeData(for: resource, toFile: temporary, options: options) { error in
            if let error = error { try? FileManager.default.removeItem(at: temporary); completion(.failure(error)); return }
            do {
                if FileManager.default.fileExists(atPath: url.path) { try? FileManager.default.removeItem(at: temporary) }
                else { try FileManager.default.moveItem(at: temporary, to: url) }
                completion(.success(url))
            } catch { try? FileManager.default.removeItem(at: temporary); completion(.failure(error)) }
        }
    }

    private func resolveSaved(_ invoke: Invoke, _ p: [String: Any]) {
        guard let path = p["path"] as? String else { invoke.reject("缺少录像路径"); return }
        photos(invoke) {
            self.materialize(path) { result in
                switch result {
                case .success(let url): self.reply(invoke, ["path": url.path])
                case .failure(let error): invoke.reject(error.localizedDescription)
                }
            }
        }
    }

    private func share(_ invoke: Invoke, _ p: [String: Any]) {
        let paths = p["paths"] as? [String] ?? []
        guard !paths.isEmpty else { invoke.reject("请选择录像"); return }
        photos(invoke) {
            var urls = [URL]()
            func next(_ index: Int) {
                if index == paths.count {
                    DispatchQueue.main.async { self.presentShare(invoke, urls) }
                    return
                }
                self.materialize(paths[index]) { result in
                    switch result {
                    case .success(let url): urls.append(url); next(index + 1)
                    case .failure(let error): invoke.reject(error.localizedDescription)
                    }
                }
            }
            next(0)
        }
    }

    private func presentShare(_ invoke: Invoke, _ urls: [URL]) {
        guard let presenter = manager.viewController else { invoke.reject("无法打开分享窗口"); return }
        let sheet = UIActivityViewController(activityItems: urls, applicationActivities: nil)
        sheet.popoverPresentationController?.sourceView = presenter.view
        sheet.popoverPresentationController?.sourceRect = CGRect(x: presenter.view.bounds.midX, y: presenter.view.bounds.midY, width: 1, height: 1)
        presenter.present(sheet, animated: true)
        invoke.resolve()
    }

    private func logs(_ invoke: Invoke, _ p: [String: Any]) {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("吉行影记日志.txt")
        do { try (p["text"] as? String ?? "").write(to: url, atomically: true, encoding: .utf8); presentShare(invoke, [url]) }
        catch { invoke.reject(error.localizedDescription) }
    }

    private func mediaURL(_ value: String) -> URL? {
        value.hasPrefix("/") ? URL(fileURLWithPath: value) : URL(string: value)
    }

    private func metadata(_ invoke: Invoke, _ p: [String: Any]) {
        guard let value = p["source"] as? String, let url = mediaURL(value) else { invoke.reject("无效录像地址"); return }
        DispatchQueue.global(qos: .userInitiated).async {
            let asset = AVURLAsset(url: url)
            let seconds = asset.duration.seconds
            var result: [String: Any] = [:]
            if seconds.isFinite && seconds > 0 { result["duration"] = seconds }
            let generator = AVAssetImageGenerator(asset: asset)
            generator.appliesPreferredTrackTransform = true
            generator.maximumSize = CGSize(width: 480, height: 270)
            if let cgImage = try? generator.copyCGImage(at: CMTime(seconds: min(0.5, max(0, seconds / 2)), preferredTimescale: 600), actualTime: nil),
               let data = UIImage(cgImage: cgImage).jpegData(compressionQuality: 0.78) {
                result["thumbnail"] = "data:image/jpeg;base64," + data.base64EncodedString()
            }
            self.reply(invoke, result)
        }
    }

    private func export(_ invoke: Invoke, _ p: [String: Any]) {
        guard exporter == nil else { invoke.reject("已有录像正在处理"); return }
        guard let source = p["source"] as? String, let url = mediaURL(source), let output = p["output"] as? String else { invoke.reject("缺少导出路径"); return }
        exportEpoch += 1
        let epoch = exportEpoch
        let asset = AVURLAsset(url: url)
        asset.loadValuesAsynchronously(forKeys: ["tracks", "duration"]) {
            do {
                let length = asset.duration.seconds
                let start = p["start"] as? Double ?? 0
                let end = p["end"] as? Double ?? length
                guard length.isFinite, start.isFinite, end.isFinite, start >= 0, end > start, end <= length + 0.1,
                      let track = asset.tracks(withMediaType: .video).first else { throw self.failure("无效的录像时间范围") }
                let composition = AVMutableComposition()
                let range = CMTimeRange(start: CMTime(seconds: start, preferredTimescale: 600), end: CMTime(seconds: min(end, length), preferredTimescale: 600))
                let video = composition.addMutableTrack(withMediaType: .video, preferredTrackID: kCMPersistentTrackID_Invalid)!
                try video.insertTimeRange(range, of: track, at: .zero)
                video.preferredTransform = track.preferredTransform
                for track in asset.tracks(withMediaType: .audio) {
                    let audio = composition.addMutableTrack(withMediaType: .audio, preferredTrackID: kCMPersistentTrackID_Invalid)!
                    try audio.insertTimeRange(range, of: track, at: .zero)
                }
                guard let session = AVAssetExportSession(asset: composition, presetName: AVAssetExportPresetHighestQuality) else { throw self.failure("无法创建录像导出器") }
                if let mask = p["mask"] as? [String: Double] {
                    let x = mask["x"] ?? 0, y = mask["y"] ?? 0, w = mask["width"] ?? 0, h = mask["height"] ?? 0
                    guard [x,y,w,h].allSatisfy({ $0.isFinite }), x >= 0, y >= 0, w > 0, h > 0, x+w <= 1, y+h <= 1 else { throw self.failure("无效的遮挡区域") }
                    session.videoComposition = AVVideoComposition(asset: composition, applyingCIFiltersWithHandler: { request in
                        let image = request.sourceImage
                        let extent = image.extent
                        // Normalized mask uses a top-left origin; Core Image uses bottom-left.
                        let rectangle = CGRect(x: extent.minX + floor(extent.width*x), y: extent.maxY - ceil(extent.height*(y+h)), width: ceil(extent.width*w)+2, height: ceil(extent.height*h)+2)
                        let black = CIImage(color: CIColor(red: 0, green: 0, blue: 0, alpha: 1)).cropped(to: rectangle)
                        request.finish(with: black.composited(over: image).cropped(to: extent), context: nil)
                    })
                }
                session.outputURL = URL(fileURLWithPath: output)
                session.outputFileType = .mp4
                session.shouldOptimizeForNetworkUse = true
                DispatchQueue.main.async {
                    guard self.exportEpoch == epoch else { invoke.reject("导出已取消"); return }
                    self.exporter = session
                    session.exportAsynchronously {
                        DispatchQueue.main.async {
                            self.exporter = nil
                            if session.status == .completed { invoke.resolve() }
                            else { try? FileManager.default.removeItem(atPath: output); invoke.reject(session.error?.localizedDescription ?? "导出已取消") }
                        }
                    }
                }
            } catch { invoke.reject(error.localizedDescription) }
        }
    }

    private func failure(_ text: String) -> Error { NSError(domain: "Recorder", code: 1, userInfo: [NSLocalizedDescriptionKey: text]) }

    private func control(_ invoke: Invoke, _ p: [String: Any]) {
        let action = p["action"] as? String ?? ""
        let value = p["value"] as? Double ?? 0
        switch action {
        case "awake":
            if value != 0 {
                if originalIdle == nil { originalIdle = UIApplication.shared.isIdleTimerDisabled }
                if originalBrightness == nil { originalBrightness = UIScreen.main.brightness }
                UIApplication.shared.isIdleTimerDisabled = true
                try? AVAudioSession.sharedInstance().setCategory(.playback, mode: .moviePlayback)
                try? AVAudioSession.sharedInstance().setActive(true)
            } else {
                UIApplication.shared.isIdleTimerDisabled = originalIdle ?? false
                if let brightness = originalBrightness { UIScreen.main.brightness = brightness }
                originalIdle = nil; originalBrightness = nil
                try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
            }
        case "brightness": UIScreen.main.brightness = CGFloat(min(1, max(0.05, value)))
        case "volume":
            if volumeView == nil {
                let view = MPVolumeView(frame: CGRect(x: -200, y: -200, width: 120, height: 40))
                view.alpha = 0.01
                manager.viewController?.view.addSubview(view)
                volumeView = view
            }
            if let slider = volumeView?.subviews.compactMap({ $0 as? UISlider }).first {
                slider.setValue(Float(min(1, max(0, value))), animated: false)
                slider.sendActions(for: .touchUpInside)
            }
        default: break
        }
        reply(invoke, ["volume": AVAudioSession.sharedInstance().outputVolume])
    }

    private func network() -> [String: Any] {
        var pointer: UnsafeMutablePointer<ifaddrs>?
        var links = [[String: Any]]()
        if getifaddrs(&pointer) == 0 {
            var cursor = pointer
            while let current = cursor {
                let item = current.pointee
                if let address = item.ifa_addr, address.pointee.sa_family == UInt8(AF_INET),
                   String(cString: item.ifa_name) == "en0", (Int32(item.ifa_flags) & IFF_UP) != 0 {
                    var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
                    if getnameinfo(address, socklen_t(address.pointee.sa_len), &host, socklen_t(host.count), nil, 0, NI_NUMERICHOST) == 0 {
                        let ip = String(cString: host)
                        var prefix = 24
                        if let mask = item.ifa_netmask {
                            prefix = Int(mask.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { $0.pointee.sin_addr.s_addr.nonzeroBitCount })
                        }
                        links.append(["id": "en0:"+ip, "interfaceName": "en0", "ip": ip, "prefixLength": prefix, "gateway": NSNull(), "fingerprint": NSNull()])
                    }
                }
                cursor = item.ifa_next
            }
            freeifaddrs(pointer)
        }
        return ["platform": "ios", "available": !links.isEmpty, "selected": links.first as Any? ?? NSNull(), "networks": links, "bound": false, "defaultTransport": links.isEmpty ? "none" : "wifi", "message": links.isEmpty ? "请先在系统设置中连接车机热点" : "已连接 Wi-Fi"]
    }
}

@_cdecl("init_plugin_recorder_media")
func initRecorderMediaPlugin() -> Plugin { RecorderMediaPlugin() }
