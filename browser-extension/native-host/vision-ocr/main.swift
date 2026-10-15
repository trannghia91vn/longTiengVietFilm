import Foundation
import Vision

do {
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = false
    request.automaticallyDetectsLanguage = false
    let languages = try request.supportedRecognitionLanguages()
    guard let vietnamese = languages.first(where: { $0.hasPrefix("vi-") }) else {
        throw NSError(domain: "LongTiengOCR", code: 1, userInfo: [NSLocalizedDescriptionKey: "Vietnamese recognition is not supported by this macOS Vision revision."])
    }
    request.recognitionLanguages = [vietnamese, "en-US"]
    request.minimumTextHeight = 0.055
    let result: Any
    if CommandLine.arguments.dropFirst().first == "--probe" {
        result = ["engine": "Apple Vision accurate", "language": vietnamese, "revision": request.revision, "system": ProcessInfo.processInfo.operatingSystemVersionString]
    } else {
        guard CommandLine.arguments.count == 2 else {
            throw NSError(domain: "LongTiengOCR", code: 2, userInfo: [NSLocalizedDescriptionKey: "Expected one local PNG path."])
        }
        let url = URL(fileURLWithPath: CommandLine.arguments[1]).standardizedFileURL
        let handler = VNImageRequestHandler(url: url, options: [:])
        try handler.perform([request])
        let rows = (request.results ?? []).filter { row in
            let box = row.boundingBox
            return box.height >= 0.055 && box.midX >= 0.15 && box.midX <= 0.85
        }.sorted { left, right in
            if abs(left.boundingBox.midY - right.boundingBox.midY) > 0.035 {
                return left.boundingBox.midY > right.boundingBox.midY
            }
            return left.boundingBox.minX < right.boundingBox.minX
        }
        var candidates: [[String: Any]] = []
        for rank in 0..<3 {
            let lines = rows.compactMap { row -> VNRecognizedText? in
                let choices = row.topCandidates(3)
                return choices.isEmpty ? nil : choices[min(rank, choices.count - 1)]
            }
            let text = lines.map(\.string).joined(separator: "\n").precomposedStringWithCanonicalMapping
            let confidence = lines.isEmpty ? 0.0 : lines.reduce(0.0) { $0 + Double($1.confidence) * 100 } / Double(lines.count)
            candidates.append(["text": text, "confidence": confidence, "variant": "vision-accurate-vi-candidate-\(rank + 1)"])
        }
        result = candidates
    }
    let data = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys, .withoutEscapingSlashes])
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data([10]))
} catch {
    FileHandle.standardError.write(Data("\(error.localizedDescription)\n".utf8))
    exit(1)
}
