// swift-tools-version:5.5
import PackageDescription

let package = Package(
    name: "RecorderIOS",
    platforms: [.iOS(.v15), .macOS(.v10_13)],
    products: [.library(name: "RecorderIOS", type: .static, targets: ["RecorderIOS"])],
    dependencies: [.package(name: "Tauri", path: "../.tauri/tauri-api")],
    targets: [.target(name: "RecorderIOS", dependencies: [.byName(name: "Tauri")], path: ".", exclude: ["Package.swift"])]
)
