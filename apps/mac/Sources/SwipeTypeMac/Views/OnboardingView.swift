//
//  OnboardingView.swift
//  SwipeTypeMac
//
//  First-run setup guide: what SwipeType does, the permission it needs,
//  picking a shortcut, and a place to try it.
//

import SwiftUI

struct OnboardingView: View {
    static let windowSize = CGSize(width: 560, height: 540)

    let onFinish: () -> Void

    @ObservedObject private var permissions = PermissionManager.shared
    @ObservedObject private var appState = AppState.shared

    @AppStorage(AppSettings.Keys.hotkeyPreset) private var hotkeyPresetRaw = AppSettings.Defaults.hotkeyPreset.rawValue
    @AppStorage(AppSettings.Keys.customToggleHotkeyKeyCode) private var customToggleHotkeyKeyCode = AppSettings.Defaults.customToggleHotkeyKeyCode
    @AppStorage(AppSettings.Keys.customToggleHotkeyModifiers) private var customToggleHotkeyModifiers = AppSettings.Defaults.customToggleHotkeyModifiers
    @AppStorage(AppSettings.Keys.showMenuBarItem) private var showMenuBarItem = AppSettings.Defaults.showMenuBarItem

    @State private var step: Step = .welcome
    @State private var practiceText = ""

    enum Step: Int, CaseIterable {
        case welcome
        case permission
        case shortcut
        case practice
        case done
    }

    private var hotkeyPreset: AppSettings.ToggleHotkeyPreset {
        AppSettings.ToggleHotkeyPreset(rawValue: hotkeyPresetRaw) ?? AppSettings.Defaults.hotkeyPreset
    }

    private var hotkeySymbol: String? {
        switch hotkeyPreset {
        case .none:
            return nil
        case .custom:
            guard customToggleHotkeyModifiers != 0 else { return nil }
            return AppSettings.hotkeyHintSymbol(keyCode: customToggleHotkeyKeyCode, modifierMask: customToggleHotkeyModifiers)
        default:
            return hotkeyPreset.hintSymbol
        }
    }

    private var isMenuBarRequired: Bool {
        hotkeySymbol == nil
    }

    var body: some View {
        VStack(spacing: 0) {
            Group {
                switch step {
                case .welcome: welcomeStep
                case .permission: permissionStep
                case .shortcut: shortcutStep
                case .practice: practiceStep
                case .done: doneStep
                }
            }
            .id(step)
            .transition(.opacity)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(.horizontal, 36)
            .padding(.top, 40)

            Divider()

            footer
                .padding(.horizontal, 24)
                .padding(.vertical, 14)
        }
        .frame(width: Self.windowSize.width, height: Self.windowSize.height)
        .onChange(of: permissions.isTrusted) { granted in
            // The user is usually in System Settings at this point; bring the
            // guide back so they can carry on.
            if granted && step == .permission {
                NotificationCenter.default.post(name: .openOnboarding, object: nil)
            }
        }
        .onChange(of: hotkeyPresetRaw) { _ in
            if isMenuBarRequired {
                showMenuBarItem = true
            }
        }
    }

    // MARK: - Footer

    private var footer: some View {
        HStack(spacing: 10) {
            StepIndicator(count: Step.allCases.count, current: step.rawValue)

            Spacer()

            if step != .welcome && step != .done {
                Button("Back") { go(to: step.rawValue - 1) }
                    .controlSize(.large)
            }

            if step == .permission && !permissions.isTrusted {
                Button("Skip for Now") { go(to: step.rawValue + 1) }
                    .controlSize(.large)
            }

            Button(primaryTitle) {
                if step == .done {
                    onFinish()
                } else {
                    go(to: step.rawValue + 1)
                }
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            // Return belongs to the overlay while practicing.
            .keyboardShortcut(step == .practice ? nil : KeyboardShortcut.defaultAction)
            .disabled(step == .permission && !permissions.isTrusted)
        }
    }

    private var primaryTitle: String {
        switch step {
        case .welcome: return "Get Started"
        case .done: return "Start Swiping"
        default: return "Continue"
        }
    }

    private func go(to index: Int) {
        guard let next = Step(rawValue: index) else { return }
        withAnimation(.easeInOut(duration: 0.2)) {
            step = next
        }
    }

    // MARK: - Steps

    private var welcomeStep: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                Spacer()
                OverlayHero()
                Spacer()
            }

            StepHeader(
                title: "Welcome to SwipeType",
                subtitle: "Sweep across your Mac's keyboard instead of typing every letter. SwipeType works out the word and types it into whatever app you're using."
            )

            OnboardingCard {
                VStack(alignment: .leading, spacing: 12) {
                    FeatureRow(icon: "hand.draw", title: "Swipe", detail: "Open the overlay, then run across the letters of a word in one motion.")
                    FeatureRow(icon: "list.number", title: "Pick", detail: "Return types the top guess, 1–5 picks another.")
                    FeatureRow(icon: "text.cursor", title: "Anywhere", detail: "Works in any text field, all on your Mac.")
                }
            }
        }
    }

    private var permissionStep: some View {
        VStack(alignment: .leading, spacing: 18) {
            StepHeader(
                icon: "lock.shield",
                title: "Allow keyboard access",
                subtitle: "macOS asks before any app can read the keyboard or type for you. SwipeType needs \(PermissionManager.paneName) access to do both."
            )

            OnboardingCard {
                VStack(alignment: .leading, spacing: 12) {
                    FeatureRow(icon: "eye", title: "Only what it needs", detail: "Keys are read for your shortcut and while the overlay is open. Everything else passes straight through.")
                    FeatureRow(icon: "lock.icloud", title: "Stays on your Mac", detail: "Predictions run locally. Nothing is sent anywhere.")
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                NumberedRow(number: 1, text: "Click Grant Access below.")
                NumberedRow(number: 2, text: "Turn on SwipeType in \(PermissionManager.settingsPath).")
                NumberedRow(number: 3, text: "Come back here. This page updates on its own.")
            }
            .padding(.horizontal, 4)

            if permissions.isTrusted {
                PermissionStatusCard(permissions: permissions)
            } else {
                HStack(alignment: .top, spacing: 12) {
                    Button {
                        permissions.requestAccess()
                    } label: {
                        Label("Grant Access", systemImage: "hand.raised")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.large)

                    PermissionStatusCard(permissions: permissions)
                }
            }
        }
    }

    private var shortcutStep: some View {
        VStack(alignment: .leading, spacing: 18) {
            StepHeader(
                icon: "keyboard",
                title: "Choose a shortcut",
                subtitle: "Press it in any app to show or hide the overlay."
            )

            OnboardingCard {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(AppSettings.ToggleHotkeyPreset.allCases) { preset in
                        ShortcutRow(
                            title: preset == .custom ? "Custom" : preset.displayName,
                            symbol: symbol(for: preset),
                            isSelected: preset == hotkeyPreset
                        ) {
                            hotkeyPresetRaw = preset.rawValue
                        }
                    }

                    Text(shortcutNote)
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 4)
                        .padding(.horizontal, 8)
                }
            }

            OnboardingCard {
                VStack(alignment: .leading, spacing: 2) {
                    Toggle("Show menu bar icon", isOn: $showMenuBarItem)
                        .disabled(isMenuBarRequired)
                    Text(isMenuBarRequired
                         ? "Required without a shortcut: it's how you open the overlay."
                         : "Quick access to the overlay, settings and this guide. Hidden? Open SwipeType again to reach Settings.")
                        .font(.system(size: 9))
                        .foregroundStyle(.secondary.opacity(0.8))
                }
            }
        }
    }

    private var shortcutNote: String {
        switch hotkeyPreset {
        case .shiftTab:
            return "Shift+Tab normally moves focus backwards. While SwipeType is running it opens the overlay instead."
        case .optionTab:
            return "Option+Tab is rarely used by other apps."
        case .controlOptionSpace:
            return "Control+Option+Space doesn't clash with common shortcuts, but check your input source switcher."
        case .custom:
            return "Pick the key and modifiers for a custom shortcut in Settings."
        case .none:
            return "No shortcut. Open the overlay from the menu bar icon."
        }
    }

    private func symbol(for preset: AppSettings.ToggleHotkeyPreset) -> String {
        switch preset {
        case .custom:
            guard customToggleHotkeyModifiers != 0 else { return "—" }
            return AppSettings.hotkeyHintSymbol(keyCode: customToggleHotkeyKeyCode, modifierMask: customToggleHotkeyModifiers)
        case .none:
            return "—"
        default:
            return preset.hintSymbol
        }
    }

    private var practiceStep: some View {
        VStack(alignment: .leading, spacing: 18) {
            StepHeader(
                icon: "hand.draw",
                title: "Give it a try",
                subtitle: "Run across the letters without lifting off the keys, as if you were drawing the word."
            )

            OnboardingCard {
                VStack(alignment: .leading, spacing: 10) {
                    NumberedRow(number: 1, text: "Click in the box below.")
                    HStack(spacing: 6) {
                        NumberedRow(number: 2, text: hotkeySymbol == nil ? "Choose Toggle Overlay from the menu bar icon." : "Press")
                        if let hotkeySymbol {
                            Keycap(hotkeySymbol)
                            Text("to open the overlay.")
                                .font(.system(size: 12))
                        }
                    }
                    NumberedRow(number: 3, text: "Swipe a word, say “hello”: start on H, sweep to E, over to L, up to O.")
                    HStack(spacing: 6) {
                        NumberedRow(number: 4, text: "Press")
                        Keycap("↵")
                        Text("to type the top guess, or")
                            .font(.system(size: 12))
                        Keycap("⎋")
                        Text("to close.")
                            .font(.system(size: 12))
                    }
                }
            }

            TextField("Swiped words show up here…", text: $practiceText, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .font(.system(size: 15))
                .lineLimit(3...4)

            if let warning = practiceWarning {
                Label(warning, systemImage: "exclamationmark.triangle.fill")
                    .font(.system(size: 11))
                    .foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var practiceWarning: String? {
        if !permissions.isTrusted {
            return "\(PermissionManager.paneName) access is still off, so the overlay can't read keys. Go back a step to grant it."
        }
        if permissions.isSecureInputActive {
            return "Another app has secure input on (a password field or Terminal's Secure Keyboard Entry). Keys are hidden from SwipeType until it's off."
        }
        if appState.isDictionaryLoading {
            return "Still loading the dictionary…"
        }
        if !appState.isDictionaryLoaded {
            return "The dictionary failed to load. Try reinstalling SwipeType."
        }
        return nil
    }

    private var doneStep: some View {
        VStack(alignment: .leading, spacing: 18) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 44))
                .foregroundStyle(Color.accentColor)

            StepHeader(
                title: "You're all set",
                subtitle: "A few keys worth knowing while the overlay is open:"
            )

            OnboardingCard {
                VStack(alignment: .leading, spacing: 10) {
                    TipRow(key: hotkeySymbol ?? "Menu", text: "Show or hide the overlay")
                    TipRow(key: "1–5", text: "Type a specific prediction")
                    TipRow(key: "⌫", text: "Clear the current swipe")
                    TipRow(key: "⎋", text: "Close the overlay")
                }
            }

            Text(showMenuBarItem
                 ? "Settings and this guide live in the menu bar icon."
                 : "To get back to Settings, open SwipeType again from Applications or Spotlight.")
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
        }
    }
}

// MARK: - Components

private struct StepHeader: View {
    var icon: String? = nil
    let title: String
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let icon {
                Image(systemName: icon)
                    .font(.system(size: 22, weight: .semibold))
                    .foregroundStyle(Color.accentColor)
                    .padding(.bottom, 4)
            }
            Text(title)
                .font(.system(size: 22, weight: .bold, design: .rounded))
            Text(subtitle)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// Same card treatment as the sections in Settings.
private struct OnboardingCard<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            content()
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.primary.opacity(0.03))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.primary.opacity(0.05), lineWidth: 1))
    }
}

private struct FeatureRow: View {
    let icon: String
    let title: String
    let detail: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: icon)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Color.accentColor)
                .frame(width: 18)
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.system(size: 12, weight: .semibold))
                Text(detail)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

private struct NumberedRow: View {
    let number: Int
    let text: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            // Matches the numbered prediction badges in the overlay.
            Text("\(number)")
                .font(.system(size: 10, weight: .bold))
                .foregroundColor(.white)
                .frame(width: 17, height: 17)
                .background(Color.accentColor)
                .clipShape(Circle())
                .alignmentGuide(.firstTextBaseline) { d in d[.bottom] - 4.5 }
            Text(text)
                .font(.system(size: 12))
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// Key symbol chip, styled like the shortcut preview in Settings.
private struct Keycap: View {
    let symbol: String

    init(_ symbol: String) {
        self.symbol = symbol
    }

    var body: some View {
        Text(symbol)
            .font(.system(size: 12, design: .monospaced))
            .fontWeight(.semibold)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(Color.primary.opacity(0.06))
            .clipShape(RoundedRectangle(cornerRadius: 4))
    }
}

private struct TipRow: View {
    let key: String
    let text: String

    var body: some View {
        HStack(spacing: 10) {
            Keycap(key)
                .frame(minWidth: 52, alignment: .leading)
            Text(text)
                .font(.system(size: 12))
        }
    }
}

private struct ShortcutRow: View {
    let title: String
    let symbol: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: isSelected ? "largecircle.fill.circle" : "circle")
                    .foregroundStyle(isSelected ? Color.accentColor : Color.secondary)
                Text(title)
                    .font(.system(size: 12, weight: isSelected ? .semibold : .regular))
                Spacer()
                Keycap(symbol)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(isSelected ? Color.accentColor.opacity(0.12) : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

private struct StepIndicator: View {
    let count: Int
    let current: Int

    var body: some View {
        HStack(spacing: 5) {
            ForEach(0..<count, id: \.self) { index in
                Capsule()
                    .fill(index == current ? Color.accentColor : Color.primary.opacity(0.15))
                    .frame(width: index == current ? 18 : 6, height: 6)
            }
        }
        .animation(.easeInOut(duration: 0.2), value: current)
    }
}

/// Miniature of the overlay, reusing the animated preview from Settings.
private struct OverlayHero: View {
    var body: some View {
        VStack(spacing: 4) {
            VStack(spacing: 3) {
                MockRow(text: "hello", primary: true)
                MockRow(text: "help", primary: false)
            }

            MockKeyboardView(input: "hgfdertyuiklo")
                .frame(height: 52)
        }
        .padding(12)
        .frame(width: 200)
        .background(
            ZStack {
                VisualEffectView(material: .hudWindow, blendingMode: .withinWindow, emphasized: false)
                Color.black.opacity(0.72)
            }
        )
        .clipShape(RoundedRectangle(cornerRadius: OverlayLayout.containerCornerRadius))
        .overlay(
            RoundedRectangle(cornerRadius: OverlayLayout.containerCornerRadius)
                .stroke(Color.white.opacity(0.08), lineWidth: 0.5)
        )
        .shadow(color: .black.opacity(0.25), radius: 12, x: 0, y: 6)
        .environment(\.colorScheme, .dark)
    }
}
