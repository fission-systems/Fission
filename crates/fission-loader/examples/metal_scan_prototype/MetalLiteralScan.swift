import Foundation
import Metal

enum Failure: Error { case message(String) }
func now() -> UInt64 { DispatchTime.now().uptimeNanoseconds }
func milliseconds(_ start: UInt64) -> Double { Double(now() - start) / 1_000_000 }

final class Scanner {
    let device: MTLDevice
    let queue: MTLCommandQueue
    let pipeline: MTLComputePipelineState
    let patternBytes: MTLBuffer
    let metadata: MTLBuffer
    let buckets: MTLBuffer
    let indices: MTLBuffer
    let count: Int

    static func buffer<T>(_ values: [T], on device: MTLDevice) throws -> MTLBuffer {
        try values.withUnsafeBufferPointer { pointer in
            let length = pointer.count * MemoryLayout<T>.stride
            let result: MTLBuffer?
            if length == 0 {
                result = device.makeBuffer(length: 1, options: .storageModeShared)
            } else {
                result = device.makeBuffer(bytes: pointer.baseAddress!, length: length, options: .storageModeShared)
            }
            guard let result else { throw Failure.message("Metal buffer allocation failed") }
            return result
        }
    }

    init(patterns: [String], shader: URL) throws {
        guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else {
            throw Failure.message("Metal device/queue unavailable; production CPU path is independent")
        }
        self.device = device
        self.queue = queue
        let source = try String(contentsOf: shader, encoding: .utf8)
        let library = try device.makeLibrary(source: source, options: nil)
        guard let function = library.makeFunction(name: "literal_presence") else {
            throw Failure.message("missing shader function")
        }
        pipeline = try device.makeComputePipelineState(function: function)
        count = patterns.count
        guard count <= Int(UInt32.max) else { throw Failure.message("pattern count exceeds UInt32") }
        var bytes = [UInt8](), meta = [UInt32]()
        var groups = Array(repeating: [UInt32](), count: 256)
        for (id, string) in patterns.enumerated() {
            let value = Array(string.utf8)
            guard !value.isEmpty else { throw Failure.message("empty literal; filter it before invoking prototype") }
            guard bytes.count <= Int(UInt32.max) - value.count else {
                throw Failure.message("pattern byte pool exceeds UInt32")
            }
            meta += [UInt32(bytes.count), UInt32(value.count)]
            bytes += value
            groups[Int(value[0])].append(UInt32(id))
        }
        var heads = [UInt32](), order = [UInt32]()
        for group in groups {
            heads += [UInt32(order.count), UInt32(group.count)]
            order += group
        }
        patternBytes = try Scanner.buffer(bytes, on: device)
        metadata = try Scanner.buffer(meta, on: device)
        buckets = try Scanner.buffer(heads, on: device)
        indices = try Scanner.buffer(order, on: device)
    }

    func inputBuffer(_ data: Data) throws -> MTLBuffer {
        guard data.count <= Int(UInt32.max), data.count <= device.maxBufferLength else {
            throw Failure.message("input exceeds single-dispatch prototype buffer bound")
        }
        return try data.withUnsafeBytes { bytes in
            let result: MTLBuffer?
            if data.isEmpty { result = device.makeBuffer(length: 1, options: .storageModeShared) }
            else { result = device.makeBuffer(bytes: bytes.baseAddress!, length: bytes.count, options: .storageModeShared) }
            guard let result else { throw Failure.message("input buffer allocation failed") }
            return result
        }
    }

    func scan(input: MTLBuffer, length: Int, result: MTLBuffer) throws -> [String: Any] {
        let start = now()
        memset(result.contents(), 0, max(1, count * MemoryLayout<UInt32>.stride))
        var gpuTime = 0.0
        if length > 0 && count > 0 {
            guard let command = queue.makeCommandBuffer(), let encoder = command.makeComputeCommandEncoder() else {
                throw Failure.message("command/encoder unavailable")
            }
            encoder.setComputePipelineState(pipeline)
            for (index, buffer) in [input, patternBytes, metadata, buckets, indices, result].enumerated() {
                encoder.setBuffer(buffer, offset: 0, index: index)
            }
            var size = UInt32(length)
            encoder.setBytes(&size, length: MemoryLayout<UInt32>.stride, index: 6)
            let width = min(256, pipeline.maxTotalThreadsPerThreadgroup)
            encoder.dispatchThreads(MTLSize(width: length, height: 1, depth: 1),
                threadsPerThreadgroup: MTLSize(width: width, height: 1, depth: 1))
            encoder.endEncoding()
            command.commit()
            command.waitUntilCompleted()
            guard command.status == .completed else {
                throw Failure.message("GPU command failed: \(String(describing: command.error))")
            }
            gpuTime = max(0, command.gpuEndTime - command.gpuStartTime) * 1000
        }
        let pointer = result.contents().bindMemory(to: UInt32.self, capacity: count)
        let found = (0..<count).map { pointer[$0] != 0 }
        return ["scan_ms": milliseconds(start), "gpu_execution_ms": gpuTime, "found": found]
    }
}

do {
    let total = now()
    let args = Array(CommandLine.arguments.dropFirst())
    guard args.count == 5, let iterations = Int(args[2]), iterations > 0,
          ["upload", "resident"].contains(args[3]) else {
        throw Failure.message("usage: metal-literal-scan PATTERNS.json INPUT ITERATIONS upload|resident SHADER.metal")
    }
    let read = now()
    let patterns = try JSONDecoder().decode([String].self, from: Data(contentsOf: URL(fileURLWithPath: args[0])))
    let data = try Data(contentsOf: URL(fileURLWithPath: args[1]))
    let readMS = milliseconds(read)
    let setup = now()
    let scanner = try Scanner(patterns: patterns, shader: URL(fileURLWithPath: args[4]))
    let setupMS = milliseconds(setup)
    let initial = now()
    let resident = args[3] == "resident" ? try scanner.inputBuffer(data) : nil
    guard let results = scanner.device.makeBuffer(length: max(1, patterns.count * 4), options: .storageModeShared) else {
        throw Failure.message("result allocation failed")
    }
    let initialBufferMS = milliseconds(initial)
    var scans = [[String: Any]]()
    for _ in 0..<iterations {
        let row: [String: Any] = try autoreleasepool {
            let start = now()
            let prepare = now()
            let buffer = try resident ?? scanner.inputBuffer(data)
            let prepareMS = milliseconds(prepare)
            var row = try scanner.scan(input: buffer, length: data.count, result: results)
            row["prepare_ms"] = prepareMS
            row["operation_ms"] = milliseconds(start)
            return row
        }
        scans.append(row)
    }
    let report: [String: Any] = ["backend": "metal", "mode": args[3], "device": scanner.device.name,
        "unified_memory": scanner.device.hasUnifiedMemory, "bytes": data.count, "patterns": patterns.count,
        "read_ms": readMS, "setup_ms": setupMS, "initial_buffer_ms": initialBufferMS,
        "total_ms": milliseconds(total), "scans": scans]
    let encoded = try JSONSerialization.data(withJSONObject: report, options: [.sortedKeys])
    FileHandle.standardOutput.write(encoded)
    FileHandle.standardOutput.write(Data([10]))
} catch {
    FileHandle.standardError.write(Data("\(error)\n".utf8))
    exit(1)
}
