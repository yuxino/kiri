private struct AnnotationHistoryStep<Element> {
    let before: [Element]
    let after: [Element]
    let undoResult: Element?
    let redoResult: Element?
}

public struct AnnotationHistory<Element> {
    private var visibleElements: [Element] = []
    private var undoSteps: [AnnotationHistoryStep<Element>] = []
    private var redoSteps: [AnnotationHistoryStep<Element>] = []

    public init() {}

    public var elements: [Element] {
        visibleElements
    }

    public var canUndo: Bool {
        !undoSteps.isEmpty
    }

    public var canRedo: Bool {
        !redoSteps.isEmpty
    }

    public mutating func append(_ element: Element) {
        let before = visibleElements
        visibleElements.append(element)
        recordStep(
            before: before,
            undoResult: element,
            redoResult: element
        )
    }

    @discardableResult
    public mutating func replace(at index: Int, with element: Element) -> Element? {
        guard visibleElements.indices.contains(index) else { return nil }
        let before = visibleElements
        let previous = visibleElements[index]
        visibleElements[index] = element
        recordStep(
            before: before,
            undoResult: element,
            redoResult: element
        )
        return previous
    }

    @discardableResult
    public mutating func remove(at index: Int) -> Element? {
        guard visibleElements.indices.contains(index) else { return nil }
        let before = visibleElements
        let removed = visibleElements.remove(at: index)
        recordStep(
            before: before,
            undoResult: removed,
            redoResult: removed
        )
        return removed
    }

    @discardableResult
    public mutating func undo() -> Element? {
        guard let step = undoSteps.popLast() else { return nil }
        visibleElements = step.before
        redoSteps.append(step)
        return step.undoResult
    }

    @discardableResult
    public mutating func redo() -> Element? {
        guard let step = redoSteps.popLast() else { return nil }
        visibleElements = step.after
        undoSteps.append(step)
        return step.redoResult
    }

    public mutating func clear() {
        visibleElements.removeAll(keepingCapacity: true)
        undoSteps.removeAll(keepingCapacity: true)
        redoSteps.removeAll(keepingCapacity: true)
    }

    private mutating func recordStep(
        before: [Element],
        undoResult: Element?,
        redoResult: Element?
    ) {
        undoSteps.append(
            AnnotationHistoryStep(
                before: before,
                after: visibleElements,
                undoResult: undoResult,
                redoResult: redoResult
            )
        )
        redoSteps.removeAll(keepingCapacity: true)
    }
}
