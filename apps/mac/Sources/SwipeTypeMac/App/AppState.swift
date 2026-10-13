import Foundation
import SwiftUI

@MainActor
final class AppState: ObservableObject {
    static let shared = AppState()

    // Core state
    @Published var predictions: [Prediction] = []
    @Published var currentInput: String = ""
    @Published var isOverlayVisible = false
    @Published var isWordCommitted = false  // True after debounce timeout
    @Published var inputTimestamps: [TimeInterval] = []
    @Published var isPlaybackActive = false
    @Published var playbackStartTime: TimeInterval = 0

    // Stats
    @Published var predictionTime: TimeInterval = 0
    @Published var actualWPM: Int = 0
    @Published var isDictionaryLoaded = false
    @Published var isDictionaryLoading = false
    @Published var dictionaryWordCount = 0

    private var requestId = 0
    private var debounceTimer: Timer?
    private var debounceDelay: TimeInterval { AppSettings.debounceDelay }

    private init() {}

    /// Builds the engine index off the main thread; the event tap runs on the
    /// main run loop and macOS disables taps that stall for too long.
    func loadDictionary() {
        guard !isDictionaryLoading else { return }
        isDictionaryLoading = true
        DispatchQueue.global(qos: .userInitiated).async {
            let wordCount = SwipeEngineBridge.shared.loadBundledDictionary()
            DispatchQueue.main.async { [weak self] in
                guard let self = self else { return }
                self.isDictionaryLoading = false
                self.isDictionaryLoaded = wordCount > 0
                self.dictionaryWordCount = max(wordCount, 0)
                if !self.currentInput.isEmpty {
                    self.triggerPrediction()
                }
            }
        }
    }

    func addCharacter(_ char: Character) -> String? {
        var wordToInsert: String? = nil


        if AppSettings.autoCommitAfterPause, isWordCommitted, let first = predictions.first {
            wordToInsert = first.word
            reset()
        }

        currentInput.append(char)
        inputTimestamps.append(Date().timeIntervalSinceReferenceDate)
        isPlaybackActive = false
        playbackStartTime = 0
        triggerPrediction()
        resetDebounceTimer()

        return wordToInsert
    }

    func deleteCharacter() {
        reset()
    }

    func selectPrediction(at index: Int) -> String? {
        guard index >= 0, index < predictions.count else { return nil }
        let word = predictions[index].word
        reset()
        return word
    }

    func reset() {
        requestId += 1
        currentInput = ""
        predictions = []
        isWordCommitted = false
        inputTimestamps = []
        isPlaybackActive = false
        playbackStartTime = 0
        cancelDebounceTimer()
    }

    func toggleOverlay() {
        isOverlayVisible.toggle()
        if !isOverlayVisible {
            reset()
        }
    }

    func hideOverlay() {
        isOverlayVisible = false
        reset()
    }

    private func resetDebounceTimer() {
        cancelDebounceTimer()
        isWordCommitted = false

        debounceTimer = Timer.scheduledTimer(withTimeInterval: debounceDelay, repeats: false) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self = self else { return }
                if !self.currentInput.isEmpty && !self.predictions.isEmpty {
                    self.isWordCommitted = true
                }
                if AppSettings.playSwipeAnimation, self.currentInput.count >= 2 {
                    self.isPlaybackActive = true
                    self.playbackStartTime = Date().timeIntervalSinceReferenceDate
                }
            }
        }
    }

    private func cancelDebounceTimer() {
        debounceTimer?.invalidate()
        debounceTimer = nil
    }

    private func triggerPrediction() {
        requestId += 1
        let thisRequest = requestId
        let input = currentInput

        if let punctuationPrediction = punctuationPrediction(for: input) {
            predictions = [Prediction(word: punctuationPrediction, score: 1.0, freq: 0.0)]
            predictionTime = 0
            updateWPM()
            return
        }

        let startTime = Date().timeIntervalSinceReferenceDate
        DispatchQueue.global(qos: .userInitiated).async {
            let results = SwipeEngineBridge.shared.predict(input: input, limit: 5)
            let endTime = Date().timeIntervalSinceReferenceDate
            let duration = endTime - startTime

            DispatchQueue.main.async { [weak self] in
                guard let self = self, self.requestId == thisRequest else { return }
                self.predictions = results
                self.predictionTime = duration
                self.updateWPM()
            }
        }
    }

    private func punctuationPrediction(for input: String) -> String? {
        guard input.count == 1, let first = input.first else { return nil }
        guard AppSettings.supportsInlinePunctuationCompletion(first) else { return nil }
        return String(first)
    }

    private func updateWPM() {
        guard let first = predictions.first,
              let start = inputTimestamps.first,
              let end = inputTimestamps.last else {
            actualWPM = 0
            return
        }

        let durationMs = (end - start) * 1000
        let minutes = durationMs / 60000.0
        if minutes > 0 {
            actualWPM = Int(round((Double(first.word.count) / 5.0) / minutes))
        } else {
            actualWPM = 0
        }
    }
}
