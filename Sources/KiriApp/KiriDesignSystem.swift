import CoreGraphics

enum KiriUI {
    enum Spacing {
        static let compact: CGFloat = 8
        static let standard: CGFloat = 12
        static let roomy: CGFloat = 18
        static let page: CGFloat = 22
    }

    enum Radius {
        static let control: CGFloat = 8
        static let preview: CGFloat = 10
        static let card: CGFloat = 14
        static let surface: CGFloat = 20
    }

    enum Motion {
        static let hover = 0.16
        static let feedback = 0.18
    }
}
