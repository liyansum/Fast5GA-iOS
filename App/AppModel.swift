import Foundation
import SwiftUI
import AirliftFFI
import UIKit
import Darwin

struct Operation: Identifiable {
    let id: String
    let title: String
    let subtitle: String
    let icon: String
    static let normal = Operation(id: "normal", title: "普通模式", subtitle: "手机重启后执行 · 写入 Overlay 并重载", icon: "bolt.fill")
    static let full = Operation(id: "full", title: "完整模式", subtitle: "首次使用执行 · 安装 IPCC 与全部载荷", icon: "square.stack.3d.up.fill")
    static let debug = [
        Operation(id: "debug_ipcc", title: "加载原版 73.0 IPCC", subtitle: "iPhone 14 Pro 联通原包 · 检查 /var 目录", icon: "shippingbox"),
        Operation(id: "debug_bundle", title: "写入 Bundle 三件套", subtitle: "PRI、Overrides、签名 · 0644", icon: "doc.on.doc"),
        Operation(id: "debug_aux", title: "写入其他三个文件", subtitle: "Carrier、Common 签名、Info · 0600", icon: "folder"),
        Operation(id: "debug_overlay", title: "写入两个 Overlay", subtitle: "72.0.1 与 73.0 · 0555", icon: "square.3.layers.3d"),
        Operation(id: "debug_reload", title: "重载 CommCenter", subtitle: "等待 5 秒 · 仅结束一次 · 观察新进程 15 秒", icon: "arrow.clockwise")
    ]
}

struct NetworkSnapshot {
    let wifi: String?
    let tunnel: Bool
    static func read() -> Self {
        var first: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&first) == 0 else { return .init(wifi: nil, tunnel: false) }
        defer { freeifaddrs(first) }
        var cursor = first; var wifi: String?; var tunnel = false
        while let item = cursor {
            defer { cursor = item.pointee.ifa_next }
            guard let addr = item.pointee.ifa_addr else { continue }
            let name = String(cString: item.pointee.ifa_name)
            let flags = Int32(item.pointee.ifa_flags)
            guard flags & IFF_UP != 0, addr.pointee.sa_family == UInt8(AF_INET) else { continue }
            if name.hasPrefix("utun") || name.hasPrefix("ipsec") { tunnel = true }
            if name == "en0" {
                var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
                if getnameinfo(addr, socklen_t(addr.pointee.sa_len), &host, socklen_t(host.count), nil, 0, NI_NUMERICHOST) == 0 {
                    wifi = String(cString: host)
                }
            }
        }
        return .init(wifi: wifi, tunnel: tunnel)
    }
}

@MainActor
final class AppModel: ObservableObject {
    @Published var busy = false
    @Published var connected = false
    @Published var current = "等待连接"
    @Published var progress: Double = 0
    @Published var logs: [String] = []
    @Published var alertText: String?
    @Published var showingAlert = false
    @Published var network = NetworkSnapshot.read()
    @Published var targetHost = UserDefaults.standard.string(forKey: "deviceIP") ?? "10.7.0.1"
    @Published var lastResult = UserDefaults.standard.string(forKey: "lastResult") ?? "尚未执行"
    let pairing = PairingManager()
    private var timer: Timer?
    private var logURL: URL?
    private var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    private static let directory: URL = {
        let url = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Fast5GA", isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        var protected = url; var values = URLResourceValues(); values.isExcludedFromBackup = true
        try? protected.setResourceValues(values)
        return url
    }()
    static var pairingURL: URL { directory.appendingPathComponent("device.rppairing") }
    static var pendingPairingURL: URL { directory.appendingPathComponent("pending.rppairing") }
    var hasPairing: Bool { FileManager.default.fileExists(atPath: Self.pairingURL.path) }
    var canRun: Bool { hasPairing && !busy && !pairing.running }

    init() {
        timer = Timer.scheduledTimer(withTimeInterval: 3, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refreshNetwork() }
        }
    }
    func refreshNetwork() {
        network = .read()
        if network.wifi == nil { connected = false }
    }
    func notify(_ text: String) { alertText = text; showingAlert = true }
    func append(_ line: String) {
        let stamp = DateFormatter.localizedString(from: Date(), dateStyle: .none, timeStyle: .medium)
        let text = "[\(stamp)] \(line)"
        logs.append(text); if logs.count > 2000 { logs.removeFirst(logs.count - 2000) }
        if line.hasPrefix("STEP ") {
            let parts = line.split(separator: " ", maxSplits: 2)
            if parts.count > 1 {
                let fractions = parts[1].split(separator: "/").compactMap { Double($0) }
                if fractions.count == 2, fractions[1] > 0 { progress = (fractions[0] - 1) / fractions[1] }
            }
            current = parts.count == 3 ? String(parts[2]) : line
        }
        if let url = logURL, let bytes = (text + "\n").data(using: .utf8), let h = try? FileHandle(forWritingTo: url) {
            defer { try? h.close() }; _ = try? h.seekToEnd(); try? h.write(contentsOf: bytes)
        }
    }
    func perform(_ op: Operation) { run(mode: op.id, title: op.title) }
    func check() { run(mode: "check", title: "检查连接") }
    private func run(mode: String, title: String) {
        guard canRun else { return }
        refreshNetwork()
        guard network.wifi != nil else { notify("请先连接 Wi-Fi，并保持 LocalDevVPN 开启。"); return }
        var address = in_addr()
        guard inet_pton(AF_INET, targetHost, &address) == 1 else { notify("Device IP 格式不正确。"); return }
        guard let resources = Optional(Bundle.main.bundleURL.appendingPathComponent("CarrierPayloads")),
              FileManager.default.fileExists(atPath: resources.appendingPathComponent("payloads.json").path) else {
            notify("应用载荷不完整，请重新导入完整 IPA。"); return
        }
        UserDefaults.standard.set(targetHost, forKey: "deviceIP")
        busy = true; connected = false; progress = 0; current = title
        UIApplication.shared.isIdleTimerDisabled = true
        backgroundTask = UIApplication.shared.beginBackgroundTask(withName: "Fast5GA") { }
        let dir = Self.directory.appendingPathComponent("Logs", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        logURL = dir.appendingPathComponent("\(Int(Date().timeIntervalSince1970))_\(mode).log")
        FileManager.default.createFile(atPath: logURL!.path, contents: nil)
        append("开启5GA · \(title)")
        let pair = Self.pairingURL.path, path = resources.path, host = targetHost
        let context = Unmanaged.passRetained(self).toOpaque()
        DispatchQueue.global(qos: .userInitiated).async {
            var output: UnsafeMutablePointer<CChar>?
            let rc = mode.withCString { m in pair.withCString { p in path.withCString { d in host.withCString { h in
                fg_run(m, p, d, h, operationLog, context, &output)
            } } } }
            let message = output.map { String(cString: $0) } ?? "未知结果"
            if let output { al_string_free(output) }
            DispatchQueue.main.async {
                let model = Unmanaged<AppModel>.fromOpaque(context).takeRetainedValue()
                model.busy = false
                UIApplication.shared.isIdleTimerDisabled = false
                if model.backgroundTask != .invalid { UIApplication.shared.endBackgroundTask(model.backgroundTask); model.backgroundTask = .invalid }
                if rc == 0 {
                    model.connected = true; model.progress = 1; model.current = mode == "check" ? "连接检查通过" : "操作已完成"
                    model.append(model.current)
                    if mode != "check" {
                        model.lastResult = title + " · " + DateFormatter.localizedString(from: Date(), dateStyle: .short, timeStyle: .short)
                        UserDefaults.standard.set(model.lastResult, forKey: "lastResult")
                        model.notify(mode == "debug_ipcc" ? "原版 73.0 IPCC 已加载。若用于恢复原版，请手动重启手机。" : "\(title)的步骤已完成。未回读测量目标权限或确认网络效果，请在手机上查看。")
                    }
                } else {
                    model.current = "操作停止"; model.append("失败：" + message)
                    model.notify(message + "\n\n已停止后续步骤，详情可在日志中查看。")
                }
            }
        }
    }
    func importPairing(_ url: URL) {
        guard !busy && !pairing.running else { return }
        let access = url.startAccessingSecurityScopedResource(); defer { if access { url.stopAccessingSecurityScopedResource() } }
        do {
            let bytes = try Data(contentsOf: url)
            let p = try PropertyListSerialization.propertyList(from: bytes, format: nil)
            guard let dict = p as? [String: Any], (dict["private_key"] as? Data)?.count == 32, (dict["public_key"] as? Data)?.count == 32, dict["identifier"] is String else {
                throw NSError(domain: "Fast5GA", code: 1, userInfo: [NSLocalizedDescriptionKey: "请选择 Remote Pairing 配对文件。旧式 Lockdown 配对文件不适用此流程。"])
            }
            try bytes.write(to: Self.pairingURL, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
            connected = false; append("已导入配对文件"); notify("配对文件已导入，请检查连接。")
        } catch { notify(error.localizedDescription) }
    }
    var exportLog: URL? { logURL }
}

private let operationLog: ALLogCallback = { ctx, text in
    guard let ctx, let text else { return }
    let message = String(cString: text)
    let model = Unmanaged<AppModel>.fromOpaque(ctx).takeUnretainedValue()
    DispatchQueue.main.async { model.append(message) }
}
