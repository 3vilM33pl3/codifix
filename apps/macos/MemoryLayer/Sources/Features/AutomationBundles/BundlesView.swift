import AppKit
import SwiftUI
import UniformTypeIdentifiers

public struct BundlesView: View {
    public let context: FeatureContext
    @State private var options = ExportOptions()
    @State private var exportPreview: BundleExportPreview?
    @State private var exportSnapshot: ExportPreviewSource?
    @State private var importPreview: BundleImportPreview?
    @State private var selectedFileBytes: Data?
    @State private var selectedFileName: String?
    @State private var importSnapshot: ImportPreviewSource?
    @State private var isChoosingFile = false
    @State private var loading = false
    @State private var busy = false
    @State private var requestID = UUID()
    @State private var operationTask: Task<Void, Never>?
    @State private var operationTaskID = UUID()
    @State private var activeReview: BundleReviewSheet?
    @State private var presentedReview: BundleReviewSheet?
    @State private var actionError: String?
    @State private var operationWarning: String?
    @State private var status: String?

    public init(context: FeatureContext) { self.context = context }
    private var service: BundleService { BundleService(context: context) }
    private var canExport: Bool { context.permissions.allows(.bundleExport) }
    private var canImport: Bool { !context.permissions.readOnly && context.permissions.allows(.bundleImport) }
    private var canAccessBundles: Bool { canExport || canImport }
    private var operationActive: Bool { busy || operationTask != nil }
    private var exportIsCurrent: Bool { exportPreview != nil && exportSnapshot?.matches(options: options, project: context.profile.projectSlug, revision: context.revision) == true }
    private var importIsCurrent: Bool { importPreview != nil && importSnapshot?.matches(bytes: selectedFileBytes, project: context.profile.projectSlug, revision: context.revision) == true }

    public var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            ProjectPageHeader(title: "Memory bundles", subtitle: "Preview, export and import a versioned bundle · \(context.profile.projectSlug)") {
                Button("Clear previews") { activeReview = nil; clearVolatileState() }.disabled(operationActive)
            }
            if let actionError { ProjectBanner(title: "Bundle operation failed", message: actionError, tone: .failure) }
            if let operationWarning { ProjectBanner(title: "Service outcome needs review", message: operationWarning, tone: .warning) }
            if let status { ProjectBanner(title: "Bundle status", message: status, tone: .success).accessibilityElement(children: .ignore).accessibilityLabel("Bundle status. \(status)").accessibilityIdentifier("bundles.status") }
            if !canAccessBundles {
                ProjectLoadingState(context: context, title: "Access restricted", message: "This identity does not have bundle export or import permission for this project.", isLoading: false)
            } else {
                HSplitView {
                    exportPane.frame(minWidth: 340, idealWidth: 470, maxWidth: .infinity, maxHeight: .infinity)
                    importPane.frame(minWidth: 340, idealWidth: 470, maxWidth: .infinity, maxHeight: .infinity)
                }
            }
        }
        .padding(20)
        .fileImporter(isPresented: $isChoosingFile, allowedContentTypes: [.zip, .data], allowsMultipleSelection: false) { result in
            switch result {
            case let .success(urls):
                guard let url = urls.first else { status = "No bundle file was selected."; return }
                launchOperation { await loadSelectedFile(url) }
            case let .failure(error):
                if (error as? CocoaError)?.code == .userCancelled { status = "File selection was cancelled." }
                else { actionError = error.localizedDescription }
            }
        }
        .sheet(item: $activeReview, onDismiss: dismissReview) { sheet in reviewSheet(sheet) }
        .onDisappear { cancelOperation() }
        .task(id: context.revision) {
            cancelOperation()
            activeReview = nil; clearVolatileState(); busy = false; loading = false; requestID = UUID()
            actionError = nil; operationWarning = nil; status = nil
        }
    }

    private var exportPane: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                ProjectCard(title: "Export bundle") {
                    Text("Choose which optional data to include. Preview the exact options before saving the archive.").font(.caption).foregroundStyle(.secondary)
                    Toggle("Include archived memories", isOn: optionBinding(\.includeArchived)).disabled(operationActive)
                    Toggle("Include tags", isOn: optionBinding(\.includeTags)).disabled(operationActive)
                    Toggle("Include relations", isOn: optionBinding(\.includeRelations)).disabled(operationActive)
                    Toggle("Include source file paths", isOn: optionBinding(\.includeSourceFilePaths)).disabled(operationActive)
                    Toggle("Include git commit hashes", isOn: optionBinding(\.includeGitCommits)).disabled(operationActive)
                    Toggle("Include source excerpts", isOn: optionBinding(\.includeSourceExcerpts)).disabled(operationActive)
                    HStack {
                        Button("Preview export") { launchOperation { await previewExport() } }.buttonStyle(.borderedProminent).disabled(!canExport || operationActive).accessibilityIdentifier("bundles.previewExport")
                        Button("Review and save…") { activeReview = .export; presentedReview = .export }.disabled(!canExport || operationActive || !exportIsCurrent).accessibilityIdentifier("bundles.reviewExport")
                        if busy && loading { ProgressView().controlSize(.small) }
                    }
                }
                if let preview = exportPreview {
                    ProjectCard(title: "Export preview") {
                        Text("\(preview.memoryCount) memories · \(preview.relationCount) relations · \(preview.warningCount) warnings").font(.caption.weight(.medium)).foregroundStyle(.secondary)
                        Text("Bundle \(preview.bundleID) · source \(preview.sourceProject)").font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                        Text(preview.summaryMarkdown).font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                        warningList(preview.warnings)
                        if !exportIsCurrent { Text("Options or connection changed. Preview again before saving.").font(.caption).foregroundStyle(.orange) }
                    }
                } else {
                    ContentUnavailableView("Export preview required", systemImage: "square.and.arrow.up", description: Text("The service will calculate the bundle contents and warnings before a file is saved."))
                        .frame(maxWidth: .infinity, minHeight: 130)
                }
            }.padding(10)
        }
    }

    private var importPane: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                ProjectCard(title: "Import bundle") {
                    Text("Choose a Memory Layer bundle archive, then inspect the service preview and warnings before importing.").font(.caption).foregroundStyle(.secondary)
                    HStack {
                        Button("Choose bundle file…") { isChoosingFile = true }.disabled(operationActive || !canImport).accessibilityIdentifier("bundles.chooseFile")
                        if let selectedFileName { Text("\(selectedFileName) · \(selectedFileBytes?.count.formatted() ?? "0") bytes").font(.caption).lineLimit(1).truncationMode(.middle).foregroundStyle(.secondary) }
                    }
                    HStack {
                        Button("Preview import") { launchOperation { await previewImport() } }.buttonStyle(.borderedProminent).disabled(!canImport || operationActive || selectedFileBytes == nil).accessibilityIdentifier("bundles.previewImport")
                        Button("Review import…") { activeReview = .import; presentedReview = .import }.disabled(!canImport || operationActive || !importIsCurrent).accessibilityIdentifier("bundles.reviewImport")
                        if busy && !loading { ProgressView().controlSize(.small) }
                    }
                }
                if let preview = importPreview {
                    ProjectCard(title: "Import preview") {
                        Text("\(preview.memoryCount) memories · \(preview.newCount) new · \(preview.unchangedCount) unchanged · \(preview.replacingCount) replacing").font(.caption.weight(.medium)).foregroundStyle(.secondary)
                        Text("\(preview.sourceProject) → \(preview.targetProject) · bundle \(preview.bundleID)").font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                        Text(preview.summaryMarkdown).font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                        warningList(preview.warnings)
                        if !importIsCurrent { Text("The selected file or connection changed. Preview again before importing.").font(.caption).foregroundStyle(.orange) }
                    }
                } else {
                    ContentUnavailableView("Import preview required", systemImage: "square.and.arrow.down", description: Text("The service validates the archive and compares it with the current project before allowing import."))
                        .frame(maxWidth: .infinity, minHeight: 130)
                }
            }.padding(10)
        }
    }

    private func optionBinding(_ keyPath: WritableKeyPath<ExportOptions, Bool>) -> Binding<Bool> {
        Binding(get: { options[keyPath: keyPath] }, set: { value in
            options[keyPath: keyPath] = value; exportPreview = nil; exportSnapshot = nil; status = "Export options changed; preview is cleared."
        })
    }
    @ViewBuilder private func warningList(_ warnings: [String]) -> some View {
        if warnings.isEmpty {
            Label("No warnings returned", systemImage: "checkmark.circle").font(.caption).foregroundStyle(.green)
        } else {
            VStack(alignment: .leading, spacing: 5) {
                ForEach(Array(warnings.enumerated()), id: \.offset) { _, warning in Label(warning, systemImage: "exclamationmark.triangle.fill").font(.caption).foregroundStyle(.orange) }
            }.frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func previewExport() async {
        guard !busy else { return }
        let token = UUID(); requestID = token
        busy = true; loading = true; actionError = nil; operationWarning = nil; status = nil; exportPreview = nil; exportSnapshot = nil
        defer { finishRequest(token) }
        do {
            let frozen = options; let project = context.profile.projectSlug; let revision = context.revision
            let preview = try await service.previewExport(frozen); try Task.checkCancellation()
            guard requestID == token, context.profile.projectSlug == project, context.revision == revision, options == frozen else { return }
            exportPreview = preview; exportSnapshot = ExportPreviewSource(options: frozen, project: project, revision: revision)
            status = "Previewed \(preview.memoryCount) memories for \(project)."
            activeReview = .export; presentedReview = .export
        } catch is CancellationError { }
        catch { actionError = error.localizedDescription }
    }

    private func saveExport() async {
        guard !busy, exportIsCurrent, let frozen = exportSnapshot else { return }
        let token = UUID(); requestID = token
        busy = true; loading = false; actionError = nil; operationWarning = nil; status = nil
        defer { finishRequest(token) }
        do {
            let bytes = try await BundleService(context: context).export(options: frozen.options)
            try Task.checkCancellation()
            guard requestID == token, exportSnapshot?.matches(options: options, project: context.profile.projectSlug, revision: context.revision) == true else { return }
            let panel = NSSavePanel()
            panel.title = "Save Memory Layer bundle"
            panel.nameFieldStringValue = "\(frozen.project)-memory-bundle.mlbundle.zip"
            panel.allowedContentTypes = [.zip]
            panel.canCreateDirectories = true
            let response = await panel.begin()
            guard response == .OK, let url = panel.url else { status = "Bundle save was cancelled; this preview remains available."; return }
            try bytes.write(to: url, options: .atomic)
            status = "Saved \(bytes.count.formatted()) bytes to \(url.lastPathComponent)."
        } catch is CancellationError { operationWarning = "The export request was interrupted. The service may have recorded a completed export; preview again before starting another export." }
        catch { actionError = error.localizedDescription }
    }

    private func loadSelectedFile(_ url: URL) async {
        guard !busy else { return }
        let token = UUID(); requestID = token
        busy = true; loading = true; actionError = nil; operationWarning = nil; status = nil
        defer { finishRequest(token) }
        do {
            let bytes = try await Task.detached(priority: .userInitiated) { try BundleFileLoader.read(url) }.value
            try Task.checkCancellation()
            guard requestID == token else { return }
            selectedFileBytes = bytes; selectedFileName = url.lastPathComponent
            importPreview = nil; importSnapshot = nil; actionError = nil
            status = "Loaded \(bytes.count.formatted()) bytes from \(url.lastPathComponent). Preview this file before importing."
        } catch { selectedFileBytes = nil; selectedFileName = nil; importPreview = nil; importSnapshot = nil; actionError = error.localizedDescription }
    }

    private func previewImport() async {
        guard !busy, let bytes = selectedFileBytes else { return }
        let token = UUID(); requestID = token
        busy = true; loading = true; actionError = nil; operationWarning = nil; status = nil; importPreview = nil; importSnapshot = nil
        defer { finishRequest(token) }
        do {
            let project = context.profile.projectSlug; let revision = context.revision
            let preview = try await service.previewImport(bytes); try Task.checkCancellation()
            guard requestID == token, context.profile.projectSlug == project, context.revision == revision, selectedFileBytes == bytes else { return }
            importPreview = preview; importSnapshot = ImportPreviewSource(bytes: bytes, project: project, revision: revision)
            status = "Previewed bundle \(preview.bundleID) for \(preview.targetProject)."
            activeReview = .import; presentedReview = .import
        } catch is CancellationError { }
        catch { actionError = error.localizedDescription }
    }

    private func importBundle() async {
        guard !busy, importIsCurrent, let frozen = importSnapshot else { return }
        let preview = importPreview
        let token = UUID(); requestID = token
        busy = true; loading = false; actionError = nil; operationWarning = nil; status = nil
        defer { finishRequest(token) }
        do {
            let result = try await BundleService(context: context).importBundle(frozen.bytes); try Task.checkCancellation()
            guard requestID == token, result.targetProject == preview?.targetProject, result.bundleID == preview?.bundleID else {
                operationWarning = "The service responded, but its import result did not match the preview. Inspect the service data before retrying."
                return
            }
            status = "Imported \(result.importedCount) memories into \(result.targetProject) (\(result.replacedCount) replaced, \(result.skippedCount) skipped)."
            importPreview = nil; importSnapshot = nil; selectedFileBytes = nil; selectedFileName = nil
            activeReview = nil
        } catch is CancellationError {
            operationWarning = "The import request for \(frozen.project) was interrupted. The service may have completed it; inspect that project on the original connection before retrying."
        } catch {
            operationWarning = "The service did not confirm whether this import into \(frozen.project) completed (\(error.localizedDescription)). Inspect that project on the original connection before retrying."
        }
    }

    private func clearVolatileState() {
        options = ExportOptions(); exportPreview = nil; exportSnapshot = nil; importPreview = nil; importSnapshot = nil
        selectedFileBytes = nil; selectedFileName = nil
    }

    private func finishRequest(_ token: UUID) {
        guard requestID == token else { return }
        loading = false; busy = false
    }

    private func dismissReview() {
        guard !operationActive else { return }
        switch presentedReview {
        case .export: exportPreview = nil; exportSnapshot = nil
        case .import: importPreview = nil; importSnapshot = nil; selectedFileBytes = nil; selectedFileName = nil
        case nil: break
        }
        presentedReview = nil
    }

    @ViewBuilder private func reviewSheet(_ sheet: BundleReviewSheet) -> some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    if let actionError { ProjectBanner(title: "Bundle operation failed", message: actionError, tone: .failure) }
                    if let operationWarning { ProjectBanner(title: "Service outcome needs review", message: operationWarning, tone: .warning) }
                    if let status { ProjectBanner(title: "Bundle status", message: status, tone: .success).accessibilityElement(children: .ignore).accessibilityLabel("Bundle status. \(status)").accessibilityIdentifier("bundles.status") }
                    if operationActive { ProgressView(loading ? "Preparing bundle review…" : "Processing bundle…").frame(maxWidth: .infinity, alignment: .leading) }
                    switch sheet {
                    case .export:
                        if let preview = exportPreview {
                            ProjectCard(title: "Export preview") {
                                Text("\(preview.memoryCount) memories · \(preview.relationCount) relations · \(preview.warningCount) warnings").font(.headline)
                                Text("\(preview.sourceProject) · bundle \(preview.bundleID)").font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                                Text(preview.summaryMarkdown).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                                warningList(preview.warnings)
                                Button("Save bundle…") { launchOperation { await saveExport() } }.buttonStyle(.borderedProminent).disabled(!canExport || operationActive || !exportIsCurrent).accessibilityIdentifier("bundles.saveExport")
                            }
                        }
                    case .import:
                        if let preview = importPreview {
                            ProjectCard(title: "Review before import") {
                        Text("\(preview.memoryCount) memories · \(preview.newCount) new · \(preview.unchangedCount) unchanged · \(preview.replacingCount) replacing").font(.headline).accessibilityIdentifier("bundles.importPreviewCounts")
                                Text("\(preview.sourceProject) → \(preview.targetProject) · \(preview.warningCount) warnings · bundle \(preview.bundleID)").font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                                Text(preview.summaryMarkdown).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                                warningList(preview.warnings)
                                if preview.replacingCount > 0 { ProjectBanner(title: "Existing memories may be replaced", message: "The preview reports \(preview.replacingCount) replacements. Confirm import only if this change is expected.", tone: .warning) }
                                Button("Import \(preview.memoryCount) memories", role: .destructive) { launchOperation { await importBundle() } }.disabled(!canImport || operationActive || !importIsCurrent).accessibilityIdentifier("bundles.confirmImport")
                            }
                        }
                    }
                }.padding(18)
            }
            .navigationTitle(sheet == .export ? "Export bundle review" : "Import bundle review")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Close") { activeReview = nil }.disabled(operationActive) } }
        }
        .frame(minWidth: 580, minHeight: 420)
        .interactiveDismissDisabled(operationActive)
    }

    private func launchOperation(_ operation: @escaping () async -> Void) {
        guard operationTask == nil else { return }
        let token = UUID(); operationTaskID = token
        operationTask = Task {
            await operation()
            guard operationTaskID == token else { return }
            operationTask = nil
        }
    }

    private func cancelOperation() {
        operationTask?.cancel()
    }
}

private enum BundleReviewSheet: String, Identifiable { case export, `import`; var id: String { rawValue } }

private enum BundleFileLoader {
    static func read(_ url: URL) throws -> Data {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        if let size = attributes[.size] as? NSNumber, size.intValue > BundleService.maximumImportBytes { throw BundleFileError.tooLarge }
        let bytes = try Data(contentsOf: url, options: [.mappedIfSafe])
        guard bytes.count <= BundleService.maximumImportBytes else { throw BundleFileError.tooLarge }
        return bytes
    }
}
private enum BundleFileError: LocalizedError { case tooLarge; var errorDescription: String? { "This file exceeds the service's 2 MiB request-body limit." } }

private extension NSSavePanel {
    func begin() async -> NSApplication.ModalResponse {
        await withCheckedContinuation { continuation in begin { continuation.resume(returning: $0) } }
    }
}
