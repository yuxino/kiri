public struct AnnotationHistory<Element> {
    private var visibleElements: [Element] = []
    private var redoElements: [Element] = []

    public init() {}

    public var elements: [Element] {
        visibleElements
    }

    public var canUndo: Bool {
        !visibleElements.isEmpty
    }

    public var canRedo: Bool {
        !redoElements.isEmpty
    }

    public mutating func append(_ element: Element) {
        visibleElements.append(element)
        redoElements.removeAll(keepingCapacity: true)
    }

    @discardableResult
    public mutating func undo() -> Element? {
        guard let element = visibleElements.popLast() else { return nil }
        redoElements.append(element)
        return element
    }

    @discardableResult
    public mutating func redo() -> Element? {
        guard let element = redoElements.popLast() else { return nil }
        visibleElements.append(element)
        return element
    }

    public mutating func clear() {
        visibleElements.removeAll(keepingCapacity: true)
        redoElements.removeAll(keepingCapacity: true)
    }
}
