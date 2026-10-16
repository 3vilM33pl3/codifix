package main

import (
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestBuildQueryMemoryInvocationUsesDefaultProjectInJSONMode(t *testing.T) {
	oldEnv := os.Getenv("MEMORY_LAYER_PROJECT")
	t.Cleanup(func() { _ = os.Setenv("MEMORY_LAYER_PROJECT", oldEnv) })
	if err := os.Setenv("MEMORY_LAYER_PROJECT", "mem-project"); err != nil {
		t.Fatal(err)
	}

	got, err := buildQueryMemoryInvocation([]string{"How does this repo work?"}, outputJSON)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	want := []string{"query", "--project", "mem-project", "--question", "How does this repo work?", "--json"}
	if len(got.commandArgs) != len(want) {
		t.Fatalf("unexpected arg count: got %v want %v", got, want)
	}
	for i := range want {
		if got.commandArgs[i] != want[i] {
			t.Fatalf("arg %d mismatch: got %q want %q", i, got.commandArgs[i], want[i])
		}
	}
	if got.project != "mem-project" {
		t.Fatalf("unexpected project: %q", got.project)
	}
}

func TestBuildCheckpointProjectInvocationAddsJSONByDefault(t *testing.T) {
	got, err := buildCheckpointProjectInvocation([]string{"--project", "memory", "--note", "Plan approved"}, outputJSON)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	want := []string{"checkpoint", "save", "--project", "memory", "--note", "Plan approved", "--json"}
	if len(got.commandArgs) != len(want) {
		t.Fatalf("unexpected arg count: got %v want %v", got, want)
	}
	for i := range want {
		if got.commandArgs[i] != want[i] {
			t.Fatalf("arg %d mismatch: got %q want %q", i, got.commandArgs[i], want[i])
		}
	}
}

func TestBuildStartTaskExecutionAddsJSONInJSONMode(t *testing.T) {
	got, err := buildInvocation([]string{"start-task-execution", "--project", "memory", "--title", "Fix query input", "--prompt", "Improve query input"}, outputJSON)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	want := []string{"checkpoint", "start-task", "--project", "memory", "--title", "Fix query input", "--prompt", "Improve query input", "--json"}
	if len(got.commandArgs) != len(want) {
		t.Fatalf("unexpected arg count: got %v want %v", got, want)
	}
	for i := range want {
		if got.commandArgs[i] != want[i] {
			t.Fatalf("arg %d mismatch: got %q want %q", i, got.commandArgs[i], want[i])
		}
	}
	if !got.expectJSON {
		t.Fatal("expected JSON output")
	}
}

func TestRememberTaskStartsDirectTaskByDefault(t *testing.T) {
	got, err := buildInvocation([]string{
		"remember-task",
		"--project", "memory",
		"--title", "Fix query input",
		"--prompt", "Improve query input",
		"--summary", "Implemented query input",
	}, outputJSON)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	wantPrelude := []string{"checkpoint", "start-task", "--project", "memory", "--title", "Fix query input", "--prompt", "Improve query input", "--json"}
	if len(got.preludeArgs) != 1 {
		t.Fatalf("expected one prelude command, got %v", got.preludeArgs)
	}
	for i := range wantPrelude {
		if got.preludeArgs[0][i] != wantPrelude[i] {
			t.Fatalf("prelude arg %d mismatch: got %q want %q", i, got.preludeArgs[0][i], wantPrelude[i])
		}
	}
}

func TestRememberTaskSkipTaskStartBypassesPrelude(t *testing.T) {
	got, err := buildInvocation([]string{
		"remember-task",
		"--skip-task-start",
		"--project", "memory",
		"--title", "Fix query input",
		"--prompt", "Improve query input",
	}, outputJSON)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(got.preludeArgs) != 0 {
		t.Fatalf("expected no prelude command, got %v", got.preludeArgs)
	}
	for _, arg := range got.commandArgs {
		if arg == "--skip-task-start" {
			t.Fatal("skip flag must not be forwarded to memory remember")
		}
	}
}

func TestRememberProjectExplanationDoesNotStartTask(t *testing.T) {
	got, err := buildInvocation([]string{
		"remember-task",
		"--type", "project",
		"--project", "memory",
		"--title", "Explained TUI",
		"--prompt", "How does the TUI work?",
	}, outputJSON)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(got.preludeArgs) != 0 {
		t.Fatalf("expected no prelude command for project memory, got %v", got.preludeArgs)
	}
}

func TestBuildCaptureTaskArgsRejectsMissingFile(t *testing.T) {
	_, err := buildCaptureTaskInvocation([]string{"/tmp/does-not-exist.json"})
	if err == nil {
		t.Fatal("expected missing file error")
	}
}

func TestReviewProposalsListAddsJSONInJSONMode(t *testing.T) {
	got, err := buildInvocation([]string{"review-proposals", "list", "--project", "memory", "--limit", "3"}, outputJSON)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	want := []string{"proposals", "list", "--project", "memory", "--limit", "3", "--json"}
	if len(got.commandArgs) != len(want) {
		t.Fatalf("unexpected arg count: got %v want %v", got.commandArgs, want)
	}
	for i := range want {
		if got.commandArgs[i] != want[i] {
			t.Fatalf("arg %d mismatch: got %q want %q", i, got.commandArgs[i], want[i])
		}
	}
	if got.project != "memory" {
		t.Fatalf("unexpected project: %q", got.project)
	}
	if !got.expectJSON {
		t.Fatal("expected JSON output")
	}
}

func TestReviewProposalsApproveTextModePassesThrough(t *testing.T) {
	got, err := buildInvocation([]string{
		"review-proposals",
		"approve",
		"--project", "memory",
		"--id", "00000000-0000-0000-0000-000000000000",
	}, outputText)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	want := []string{"proposals", "approve", "--project", "memory", "--id", "00000000-0000-0000-0000-000000000000"}
	if len(got.commandArgs) != len(want) {
		t.Fatalf("unexpected arg count: got %v want %v", got.commandArgs, want)
	}
	for i := range want {
		if got.commandArgs[i] != want[i] {
			t.Fatalf("arg %d mismatch: got %q want %q", i, got.commandArgs[i], want[i])
		}
	}
	if got.expectJSON {
		t.Fatal("did not expect JSON output")
	}
	if !got.textPassthrough {
		t.Fatal("expected text passthrough")
	}
}

func TestReviewProposalsRejectsUnknownAction(t *testing.T) {
	_, err := buildInvocation([]string{"review-proposals", "maybe", "--project", "memory"}, outputJSON)
	if err == nil {
		t.Fatal("expected unknown action error")
	}
}

func TestParseGlobalOptionsSupportsTextMode(t *testing.T) {
	mode, remaining, err := parseGlobalOptions([]string{"--text", "resume-project", "memory"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if mode != outputText {
		t.Fatalf("expected text mode, got %v", mode)
	}
	if len(remaining) != 2 || remaining[0] != "resume-project" || remaining[1] != "memory" {
		t.Fatalf("unexpected remaining args: %v", remaining)
	}
}

func TestEmitHelperSuccessUsesStructuredEnvelope(t *testing.T) {
	oldStdout := os.Stdout
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	os.Stdout = w
	t.Cleanup(func() { os.Stdout = oldStdout })

	exitCode := emitHelperSuccess("resume-project", "memory", map[string]any{"answer": "ok"})
	_ = w.Close()
	output, _ := io.ReadAll(r)

	if exitCode != 0 {
		t.Fatalf("unexpected exit code: %d", exitCode)
	}
	var envelope helperEnvelope
	if err := json.Unmarshal(output, &envelope); err != nil {
		t.Fatalf("invalid json output: %v\n%s", err, string(output))
	}
	if !envelope.Ok || envelope.HelperCommand != "resume-project" || envelope.Project != "memory" {
		t.Fatalf("unexpected envelope: %+v", envelope)
	}
}

func TestEmitHelperFailureUsesStructuredEnvelope(t *testing.T) {
	oldStdout := os.Stdout
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	os.Stdout = w
	t.Cleanup(func() { os.Stdout = oldStdout })

	exitCode := emitHelperFailure(outputJSON, "query-memory", "memory", "command_failed", "boom", 7, []string{"memory", "query"}, "partial", "boom")
	_ = w.Close()
	output, _ := io.ReadAll(r)

	if exitCode != 7 {
		t.Fatalf("unexpected exit code: %d", exitCode)
	}
	var envelope helperEnvelope
	if err := json.Unmarshal(output, &envelope); err != nil {
		t.Fatalf("invalid json output: %v\n%s", err, string(output))
	}
	if envelope.Ok || envelope.Error == nil || envelope.Error.Kind != "command_failed" {
		t.Fatalf("unexpected envelope: %+v", envelope)
	}
	if envelope.Error.ExitCode == nil || *envelope.Error.ExitCode != 7 {
		t.Fatalf("missing exit code in envelope: %+v", envelope)
	}
}

func TestResolverPrefersExplicitMemctlBin(t *testing.T) {
	oldEnv := os.Getenv("MEMCTL_BIN")
	t.Cleanup(func() { _ = os.Setenv("MEMCTL_BIN", oldEnv) })
	if err := os.Setenv("MEMCTL_BIN", "memory --json"); err != nil {
		t.Fatal(err)
	}

	resolver := &memoryCommandResolver{sourceRoot: t.TempDir()}
	got, err := resolver.Resolve()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	want := []string{"memory", "--json"}
	if len(got) != len(want) {
		t.Fatalf("unexpected arg count: got %v want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("arg %d mismatch: got %q want %q", i, got[i], want[i])
		}
	}
}

func TestResolverPrefersSourceRepoOverInstalledMemory(t *testing.T) {
	sourceRoot := t.TempDir()
	mustWriteFile(t, filepath.Join(sourceRoot, "Cargo.toml"), []byte("[workspace]\n"))
	mustWriteFile(t, filepath.Join(sourceRoot, "crates", "mem-cli", "Cargo.toml"), []byte("[package]\nname=\"mem-cli\"\n"))

	binDir := t.TempDir()
	mustWriteFile(t, filepath.Join(binDir, "memory"), []byte("#!/bin/sh\nexit 0\n"))
	if err := os.Chmod(filepath.Join(binDir, "memory"), 0o755); err != nil {
		t.Fatal(err)
	}

	oldEnv := os.Getenv("MEMCTL_BIN")
	t.Cleanup(func() { _ = os.Setenv("MEMCTL_BIN", oldEnv) })
	if err := os.Unsetenv("MEMCTL_BIN"); err != nil {
		t.Fatal(err)
	}
	oldPath := os.Getenv("PATH")
	t.Cleanup(func() { _ = os.Setenv("PATH", oldPath) })
	if err := os.Setenv("PATH", binDir); err != nil {
		t.Fatal(err)
	}

	resolver := &memoryCommandResolver{sourceRoot: sourceRoot}
	got, err := resolver.Resolve()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	want := []string{
		"cargo", "run", "--quiet", "--bin", "memory", "--manifest-path", filepath.Join(sourceRoot, "Cargo.toml"), "--",
	}
	if len(got) != len(want) {
		t.Fatalf("unexpected arg count: got %v want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("arg %d mismatch: got %q want %q", i, got[i], want[i])
		}
	}
}

func TestResolverFallsBackToCargoWhenSourceRepoExists(t *testing.T) {
	sourceRoot := t.TempDir()
	mustWriteFile(t, filepath.Join(sourceRoot, "Cargo.toml"), []byte("[workspace]\n"))
	mustWriteFile(t, filepath.Join(sourceRoot, "crates", "mem-cli", "Cargo.toml"), []byte("[package]\nname=\"mem-cli\"\n"))

	oldEnv := os.Getenv("MEMCTL_BIN")
	t.Cleanup(func() { _ = os.Setenv("MEMCTL_BIN", oldEnv) })
	if err := os.Unsetenv("MEMCTL_BIN"); err != nil {
		t.Fatal(err)
	}
	oldPath := os.Getenv("PATH")
	t.Cleanup(func() { _ = os.Setenv("PATH", oldPath) })
	if err := os.Setenv("PATH", ""); err != nil {
		t.Fatal(err)
	}

	resolver := &memoryCommandResolver{sourceRoot: sourceRoot}
	got, err := resolver.Resolve()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	want := []string{
		"cargo", "run", "--quiet", "--bin", "memory", "--manifest-path", filepath.Join(sourceRoot, "Cargo.toml"), "--",
	}
	if len(got) != len(want) {
		t.Fatalf("unexpected arg count: got %v want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("arg %d mismatch: got %q want %q", i, got[i], want[i])
		}
	}
}

func TestSourceDefaultEnvironmentUsesDevWithoutInheritedAreaRouting(t *testing.T) {
	t.Setenv("MEMORY_LAYER_PROFILE", "prod")
	t.Setenv("MEMORY_LAYER_AREA", "workshop")
	t.Setenv("MEMORY_LAYER_AREA_ROUTING", "/operator/routes.json")
	t.Setenv("MEMORY_LAYER_PROJECT", "memory")
	got := commandEnvironment(helperInvocation{devSourceRoot: t.TempDir()})
	joined := "\n" + strings.Join(got, "\n") + "\n"
	for _, unexpected := range []string{"MEMORY_LAYER_PROFILE=prod", "MEMORY_LAYER_AREA=", "MEMORY_LAYER_AREA_ROUTING="} {
		if strings.Contains(joined, unexpected) {
			t.Fatalf("default dev inherited installed routing: %s", unexpected)
		}
	}
	for _, expected := range []string{"MEMORY_LAYER_PROFILE=dev", "MEMORY_LAYER_PROJECT=memory"} {
		if !strings.Contains(joined, "\n"+expected+"\n") {
			t.Fatalf("missing environment entry: %s", expected)
		}
	}
	// An explicit override retains the installed launcher's environment.
	joined = strings.Join(commandEnvironment(helperInvocation{}), "\n")
	if !strings.Contains(joined, "MEMORY_LAYER_AREA=workshop") || !strings.Contains(joined, "MEMORY_LAYER_PROFILE=prod") {
		t.Fatal("explicit override lost its selected routing")
	}
}

func TestResolverUsesExplicitProjectCommandBeforeSourceDefault(t *testing.T) {
	t.Setenv("MEMCTL_BIN", "")
	root := t.TempDir()
	mustWriteFile(t, filepath.Join(root, "Cargo.toml"), []byte("[workspace]\n"))
	mustWriteFile(t, filepath.Join(root, "crates", "mem-cli", "Cargo.toml"), []byte("[package]\n"))
	path := filepath.Join(root, ".agents", "memory-layer-command.json")
	mustWriteFile(t, path, []byte(`["memory","--area","workshop"]`))
	resolver := &memoryCommandResolver{sourceRoot: root}
	got, err := resolver.Resolve()
	if err != nil || strings.Join(got, " ") != "memory --area workshop" || resolver.usesDevCommand(got) {
		t.Fatalf("project command not selected: %v (%v)", got, err)
	}
	t.Setenv("MEMCTL_BIN", "cargo run --quiet --bin memory --")
	got, err = resolver.Resolve()
	if err != nil || !resolver.usesDevCommand(got) {
		t.Fatalf("explicit dev test override not selected: %v (%v)", got, err)
	}
	t.Setenv("MEMCTL_BIN", "")
	for _, invalid := range []string{`[]`, `null`, `[""]`, `["memory",`, `{ "command": "memory" }`} {
		mustWriteFile(t, path, []byte(invalid))
		if _, err := resolver.Resolve(); err == nil {
			t.Fatalf("invalid project selection silently fell back: %s", invalid)
		}
	}
}

func TestDevServiceFailuresReportRecoveryWithoutFallback(t *testing.T) {
	for _, tc := range []struct{ name, failure, kind string }{
		{"connection", "Error: error sending request for url (http://127.0.0.1:4250/v1/query): Connection refused", "dev_service_unavailable"},
		{"degraded", "503 Service Unavailable: PostgreSQL is unavailable; service is running in offline degraded mode", "dev_service_degraded"},
		{"auth", "401 Unauthorized: invalid or expired service token", "dev_service_auth_failed"},
	} {
		for _, textMode := range []bool{false, true} {
			t.Run(tc.name+map[bool]string{true: "-text", false: "-json"}[textMode], func(t *testing.T) {
				t.Setenv("MEMCTL_BIN", "cargo run --quiet --bin memory --")
				binDir := t.TempDir()
				calls := filepath.Join(binDir, "calls")
				cargo := filepath.Join(binDir, "cargo")
				mustWriteFile(t, cargo, []byte("#!/bin/sh\necho dev >> '"+calls+"'\necho '"+tc.failure+"' >&2\nexit 7\n"))
				if err := os.Chmod(cargo, 0o755); err != nil {
					t.Fatal(err)
				}
				installed := filepath.Join(binDir, "memory")
				mustWriteFile(t, installed, []byte("#!/bin/sh\necho installed >> '"+calls+"'\nexit 0\n"))
				if err := os.Chmod(installed, 0o755); err != nil {
					t.Fatal(err)
				}
				t.Setenv("PATH", binDir)
				for _, args := range [][]string{
					{"query-memory", "question", "memory"},
					{"remember-task", "--project", "memory", "--title", "title", "--prompt", "prompt", "--summary", "summary"},
				} {
					if textMode {
						args = append([]string{"--text"}, args...)
					}
					code, stdout, stderr := captureHelperRun(t, func() int { return run(args) })
					if code != 7 {
						t.Fatalf("exit code = %d, want 7", code)
					}
					message := stderr
					if !textMode {
						var envelope helperEnvelope
						if err := json.Unmarshal([]byte(stdout), &envelope); err != nil {
							t.Fatalf("invalid JSON: %v: %s", err, stdout)
						}
						if envelope.Error == nil || envelope.Error.Kind != tc.kind || envelope.Error.ExitCode == nil || *envelope.Error.ExitCode != 7 {
							t.Fatalf("unexpected failure: %+v", envelope)
						}
						if envelope.Details == nil || !strings.Contains(envelope.Details.Stderr, tc.failure) {
							t.Fatal("raw error was not preserved")
						}
						message = envelope.Error.Message
					}
					for _, expected := range []string{tc.failure, "dev Memory Layer service", "cargo run --bin memory -- doctor", "cargo run --bin memory -- service run", "database=up", "MEMCTL_BIN='memory --area workshop'", "never a failure fallback", "partially completed"} {
						if !strings.Contains(message, expected) {
							t.Fatalf("missing %q in %s", expected, message)
						}
					}
				}
				data, err := os.ReadFile(calls)
				if err != nil || string(data) != "dev\ndev\n" {
					t.Fatalf("expected one attempt per failure, no prelude continuation or installed fallback: %q (%v)", data, err)
				}
			})
		}
	}
}

func TestUnrelatedAndExplicitOverrideFailuresKeepOriginalKind(t *testing.T) {
	for _, tc := range []struct {
		invocation helperInvocation
		failure    string
	}{
		{helperInvocation{devSourceRoot: t.TempDir()}, "error: could not compile mem-cli"},
		{helperInvocation{}, "error sending request for url (http://127.0.0.1:4260/v1/query)"},
	} {
		_, stdout, _ := captureHelperRun(t, func() int {
			return emitCommandFailure(outputJSON, tc.invocation, []string{"memory", "query"}, "command_failed", 1, nil, "", tc.failure)
		})
		var envelope helperEnvelope
		if err := json.Unmarshal([]byte(stdout), &envelope); err != nil {
			t.Fatal(err)
		}
		if envelope.Error.Kind != "command_failed" || envelope.Error.Message != tc.failure {
			t.Fatalf("unrelated failure mislabeled as dev outage: %+v", envelope.Error)
		}
	}
}

func captureHelperRun(t *testing.T, f func() int) (int, string, string) {
	t.Helper()
	oldOut, oldErr := os.Stdout, os.Stderr
	outReader, outWriter, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	errReader, errWriter, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	defer outReader.Close()
	defer errReader.Close()
	defer func() { os.Stdout, os.Stderr = oldOut, oldErr }()
	os.Stdout, os.Stderr = outWriter, errWriter
	code := f()
	outWriter.Close()
	errWriter.Close()
	stdout, _ := io.ReadAll(outReader)
	stderr, _ := io.ReadAll(errReader)
	return code, string(stdout), string(stderr)
}

func mustWriteFile(t *testing.T, path string, content []byte) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, content, 0o644); err != nil {
		t.Fatal(err)
	}
}
