import CoreGraphics

enum KiriUI {
    enum Spacing {
        static let compact: CGFloat = 8
        static let standard: CGFloat = 12
        static let roomy: CGFloat = 18
        static let page: CGFloat = 22
    }

    enum Radius {
        static let control: CGFloat = 10
        static let badge: CGFloat = 8
        static let preview: CGFloat = 12
        static let card: CGFloat = 16
        static let surface: CGFloat = 22
    }

    enum Header {
        static let searchWidth: CGFloat = 210
        static let sectionPickerWidth: CGFloat = 164
        static let controlHeight: CGFloat = 32
    }

    enum Card {
        static let thumbnailHeight: CGFloat = 190
        static let padding: CGFloat = 14
        static let actionSpacing: CGFloat = 6
        static let metadataSpacing: CGFloat = 6
    }

    enum Motion {
        static let hover = 0.16
        static let feedback = 0.18
    }
}
