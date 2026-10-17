import SwiftUI

private struct ProjectSkillLoadKey: Hashable {
    let revision: Int
    let filter: ProjectSkillFilter
    let skill: String?
    let refreshToken: UUID
}

public struct SkillsView: View {
    public let context: FeatureContext
    @State private var inventory: ProjectSkillInventory?
    @State private var content: ProjectSkillContentResponse?
    @State private var selectedSkillID: String?
    @State private var filter: ProjectSkillFilter = .codifix
    @State private var loading = true
    @State private var contentLoading = false
    @State private var refreshID = UUID()
    @State private var loadError: String?
    @State private var contentError: String?
    @State private var repairError: String?
    @State private var repairResult: String?
    @State private var repairBusy = false
    @State private var repairConfirmation = false
    @State private var repairContext: FeatureContext?
    @State private var contentRefreshToken = UUID()

    public init(context: FeatureContext) { self.context = context }

    private var selectedSkill: ProjectSkillVersion? {
        inventory?.skills.first(where: { $0.name == selectedSkillID })
    }

    private var selectedContentKey: ProjectSkillLoadKey {
        ProjectSkillLoadKey(revision: context.revision, filter: filter, skill: selectedSkillID, refreshToken: contentRefreshToken)
    }

    public var body: some View {
        ZStack {
            if !context.permissions.allows(.memoryRead) {
                ProjectLoadingState(context: context, title: "Access restricted", message: "This identity does not have permission to inspect project skills.", isLoading: false)
            } else if context.profile.repositoryRoot == nil {
                ContentUnavailableView {
                    Label("Repository root required", systemImage: "folder.badge.questionmark")
                } description: {
                    Text("Choose a repository root in connection settings to inspect the skill inventory for this connected project.")
                } actions: {
                    Button("Review connection settings…") {
                        NotificationCenter.default.post(name: .showConnectionSettings, object: nil)
                    }
                    .buttonStyle(.borderedProminent)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if inventory == nil && loading {
                ProjectLoadingState(context: context, title: "Loading skills…", message: "Reading the connected repository’s Codifix skill inventory.", isLoading: true)
            } else if inventory == nil, let loadError {
                ProjectLoadingState(context: context, title: "Could not load skills", message: "The connected repository did not return a skill inventory.", isLoading: false, error: loadError) { refreshID = UUID() }
            } else {
                contentView
            }
        }
        .task(id: context.revision) {
            inventory = nil
            content = nil
            selectedSkillID = nil
            loadError = nil
            repairError = nil
            repairResult = nil
            repairBusy = false
            contentLoading = false
            refreshID = UUID()
        }
        .task(id: refreshID) { await loadInventory() }
        .task(id: selectedContentKey) { await loadSelectedContent(for: selectedContentKey) }
        .confirmationDialog("Repair project skills?", isPresented: $repairConfirmation, titleVisibility: .visible) {
            Button("Repair skills", role: .destructive) {
                let snapshot = repairContext ?? context
                Task { await repair(using: snapshot) }
            }
            Button("Cancel", role: .cancel) { repairContext = nil }
        } message: {
            Text("Bring managed skill files in \(repairContext?.profile.repositoryRoot?.path ?? context.profile.repositoryRoot?.path ?? "the selected repository") up to date from their templates. Existing newer local versions are preserved by the service.")
        }
    }

    private var contentView: some View {
        VStack(alignment: .leading, spacing: 14) {
            ProjectPageHeader(title: "Skills", subtitle: "Managed skill inventory · \(context.profile.projectSlug)") {
                HStack(spacing: 8) {
                    Button("Repair skills…") {
                        repairContext = context
                        repairConfirmation = true
                    }
                    .buttonStyle(.bordered)
                    .disabled(!canRepair || repairBusy)
                    Button("Refresh") { refreshID = UUID() }
                        .disabled(loading || repairBusy)
                }
            }

            if let loadError {
                ProjectBanner(title: "Refresh failed", message: "Showing the last skill inventory. \(loadError)", tone: .failure) { refreshID = UUID() }
            }
            if let repairResult { ProjectBanner(title: "Skills repaired", message: repairResult, tone: .success) }
            if let repairError { ProjectBanner(title: "Skill repair failed", message: repairError, tone: .failure) }

            if let inventory {
                HSplitView {
                    VStack(alignment: .leading, spacing: 10) {
                        Picker("Filter", selection: $filter) {
                            ForEach(ProjectSkillFilter.allCases) { item in Text(item.label).tag(item) }
                        }
                        .pickerStyle(.segmented)
                        .disabled(loading || repairBusy)
                        HStack {
                            Text("Bundle v\(inventory.bundleVersion)")
                            Spacer()
                            Text(inventory.status.capitalized).foregroundStyle(statusColor(inventory.status))
                        }
                        .font(.caption).foregroundStyle(.secondary)
                        Text(inventory.summary).font(.callout).foregroundStyle(.secondary)
                        if inventory.skills.isEmpty {
                            ContentUnavailableView("No skills found", systemImage: "chevron.left.forwardslash.chevron.right", description: Text("The selected inventory filter returned no skills."))
                        } else {
                            List(inventory.skills, selection: $selectedSkillID) { skill in
                                VStack(alignment: .leading, spacing: 5) {
                                    HStack {
                                        Text(skill.name).font(.subheadline.weight(.semibold))
                                        Spacer(minLength: 4)
                                        Text(skill.status.replacingOccurrences(of: "_", with: " ").capitalized)
                                            .font(.caption).foregroundStyle(statusColor(skill.status))
                                    }
                                    Text(skill.detail ?? skill.projectPath).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                                    Text("\(skill.action.capitalized) · Local \(skill.projectVersion ?? "missing")")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                .padding(.vertical, 5)
                                .tag(skill.id)
                                .contentShape(Rectangle())
                                .onTapGesture { select(skill) }
                            }
                            .listStyle(.plain)
                        }
                    }
                    .frame(minWidth: 275, idealWidth: 330, maxWidth: 390, maxHeight: .infinity)

                    skillDetail
                        .frame(minWidth: 390, maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .onChange(of: filter) { _, _ in
            content = nil
            contentError = nil
            contentLoading = false
            refreshID = UUID()
        }
        .onChange(of: selectedSkillID) { _, _ in
            content = nil
            contentError = nil
            contentLoading = false
        }
    }

    @ViewBuilder private var skillDetail: some View {
        if let skill = selectedSkill {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(skill.name).font(.title3.weight(.semibold))
                        Text(skill.detail ?? "Current skill inventory entry").font(.callout).foregroundStyle(.secondary)
                    }
                    ProjectCard(title: nil) {
                        LabeledContent("Project version", value: skill.projectVersion ?? "Missing")
                        LabeledContent("Template version", value: skill.templateVersion ?? "Not available")
                        LabeledContent("Status", value: skill.status.replacingOccurrences(of: "_", with: " ").capitalized)
                        LabeledContent("Action", value: skill.action.capitalized)
                        LabeledContent("Project path", value: skill.projectPath).textSelection(.enabled)
                        LabeledContent("Template path", value: skill.templatePath ?? "Not found").textSelection(.enabled)
                    }
                    VStack(alignment: .leading, spacing: 7) {
                        Text("Skill content").font(.headline)
                        if let content, content.skill.name == skill.name {
                            if let value = content.content, !value.isEmpty {
                                Text(value)
                                    .font(.system(.callout, design: .monospaced))
                                    .textSelection(.enabled)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .padding(12)
                                    .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
                                if content.contentTruncated {
                                    Text("Content preview is truncated.").font(.caption).foregroundStyle(.secondary)
                                }
                            } else {
                                Text("No SKILL.md content is available for this skill.").foregroundStyle(.secondary)
                            }
                        } else if let contentError {
                            ProjectBanner(title: "Could not load skill content", message: contentError, tone: .failure)
                        } else if contentLoading {
                            ProgressView("Loading skill content…").controlSize(.small)
                        } else {
                            Text("Content preview · Read-only").font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        } else {
            ContentUnavailableView("Select a skill", systemImage: "chevron.left.forwardslash.chevron.right", description: Text("Choose an inventory row to inspect its versions, status, and read-only content."))
        }
    }

    private var canRepair: Bool {
        !context.permissions.readOnly && context.permissions.globalAllowed.contains(.systemAdmin)
    }

    private func select(_ skill: ProjectSkillVersion) {
        selectedSkillID = skill.id
        content = nil
        contentError = nil
    }

    private func loadInventory() async {
        let token = refreshID
        let revision = context.revision
        loading = true
        loadError = nil
        contentRefreshToken = UUID()
        content = nil
        contentLoading = false
        do {
            let result = try await ProjectService(context: context).skills(filter: filter)
            try Task.checkCancellation()
            guard token == refreshID, revision == context.revision else { return }
            inventory = result
            if selectedSkillID == nil || !result.skills.contains(where: { $0.id == selectedSkillID }) {
                selectedSkillID = result.skills.first?.id
            }
            content = nil
            contentRefreshToken = UUID()
        } catch is CancellationError {
            return
        } catch {
            guard token == refreshID, revision == context.revision else { return }
            loadError = error.localizedDescription
        }
        if token == refreshID, revision == context.revision { loading = false }
    }

    private func loadSelectedContent(for key: ProjectSkillLoadKey) async {
        guard let skillID = key.skill,
              inventory?.filter == key.filter.rawValue,
              inventory?.skills.contains(where: { $0.name == skillID }) == true else { return }
        let revision = key.revision
        contentLoading = true
        contentError = nil
        content = nil
        do {
            let result = try await ProjectService(context: context).skill(name: skillID, filter: key.filter)
            try Task.checkCancellation()
            guard selectedContentKey == key, revision == context.revision else { return }
            content = result
        } catch is CancellationError {
            return
        } catch {
            guard selectedContentKey == key, revision == context.revision else { return }
            contentError = error.localizedDescription
        }
        if selectedContentKey == key, revision == context.revision { contentLoading = false }
    }

    private func repair(using snapshot: FeatureContext) async {
        guard !repairBusy else { return }
        repairBusy = true
        repairError = nil
        repairResult = nil
        do {
            let response = try await ProjectService(context: snapshot).repairSkills(filter: filter)
            try Task.checkCancellation()
            guard snapshot.revision == context.revision else { return }
            inventory = response.inventory
            repairResult = response.inventory.summary
            if selectedSkillID == nil || !response.inventory.skills.contains(where: { $0.id == selectedSkillID }) {
                selectedSkillID = response.inventory.skills.first?.id
            }
            content = nil
            contentRefreshToken = UUID()
            repairContext = nil
        } catch is CancellationError {
            return
        } catch {
            guard snapshot.revision == context.revision else { return }
            repairError = error.localizedDescription
        }
        if snapshot.revision == context.revision { repairBusy = false }
    }

    private func statusColor(_ status: String) -> Color {
        switch status {
        case "up_to_date", "ok": .green
        case "outdated", "missing", "warn", "unversioned", "invalid_version": .orange
        case "error", "template_missing": .red
        default: .secondary
        }
    }
}
