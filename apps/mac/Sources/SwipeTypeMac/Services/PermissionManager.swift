import ApplicationServices
import Carbon.HIToolbox
import Cocoa

/// Tracks the Accessibility (macOS 27: "Device Control and Data Access")
/// permission SwipeType needs for its keyboard event tap and for typing into
/// other apps.
///
/// Trust is polled rather than requested once at launch: macOS does not notify
/// apps when the user flips the switch, and a grant can silently stop matching
/// after the app is rebuilt or updated with a different signature.
@MainActor
final class PermissionManager: ObservableObject {
    static let shared = PermissionManager()

    @Published private(set) var isTrusted = AXIsProcessTrusted()
    /// True while another app (a password field, Terminal's Secure Keyboard
    /// Entry, …) has secure input enabled, which hides keystrokes from event taps.
    @Published private(set) var isSecureInputActive = IsSecureEventInputEnabled()

    private var timer: Timer?

    private init() {}

    /// Name of the System Settings pane that holds the permission.
    nonisolated static var paneName: String {
        if ProcessInfo.processInfo.isOperatingSystemAtLeast(
            OperatingSystemVersion(majorVersion: 27, minorVersion: 0, patchVersion: 0)
        ) {
            return "Device Control and Data Access"
        }
        return "Accessibility"
    }

    nonisolated static var settingsPath: String {
        "System Settings › Privacy & Security › \(paneName)"
    }

    func startMonitoring() {
        guard timer == nil else { return }
        refresh()
        // Poll quickly while waiting for a grant, slowly afterwards so a
        // revoked grant still shuts the event tap down promptly.
        let timer = Timer(timeInterval: 1.0, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
        timer.tolerance = 0.3
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private var ticksSinceSlowCheck = 0

    private func tick() {
        if isTrusted {
            ticksSinceSlowCheck += 1
            guard ticksSinceSlowCheck >= 3 else { return }
        }
        ticksSinceSlowCheck = 0
        refresh()
    }

    func refresh() {
        let trusted = AXIsProcessTrusted()
        if trusted != isTrusted {
            isTrusted = trusted
        }
        let secure = IsSecureEventInputEnabled()
        if secure != isSecureInputActive {
            isSecureInputActive = secure
        }
    }

    /// Shows the system prompt (which also adds SwipeType to the list in
    /// System Settings) and opens the right pane.
    func requestAccess() {
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue(): true] as CFDictionary
        if !AXIsProcessTrustedWithOptions(options) {
            openSettings()
        }
        refresh()
    }

    func openSettings() {
        let candidates = [
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility",
            "x-apple.systempreferences:com.apple.preference.security",
        ]
        for candidate in candidates {
            if let url = URL(string: candidate), NSWorkspace.shared.open(url) {
                return
            }
        }
    }

    /// Clears a stale entry (for example one left behind by a previous build,
    /// which shows as enabled but no longer matches this copy of the app) and
    /// asks again.
    func resetAndRequestAccess() {
        guard let bundleID = Bundle.main.bundleIdentifier else {
            requestAccess()
            return
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/tccutil")
        process.arguments = ["reset", "Accessibility", bundleID]
        process.terminationHandler = { _ in
            Task { @MainActor in
                PermissionManager.shared.requestAccess()
            }
        }
        do {
            try process.run()
        } catch {
            requestAccess()
        }
    }
}
