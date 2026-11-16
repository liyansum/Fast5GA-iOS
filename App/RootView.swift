import SwiftUI
import UniformTypeIdentifiers

struct RootView: View {
    @EnvironmentObject var model: AppModel
    var body: some View {
        TabView {
            WriteView().tabItem { Label("写入", systemImage: "antenna.radiowaves.left.and.right") }
            PairingView(manager: model.pairing).tabItem { Label("配对", systemImage: "link") }
            DebugView().tabItem { Label("调试", systemImage: "slider.horizontal.3") }
            LogView().tabItem { Label("日志", systemImage: "terminal") }
        }
        .alert("开启5GA", isPresented: $model.showingAlert) { Button("好", role: .cancel) {} } message: { Text(model.alertText ?? "") }
    }
}
private struct Canvas<Content: View>: View {
    let title: String
    @ViewBuilder var content: () -> Content
    var body: some View {
        NavigationStack {
            ZStack {
                Color(red: 0.025, green: 0.045, blue: 0.085).ignoresSafeArea()
                ScrollView { VStack(alignment: .leading, spacing: 20, content: content).padding(20).padding(.bottom, 24) }
            }.navigationTitle(title)
        }
    }
}
private struct Surface<Content: View>: View {
    @ViewBuilder var content: () -> Content
    var body: some View { VStack(alignment: .leading, spacing: 14, content: content).frame(maxWidth: .infinity, alignment: .leading).padding(20).background(.white.opacity(0.055), in: RoundedRectangle(cornerRadius: 24)).overlay(RoundedRectangle(cornerRadius: 24).stroke(.white.opacity(0.07))) }
}
private struct WriteView: View {
    @EnvironmentObject var model: AppModel
    var body: some View {
        Canvas(title: "开启5GA") {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 9) {
                    Text("让连接，继续向前。").font(.title2.bold())
                    Text("iPhone 14 Pro  ·  中国联通\niOS 27.0.1  ·  73.0 配置").font(.subheadline).foregroundStyle(.secondary).lineSpacing(5)
                }
                Spacer()
                Image(systemName: "antenna.radiowaves.left.and.right").font(.system(size: 38, weight: .light)).foregroundStyle(.cyan).padding(.top, 5)
            }.padding(.vertical, 12)
            ConnectionCard(manager: model.pairing)
            if model.busy { ProgressCard() }
            VStack(alignment: .leading, spacing: 12) {
                Text("选择模式").font(.headline)
                OperationButton(operation: .full, accent: .blue)
                OperationButton(operation: .normal, accent: .cyan)
            }
            Surface {
                Label("使用顺序", systemImage: "sparkles").font(.headline)
                Text("首次：连接 Wi-Fi → 配对 → 完整模式。\n以后每次重启手机：执行普通模式（写入后重载，不再补写）。\n保存配对后可关闭 Wi-Fi，保持 VPN 开启，再检查连接。").font(.subheadline).foregroundStyle(.secondary).lineSpacing(6)
                Divider().overlay(.white.opacity(0.05))
                Text("上次完成：\(model.lastResult)").font(.caption).foregroundStyle(.secondary)
            }
            Text("沿用原批处理的联通配置。写入完成后，请自行查看信号与网络效果。").font(.caption).foregroundStyle(.secondary)
        }
    }
}
private struct ConnectionCard: View {
    @EnvironmentObject var model: AppModel
    @ObservedObject var manager: PairingManager
    var body: some View {
        Surface {
            HStack {
                Circle().fill(model.connected ? .green : .orange).frame(width: 9, height: 9)
                Text(model.connected ? "连接检查通过" : (manager.paired || model.hasPairing ? "已保存配对，请检查连接" : "请先完成配对")).font(.subheadline.bold())
                Spacer()
                if model.busy { ProgressView().controlSize(.small) }
            }
            HStack(spacing: 16) {
                Label(model.network.wifi == nil ? "Wi-Fi 未连接" : "Wi-Fi 已连接", systemImage: "wifi")
                Label(model.network.tunnel ? "检测到隧道" : "保持 VPN 开启", systemImage: "point.3.connected.trianglepath.dotted")
            }.font(.caption).foregroundStyle(.secondary)
            Button(action: model.check) { Text("检查连接").font(.subheadline.bold()).frame(maxWidth: .infinity).padding(.vertical, 10) }
                .buttonStyle(.bordered).disabled(!model.canRun)
        }
    }
}
private struct OperationButton: View {
    @EnvironmentObject var model: AppModel
    let operation: Operation
    let accent: Color
    var body: some View {
        Button { model.perform(operation) } label: {
            HStack(spacing: 16) {
                Image(systemName: operation.icon).font(.title2).foregroundStyle(accent).frame(width: 44, height: 50).background(accent.opacity(0.10), in: RoundedRectangle(cornerRadius: 14))
                VStack(alignment: .leading, spacing: 6) { Text(operation.title).font(.headline).foregroundStyle(.white); Text(operation.subtitle).font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.leading) }
                Spacer(minLength: 0)
                Image(systemName: "chevron.right").font(.caption.bold()).foregroundStyle(.secondary)
            }.padding(18).background(.white.opacity(0.055), in: RoundedRectangle(cornerRadius: 22)).overlay(RoundedRectangle(cornerRadius: 22).stroke(accent.opacity(0.22)))
        }.buttonStyle(.plain).disabled(!model.canRun).opacity(model.canRun ? 1 : 0.5)
    }
}
private struct ProgressCard: View {
    @EnvironmentObject var model: AppModel
    var body: some View { Surface { HStack { ProgressView().controlSize(.small); Text(model.current).font(.subheadline.bold()) }; ProgressView(value: model.progress).tint(.cyan); Text("保持程序在前台，等待本次操作结束。").font(.caption).foregroundStyle(.secondary) } }
}
private struct PairingView: View {
    @EnvironmentObject var model: AppModel
    @ObservedObject var manager: PairingManager
    @State private var importing = false
    var body: some View {
        Canvas(title: "连接与配对") {
            Surface {
                Label(manager.paired || model.hasPairing ? "配对文件已保存" : "在这台 iPhone 上配对", systemImage: "link.circle.fill").font(.headline)
                Text("连接 Wi-Fi，保持 LocalDevVPN 开启，并在系统设置中启用开发者模式。").foregroundStyle(.secondary).font(.subheadline)
                Text(manager.status).font(.subheadline).foregroundStyle(manager.running ? .cyan : .secondary)
                if let pin = manager.pin {
                    Text(pin).font(.system(size: 38, weight: .bold, design: .monospaced)).tracking(6).textSelection(.enabled).frame(maxWidth: .infinity).padding(.vertical, 12).background(.cyan.opacity(0.12), in: RoundedRectangle(cornerRadius: 14))
                }
                if manager.running {
                    Button("取消配对", role: .destructive) { manager.cancel() }.buttonStyle(.bordered)
                } else {
                    Button(manager.paired || model.hasPairing ? "重新配对" : "开始本机配对") { model.connected = false; manager.start() }.buttonStyle(.borderedProminent).disabled(model.busy)
                }
            }
            Surface {
                Text("配对步骤").font(.headline)
                ForEach(Array(["点击“开始本机配对”，允许本地网络访问。", "打开设置 → 隐私与安全性 → 开发者模式。", "选择“与 Fast5GA 配对”，输入程序显示的 PIN。", "回到程序，检查连接，然后选择写入模式。"].enumerated()), id: \.offset) { index, text in
                    HStack(alignment: .top, spacing: 12) { Text("\(index + 1)").font(.caption.bold()).foregroundStyle(.cyan).frame(width: 24, height: 24).background(.cyan.opacity(0.10), in: Circle()); Text(text).font(.subheadline).foregroundStyle(.secondary) }
                }
                Text("切换到设置期间会播放无声音频，帮助保持配对服务运行。LiveContainer 需要允许宿主后台音频运行。").font(.caption).foregroundStyle(.secondary)
            }
            Surface {
                Text("LocalDevVPN 的 Device IP").font(.subheadline.bold())
                TextField("10.7.0.1", text: $model.targetHost).keyboardType(.decimalPad).textInputAutocapitalization(.never).autocorrectionDisabled().font(.system(.body, design: .monospaced)).disabled(model.busy || manager.running)
                Text("填写 VPN 中的 Device IP。默认 10.7.0.1；程序不会更改 VPN 配置。").font(.caption).foregroundStyle(.secondary)
                Text("Remote Pairing 服务端口").font(.subheadline.bold())
                TextField("自动发现（留空）", text: $model.targetPort).keyboardType(.numberPad).font(.system(.body, design: .monospaced)).disabled(model.busy || manager.running)
                Text("默认自动发现本机端口，发现失败时尝试同一 Device IP 上次成功的端口。若其他工具能连接，可填写 SideStore 网络发现或 StikDebug 显示的 Remote Pairing 端口；这不是配对时的 PIN，也不是配对主机端口。").font(.caption).foregroundStyle(.secondary)
                Button("检查连接", action: model.check).buttonStyle(.bordered).disabled(!model.canRun)
                Button("导入已有配对文件") { importing = true }.font(.subheadline).disabled(model.busy || manager.running)
            }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.data, .propertyList], allowsMultipleSelection: false) { result in
            if case let .success(urls) = result, let url = urls.first { model.importPairing(url); manager.paired = model.hasPairing }
            if case let .failure(error) = result { model.notify(error.localizedDescription) }
        }
    }
}
private struct DebugView: View {
    @EnvironmentObject var model: AppModel
    var body: some View {
        Canvas(title: "调试选项") {
            Text("每次只执行所选项目，顺序与批处理菜单一致。").font(.subheadline).foregroundStyle(.secondary)
            if model.busy { ProgressCard() }
            ForEach(Operation.debug) { op in OperationButton(operation: op, accent: .mint) }
            Surface { Label("恢复原版", systemImage: "arrow.uturn.backward").font(.headline); Text("执行“加载原版 73.0 IPCC”后手动重启手机。程序不会自动重启设备。").font(.subheadline).foregroundStyle(.secondary) }
        }
    }
}
private struct LogView: View {
    @EnvironmentObject var model: AppModel
    var body: some View {
        NavigationStack {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 8) {
                        if model.logs.isEmpty { Text("配对后执行连接检查或写入，运行记录会显示在这里。").foregroundStyle(.secondary) }
                        ForEach(Array(model.logs.enumerated()), id: \.offset) { index, line in Text(line).font(.system(size: 11, design: .monospaced)).foregroundStyle(line.contains("失败") ? .orange : .secondary).frame(maxWidth: .infinity, alignment: .leading).textSelection(.enabled).id(index) }
                    }.padding(20)
                }.background(Color(red: 0.025, green: 0.045, blue: 0.085)).navigationTitle("运行日志")
                .toolbar { if let url = model.exportLog { ShareLink(item: url) { Image(systemName: "square.and.arrow.up") } } }
                .onChange(of: model.logs.count) { _ in if let last = model.logs.indices.last { proxy.scrollTo(last, anchor: .bottom) } }
            }
        }
    }
}
