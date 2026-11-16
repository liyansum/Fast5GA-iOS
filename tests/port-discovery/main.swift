import Foundation

func check(_ condition: Bool, _ message: String) {
    precondition(condition, message)
}

do {
    check(try RemotePairingPort.manual("") == nil, "blank setting must enable discovery")
    check(try RemotePairingPort.manual("  \n") == nil, "whitespace must enable discovery")
    check(try RemotePairingPort.manual(" 52107 ") == 52107, "dynamic port must survive parsing")
    check(try RemotePairingPort.manual("65535") == 65535, "upper valid port boundary")
} catch { fatalError("valid configuration rejected: \(error)") }

for invalid in ["0", "65536", "-1", "abc", "49152.0"] {
    do {
        _ = try RemotePairingPort.manual(invalid)
        fatalError("invalid port accepted: \(invalid)")
    } catch { }
}

// Regression: discovery must not select the first nearby phone on Wi-Fi.
check(!RemotePairingPort.belongsToThisDevice(loopbackInterface: false, addresses: ["192.168.1.20"], localAddresses: ["192.168.1.10"]), "reject a neighbouring device")
check(RemotePairingPort.belongsToThisDevice(loopbackInterface: false, addresses: ["192.168.1.10"], localAddresses: ["192.168.1.10"]), "accept this phone's Wi-Fi advertisement")
check(RemotePairingPort.belongsToThisDevice(loopbackInterface: true, addresses: [], localAddresses: []), "accept service scoped to the local loopback interface")

check(RemotePairingPort.fallback(previous: 52107, reason: "offline").value == 52107, "reuse previously successful dynamic port when offline discovery is unavailable")
check(RemotePairingPort.fallback(previous: nil, reason: "offline").value == 49152, "default port only when no successful port is known")
print("PASS: port overrides, local-device selection and offline port fallback")
