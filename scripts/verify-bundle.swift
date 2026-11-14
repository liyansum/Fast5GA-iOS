import Foundation
import CoreFoundation

let fm = FileManager.default
let input = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true).standardizedFileURL
let temp = fm.temporaryDirectory.appendingPathComponent("Fast5GA-BundleCheck-" + UUID().uuidString, isDirectory: true)
try fm.createDirectory(at: temp, withIntermediateDirectories: true)
defer { try? fm.removeItem(at: temp) }

func inspect(_ url: URL, requireValid: Bool) throws {
    guard let bundle = CFBundleCreate(nil, url as CFURL) else { fatalError("CFBundleCreate failed: \(url.path)") }
    let executable = CFBundleCopyExecutableURL(bundle) as URL?
    let identifier = CFBundleGetIdentifier(bundle) as String?
    let info = CFBundleGetInfoDictionary(bundle) as NSDictionary?
    print("BUNDLE CHECK \(url.lastPathComponent): identifier=\(identifier ?? "nil"), executable=\(executable?.path ?? "nil"), resources=\(CFBundleCopyResourcesDirectoryURL(bundle) as URL?)")
    if requireValid {
        precondition(identifier == "com.liyansum.fast5ga", "Bundle metadata not found")
        precondition(info?["CFBundleExecutable"] as? String == "Fast5GA", "Executable metadata not found")
        precondition(executable?.lastPathComponent == "Fast5GA", "Executable path not found")
        precondition(fm.isExecutableFile(atPath: executable!.path), "Executable permissions lost")
    }
}
try inspect(input, requireValid: true)
let renamed = temp.appendingPathComponent("com.liyansum.fast5ga.app", isDirectory: true)
try fm.copyItem(at: input, to: renamed)
// LiveContainer changes MH_EXECUTE to MH_DYLIB before resolving the bundle.
let executable = renamed.appendingPathComponent("Fast5GA")
var macho = try Data(contentsOf: executable)
precondition(Array(macho.prefix(4)) == [0xcf,0xfa,0xed,0xfe], "Expected arm64 Mach-O")
macho.replaceSubrange(12..<16, with: [UInt8(6),0,0,0])
try macho.write(to: executable)
try fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: executable.path)
try inspect(renamed, requireValid: true)
// Reproduce the original packaging layout for a direct comparison on Apple CFBundle.
let legacy = temp.appendingPathComponent("LegacyLayout.app", isDirectory: true)
try fm.copyItem(at: renamed, to: legacy)
try fm.moveItem(at: legacy.appendingPathComponent("CarrierPayloads"), to: legacy.appendingPathComponent("Resources"))
try inspect(legacy, requireValid: false)
print("PASS: CoreFoundation resolved executable before and after LiveContainer-style rename/patch")
