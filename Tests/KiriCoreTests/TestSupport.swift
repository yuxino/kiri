import Foundation

struct TestFailure: Error, CustomStringConvertible {
    let description: String
}

func expect(
    _ condition: @autoclosure () throws -> Bool,
    _ message: @autoclosure () -> String,
    file: StaticString = #filePath,
    line: UInt = #line
) throws {
    guard try condition() else {
        throw TestFailure(description: "\(file):\(line): \(message())")
    }
}

struct KiriTest {
    let name: String
    let run: () async throws -> Void
}

func runTests(_ tests: [KiriTest]) async throws {
    var failures: [String] = []
    for test in tests {
        do {
            try await test.run()
            testLog("✓ \(test.name)")
        } catch {
            failures.append("✗ \(test.name): \(error)")
            testLog(failures.last!)
        }
    }
    guard failures.isEmpty else {
        throw TestFailure(description: "\(failures.count) test(s) failed")
    }
    testLog("\n\(tests.count) tests passed")
}

func testLog(_ message: String) {
    FileHandle.standardOutput.write(Data("\(message)\n".utf8))
}
