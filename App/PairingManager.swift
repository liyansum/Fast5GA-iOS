import Foundation
import AirliftFFI

@MainActor
final class PairingManager: ObservableObject {
    @Published var running = false
    @Published var status = "尚未开始配对"
    @Published var pin: String?
    @Published var paired = FileManager.default.fileExists(atPath: AppModel.pairingURL.path)
    private var service: NetService?
    private let keepAlive = KeepAlive()
    private let permission = LocalNetworkAuthorization()
    func start() {
        guard !running else { return }
        guard NetworkSnapshot.read().wifi != nil else { status = "请先连接 Wi-Fi"; return }
        running = true; pin = nil; status = "正在启动配对…"
        Task {
            _ = await permission.request()
            keepAlive.startAudio()
            let path = AppModel.pendingPairingURL.path
            try? FileManager.default.removeItem(atPath: path)
            if let bytes = try? Data(contentsOf: AppModel.pairingURL) {
                try? bytes.write(to: AppModel.pendingPairingURL, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
            }
            let irk = UserDefaults.standard.string(forKey: "hostAltIRK") ?? ""
            let ctx = Unmanaged.passRetained(self).toOpaque()
            DispatchQueue.global(qos: .userInitiated).async {
                var result = ALPairResult()
                let rc = path.withCString { p in irk.withCString { i in
                    al_pairing_run_host("0.0.0.0", 0, "Fast5GA", "Mac17,7", p, i, pairReady, pairPIN, ctx, &result)
                } }
                let error = result.error.map { String(cString: $0) } ?? "配对失败"
                let key = result.host_alt_irk_hex.map { String(cString: $0) } ?? ""
                al_pairing_result_free(&result)
                DispatchQueue.main.async {
                    let owner = Unmanaged<PairingManager>.fromOpaque(ctx).takeRetainedValue()
                    owner.service?.stop(); owner.service = nil; owner.running = false; owner.pin = nil
                    DispatchQueue.main.asyncAfter(deadline: .now() + 5) { owner.keepAlive.stopAll() }
                    if rc == 0 {
                        do {
                            let bytes = try Data(contentsOf: AppModel.pendingPairingURL)
                            guard !bytes.isEmpty else { throw CocoaError(.fileReadCorruptFile) }
                            try bytes.write(to: AppModel.pairingURL, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
                            UserDefaults.standard.set(key, forKey: "hostAltIRK")
                            owner.paired = true; owner.status = "配对成功，可以返回写入页面"
                        } catch { owner.status = "保存失败：" + error.localizedDescription }
                    } else { owner.status = error }
                    try? FileManager.default.removeItem(at: AppModel.pendingPairingURL)
                }
            }
        }
    }
    func cancel() { guard running else { return }; status = "正在结束配对…"; fg_pair_cancel() }
    fileprivate func advertise(id: String, port: Int32, txt: [String: Data]) {
        let s = NetService(domain: "", type: "_remotepairing-pairable-host._tcp.", name: id, port: port)
        s.setTXTRecord(NetService.data(fromTXTRecord: txt)); s.publish(); service = s
        status = "打开设置 → 隐私与安全性 → 开发者模式 → 与 Fast5GA 配对"
    }
    fileprivate func show(pin: String) { self.pin = pin; status = "在系统设置的配对页面输入此 PIN" }
}
private let pairReady: ALPairReadyCb = { ctx, serviceID, port, keys, values, count in
    guard let ctx, let serviceID else { return }
    let id = String(cString: serviceID)
    var txt: [String: Data] = [:]
    if let keys, let values { for i in 0..<Int(count) { if let k = keys[i], let v = values[i] { txt[String(cString: k)] = Data(String(cString: v).utf8) } } }
    let owner = Unmanaged<PairingManager>.fromOpaque(ctx).takeUnretainedValue()
    DispatchQueue.main.async { owner.advertise(id: id, port: Int32(port), txt: txt) }
}
private let pairPIN: ALPairPinCb = { pin, ctx in
    guard let ctx, let pin else { return }; let text = String(cString: pin)
    let owner = Unmanaged<PairingManager>.fromOpaque(ctx).takeUnretainedValue()
    DispatchQueue.main.async { owner.show(pin: text) }
}
