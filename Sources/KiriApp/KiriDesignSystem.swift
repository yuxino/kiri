import AppKit
import CoreGraphics
import SwiftUI

enum KiriUI {
    enum Spacing {
        static let tight: CGFloat = 6
        static let compact: CGFloat = 10
        static let standard: CGFloat = 14
        static let roomy: CGFloat = 20
        static let page: CGFloat = 24
    }

    enum Radius {
        static let control: CGFloat = 11
        static let badge: CGFloat = 9
        static let preview: CGFloat = 14
        static let card: CGFloat = 18
        static let surface: CGFloat = 24
    }

    enum Header {
        static let searchWidth: CGFloat = 228
        static let sectionPickerWidth: CGFloat = 176
        static let controlHeight: CGFloat = 36
    }

    enum Card {
        static let thumbnailHeight: CGFloat = 184
        static let padding: CGFloat = 12
        static let actionSpacing: CGFloat = 8
        static let metadataSpacing: CGFloat = 7
    }

    enum Motion {
        static let hover = 0.14
        static let feedback = 0.20
    }

    enum Palette {
        static let accent = Color(nsColor: CaptureUIColors.accent)
        static let accentStrong = Color(nsColor: CaptureUIColors.accentStrong)
        static let cyan = Color(nsColor: CaptureUIColors.cyan)
        static let coral = Color(nsColor: CaptureUIColors.blossom)
        static let canvas = Color(nsColor: CaptureUIColors.canvas)
        static let card = Color(nsColor: CaptureUIColors.card)
        static let elevated = Color(nsColor: CaptureUIColors.elevated)
        static let border = Color(nsColor: CaptureUIColors.surfaceBorder)
    }

    static let brandGradient = LinearGradient(
        colors: [Palette.accentStrong, Palette.accent, Palette.cyan],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )

    static let warmGradient = LinearGradient(
        colors: [Palette.coral, Palette.accent],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )
}

@MainActor
private enum KiriBrandArtwork {
    static let image: NSImage? = {
        if let path = ProcessInfo.processInfo.environment["KIRI_BRAND_ICON_PATH"],
           let image = NSImage(contentsOfFile: path) {
            return image
        }
        if let url = Bundle.main.url(forResource: "kiri-icon", withExtension: "png"),
           let image = NSImage(contentsOf: url) {
            return image
        }
        return NSImage(named: NSImage.applicationIconName)
    }()
}

struct KiriBrandMark: View {
    var size: CGFloat = 38

    var body: some View {
        Group {
            if let image = KiriBrandArtwork.image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .scaledToFill()
            } else {
                Image(systemName: "viewfinder")
                    .font(.system(size: size * 0.42, weight: .bold))
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(KiriUI.brandGradient)
            }
        }
            .frame(width: size, height: size)
            .clipShape(RoundedRectangle(cornerRadius: size * 0.3))
            .overlay {
                RoundedRectangle(cornerRadius: size * 0.3)
                    .stroke(KiriUI.Palette.border.opacity(0.9), lineWidth: 1)
            }
            .shadow(color: KiriUI.Palette.accent.opacity(0.24), radius: 10, y: 4)
            .accessibilityHidden(true)
    }
}

struct KiriSymbolMark: View {
    let symbol: String
    var size: CGFloat = 38

    var body: some View {
        Image(systemName: symbol)
            .font(.system(size: size * 0.42, weight: .bold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(KiriUI.brandGradient, in: RoundedRectangle(cornerRadius: size * 0.3))
            .overlay {
                RoundedRectangle(cornerRadius: size * 0.3)
                    .stroke(.white.opacity(0.24), lineWidth: 1)
            }
            .shadow(color: KiriUI.Palette.accent.opacity(0.24), radius: 10, y: 4)
            .accessibilityHidden(true)
    }
}

struct KiriPrimaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12.5, weight: .semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 14)
            .frame(minHeight: 36)
            .background(KiriUI.brandGradient, in: RoundedRectangle(cornerRadius: KiriUI.Radius.control))
            .overlay {
                RoundedRectangle(cornerRadius: KiriUI.Radius.control)
                    .stroke(.white.opacity(0.2), lineWidth: 1)
            }
            .shadow(
                color: KiriUI.Palette.accent.opacity(configuration.isPressed ? 0.12 : 0.24),
                radius: configuration.isPressed ? 4 : 10,
                y: configuration.isPressed ? 1 : 4
            )
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
            .saturation(isEnabled ? 1 : 0.18)
            .opacity(isEnabled ? (configuration.isPressed ? 0.92 : 1) : 0.48)
            .animation(.easeOut(duration: KiriUI.Motion.hover), value: configuration.isPressed)
            .animation(.easeOut(duration: KiriUI.Motion.hover), value: isEnabled)
    }
}

struct KiriSurfaceModifier: ViewModifier {
    var radius: CGFloat = KiriUI.Radius.card
    var elevated = false

    func body(content: Content) -> some View {
        content
            .background(elevated ? KiriUI.Palette.elevated : KiriUI.Palette.card)
            .clipShape(RoundedRectangle(cornerRadius: radius))
            .overlay {
                RoundedRectangle(cornerRadius: radius)
                    .stroke(KiriUI.Palette.border, lineWidth: 1)
            }
            .shadow(color: .black.opacity(elevated ? 0.10 : 0.045), radius: elevated ? 18 : 8, y: elevated ? 8 : 3)
    }
}

extension View {
    func kiriSurface(radius: CGFloat = KiriUI.Radius.card, elevated: Bool = false) -> some View {
        modifier(KiriSurfaceModifier(radius: radius, elevated: elevated))
    }
}
