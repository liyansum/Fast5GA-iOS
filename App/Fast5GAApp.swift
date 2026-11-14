import SwiftUI

@main
struct Fast5GAApp: App {
    @StateObject private var model = AppModel()
    var body: some Scene {
        WindowGroup {
            RootView().environmentObject(model).tint(.cyan)
                .preferredColorScheme(.dark)
        }
    }
}
