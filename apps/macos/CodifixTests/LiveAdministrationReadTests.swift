import Foundation
import XCTest
@testable import Codifix

final class LiveAdministrationReadTests: XCTestCase {
    func testOptionalLiveGraphAutomationAndAccessReads() async throws {
        let env = ProcessInfo.processInfo.environment
        guard env["CODIFIX_LIVE_READ_SMOKE"] == "1" ||
                env["TEST_RUNNER_CODIFIX_LIVE_READ_SMOKE"] == "1" else {
            throw XCTSkip("Enable the existing development service read smoke explicitly.")
        }
        let profile = ConnectionProfile(name: "Live read smoke", baseURL: URL(string: "http://127.0.0.1:4250")!,
                                        projectSlug: "memory", repositoryRoot: URL(fileURLWithPath: "/Users/olivier/Projects/memory"),
                                        kind: .localSession)
        let client = MemoryServiceClient(profile: profile)
        let auth = try await client.bootstrapLocalSession(context: RequestContext(profile: profile, revision: 1))
        let context = FeatureContext(client: client, profile: profile, auth: auth, revision: 1)
        let graph = try await GraphService(context: context).load(filters: GraphFilterDraft())
        XCTAssertEqual(graph.memoryGraph.project, profile.projectSlug)
        let automation = AutomationService(context: context)
        let definitions = try await automation.definitions()
        _ = try await automation.globalState()
        _ = try await automation.pendingApprovals()
        _ = try await automation.pendingMemoryProposals()
        if let first = definitions.first {
            _ = try await automation.definition(first.loopID)
            let runs = try await automation.recentRuns(loopID: first.loopID)
            if let run = runs.first { _ = try await automation.run(id: run.id) }
        }
        let access = AccessService(context: context)
        let principals = try await access.principals()
        _ = try await access.memberships()
        _ = try await access.tokens()
        print("Live read smoke: graph_nodes=\(graph.memoryGraph.nodes.count) loops=\(definitions.count) principals=\(principals.count)")
    }
}
