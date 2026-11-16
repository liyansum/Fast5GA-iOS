import Foundation
import Network
import Darwin

struct RemotePairingPort {
    let value: UInt16
    let source: String

    static func manual(_ text: String) throws -> UInt16? {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty { return nil }
        guard let value = UInt16(text), value > 0 else {
            throw NSError(domain: "Fast5GA", code: 2, userInfo: [NSLocalizedDescriptionKey: "服务端口应为 1–65535；留空则自动发现。"])
        }
        return value
    }

    static func fallback(previous: UInt16?, reason: String) -> Self {
        if let previous, previous > 0 { return .init(value: previous, source: "上次成功端口；\(reason)") }
        return .init(value: 49152, source: "默认回退：\(reason)")
    }

    static func belongsToThisDevice(loopbackInterface: Bool, addresses: Set<String>, localAddresses: Set<String>) -> Bool {
        loopbackInterface || !addresses.isDisjoint(with: localAddresses)
    }
}

/// Resolve the advertised port without opening a Remote Pairing control connection.
/// Only accept this device's service, never an arbitrary neighbouring iPhone.
@MainActor
final class RemotePairingPortResolver: NSObject, NetServiceDelegate {
    private var browser: NWBrowser?
    private var services: [NetService] = []
    private var servicesByName: [String: NetService] = [:]
    private var loopbackServices: Set<ObjectIdentifier> = []
    private var continuation: CheckedContinuation<RemotePairingPort, Never>?
    private var deadline: DispatchWorkItem?
    private var localAddresses: Set<String> = []
    private var previousPort: UInt16?
    private var fallbackReason = "未发现本机服务"

    func resolve(host: String, manual: UInt16?, previousPort: UInt16? = nil) async -> RemotePairingPort {
        if let manual { return .init(value: manual, source: "手动设置") }
        self.previousPort = previousPort
        localAddresses = Self.interfaceAddresses()
        localAddresses.insert(host)
        fallbackReason = "未发现本机服务"
        return await withCheckedContinuation { continuation in
            self.continuation = continuation
            let parameters = NWParameters.tcp
            parameters.includePeerToPeer = true
            let browser = NWBrowser(for: .bonjour(type: "_remotepairing._tcp", domain: "local."), using: parameters)
            self.browser = browser
            browser.browseResultsChangedHandler = { [weak self] results, _ in
                MainActor.assumeIsolated {
                    guard let self, self.continuation != nil else { return }
                    for result in results {
                        guard case let .service(name, type, domain, _) = result.endpoint else { continue }
                        let onLoopback = result.interfaces.contains { $0.type == .loopback }
                        let key = name + "|" + type + "|" + domain
                        if let existing = self.servicesByName[key] {
                            if onLoopback { self.loopbackServices.insert(ObjectIdentifier(existing)) }
                            continue
                        }
                        let service = NetService(domain: domain, type: type, name: name)
                        self.services.append(service)
                        self.servicesByName[key] = service
                        if onLoopback { self.loopbackServices.insert(ObjectIdentifier(service)) }
                        service.includesPeerToPeer = true
                        service.delegate = self
                        service.resolve(withTimeout: 4)
                    }
                }
            }
            browser.stateUpdateHandler = { [weak self] state in
                MainActor.assumeIsolated {
                    if case let .failed(error) = state {
                        guard let self else { return }
                        self.finish(RemotePairingPort.fallback(previous: self.previousPort, reason: "Bonjour \(error)"))
                    }
                }
            }
            let deadline = DispatchWorkItem { [weak self] in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    self.finish(RemotePairingPort.fallback(previous: self.previousPort, reason: self.fallbackReason))
                }
            }
            self.deadline = deadline
            DispatchQueue.main.asyncAfter(deadline: .now() + 5, execute: deadline)
            browser.start(queue: .main)
        }
    }

    func netServiceDidResolveAddress(_ sender: NetService) {
        guard continuation != nil, (1...65535).contains(sender.port) else { return }
        let addresses = Set((sender.addresses ?? []).compactMap(Self.numericHost))
        guard RemotePairingPort.belongsToThisDevice(
            loopbackInterface: loopbackServices.contains(ObjectIdentifier(sender)),
            addresses: addresses, localAddresses: localAddresses
        ) else { return }
        finish(.init(value: UInt16(sender.port), source: "Bonjour 自动发现本机服务"))
    }

    func netService(_ sender: NetService, didNotResolve errorDict: [String: NSNumber]) {
        fallbackReason = "本机服务解析未完成"
    }

    private func finish(_ result: RemotePairingPort) {
        guard let continuation else { return }
        self.continuation = nil
        deadline?.cancel(); deadline = nil
        browser?.cancel(); browser = nil
        services.forEach { $0.delegate = nil; $0.stop() }
        services.removeAll(); servicesByName.removeAll(); loopbackServices.removeAll()
        continuation.resume(returning: result)
    }

    private static func numericHost(_ data: Data) -> String? {
        data.withUnsafeBytes { bytes in
            guard let address = bytes.baseAddress?.assumingMemoryBound(to: sockaddr.self) else { return nil }
            var buffer = [CChar](repeating: 0, count: Int(NI_MAXHOST))
            guard getnameinfo(address, socklen_t(data.count), &buffer, socklen_t(buffer.count), nil, 0, NI_NUMERICHOST) == 0 else { return nil }
            return String(cString: buffer).components(separatedBy: "%")[0]
        }
    }

    private static func interfaceAddresses() -> Set<String> {
        var first: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&first) == 0 else { return [] }
        defer { freeifaddrs(first) }
        var addresses: Set<String> = []
        var cursor = first
        while let item = cursor {
            defer { cursor = item.pointee.ifa_next }
            guard let address = item.pointee.ifa_addr,
                  address.pointee.sa_family == UInt8(AF_INET) || address.pointee.sa_family == UInt8(AF_INET6) else { continue }
            if let host = numericHost(Data(bytes: address, count: Int(address.pointee.sa_len))) { addresses.insert(host) }
        }
        return addresses
    }
}
