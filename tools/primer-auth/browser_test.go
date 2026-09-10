package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestMain(m *testing.M) {
	if len(os.Args) > 1 && os.Args[1] == "browser" {
		os.Exit(browserTestProcess())
	}
	os.Exit(m.Run())
}

func browserTestProcess() int {
	if len(os.Args) != 4 || os.Args[2] != "status" || os.Args[3] != "--json" {
		return 10
	}
	input, err := io.ReadAll(io.LimitReader(os.Stdin, 1))
	if err != nil || len(input) != 0 {
		return 11
	}
	for _, env := range os.Environ() {
		key, _, _ := strings.Cut(env, "=")
		if key != "HOME" && key != "PATH" {
			return 12
		}
	}
	dir, err := os.Getwd()
	if err != nil {
		return 13
	}
	info, err := os.Stat(dir)
	if err != nil || info.Mode().Perm() != 0700 {
		return 14
	}
	entries, err := os.ReadDir(dir)
	if err != nil || len(entries) != 0 {
		return 15
	}
	if err := os.WriteFile(filepath.Join(os.Getenv("HOME"), "discovery-cwd"), []byte(dir), 0600); err != nil {
		return 16
	}
	fixture, err := os.ReadFile(filepath.Join(os.Getenv("HOME"), "status.json"))
	if err != nil {
		return 17
	}
	switch string(fixture) {
	case "sleep":
		time.Sleep(10 * time.Second)
		return 18
	case "stdout_limit":
		_, _ = io.WriteString(os.Stdout, strings.Repeat("x", maxBrowserOutput+1))
		return 0
	case "stderr_limit":
		_, _ = io.WriteString(os.Stderr, strings.Repeat("x", maxBrowserOutput+1))
		return 0
	case "combined_limit":
		_, _ = io.WriteString(os.Stdout, strings.Repeat("x", maxBrowserOutput/2+1))
		_, _ = io.WriteString(os.Stderr, strings.Repeat("x", maxBrowserOutput/2+1))
		return 0
	case "exit_failure":
		fmt.Fprintln(os.Stdout, "synthetic-secret-output")
		return 19
	case "stderr_only":
		_, _ = os.Stderr.Write([]byte(`{"connection":"connected","mode":"managed","endpoint":{"ws_url":"ws://127.0.0.1:1/devtools/browser/test"}}`))
		return 0
	}
	fmt.Fprintln(os.Stderr, "synthetic stderr secret: must never be returned")
	_, _ = os.Stdout.Write(fixture)
	return 0
}

func statusTestCommand(t *testing.T, fixture string) string {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	for _, key := range []string{"PRIMER_TOKEN", "CLERK_SECRET_KEY", "LOCAL_API_KEY", "PYTHONPATH", "BUT_HOME", "BUT_DEBUG", "NODE_OPTIONS", "DISPLAY", "HTTP_PROXY"} {
		t.Setenv(key, "synthetic-secret-value")
	}
	if err := os.WriteFile(filepath.Join(home, "status.json"), []byte(fixture), 0600); err != nil {
		t.Fatal(err)
	}
	binary, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	return binary
}

func TestStatusDiscovery(t *testing.T) {
	for _, tt := range []struct {
		name, fixture string
		success       bool
	}{
		{"managed", `{"connection":"connected","mode":"managed","endpoint":{"ws_url":"ws://127.0.0.1:1234/devtools/browser/test"}}`, true},
		{"stdout_limit", "stdout_limit", false},
		{"stderr_limit", "stderr_limit", false},
		{"combined_limit", "combined_limit", false},
		{"exit_failure", "exit_failure", false},
		{"stderr_only", "stderr_only", false},
		{"sleep", "sleep", false},
		{"remote", `{"connection":"connected","mode":"managed","endpoint":{"ws_url":"ws://remote.example:1234/devtools/browser/test"}}`, false},
		{"missing state", `{"endpoint":{"ws_url":"ws://127.0.0.1:1234/devtools/browser/test"}}`, false},
		{"missing connection", `{"mode":"managed","endpoint":{"ws_url":"ws://127.0.0.1:1234/devtools/browser/test"}}`, false},
		{"missing mode", `{"connection":"connected","endpoint":{"ws_url":"ws://127.0.0.1:1234/devtools/browser/test"}}`, false},
		{"disconnected", `{"connection":"disconnected","mode":"managed","endpoint":{"ws_url":"ws://127.0.0.1:1234/devtools/browser/test"}}`, false},
		{"unmanaged", `{"connection":"connected","mode":"unmanaged","endpoint":{"ws_url":"ws://127.0.0.1:1234/devtools/browser/test"}}`, false},
	} {
		t.Run(tt.name, func(t *testing.T) {
			fixture := tt.fixture
			command := statusTestCommand(t, fixture)
			ctx, cancel := context.WithTimeout(t.Context(), 5*time.Second)
			if fixture == "sleep" {
				cancel()
				ctx, cancel = context.WithTimeout(t.Context(), 150*time.Millisecond)
			}
			defer cancel()
			started := time.Now()
			endpoint, err := discoverBrowser(ctx, command)
			if (err == nil) != tt.success {
				t.Fatalf("discovery success=%v, want %v", err == nil, tt.success)
			}
			if tt.success && endpoint != "ws://127.0.0.1:1234/devtools/browser/test" {
				t.Fatal("discovery changed the endpoint")
			}
			if !tt.success && (err != errBrowser || endpoint != "") {
				t.Fatal("discovery exposed error details")
			}
			if fixture == "sleep" && time.Since(started) > 2*time.Second {
				t.Fatal("timeout not enforced")
			}
			dir, err := os.ReadFile(filepath.Join(os.Getenv("HOME"), "discovery-cwd"))
			if err != nil {
				t.Fatal("status subprocess did not validate fixed arguments, empty stdin, minimal env and private cwd")
			}
			if _, err := os.Stat(string(dir)); !os.IsNotExist(err) {
				t.Fatal("discovery directory not removed")
			}
		})
	}
}

func TestParseBrowserStatus(t *testing.T) {
	good := "ws://127.0.0.1:1234/devtools/browser/test-id"
	for _, address := range []string{
		good, "ws://127.0.0.1:65535/devtools/browser/test", "ws://127.0.0.1:0/devtools/browser/test",
		"ws://127.0.0.1:65536/devtools/browser/test", "ws://127.0.0.1:0123/devtools/browser/test",
		"ws://localhost:1234/devtools/browser/test", "ws://[::1]:1234/devtools/browser/test",
		"ws://192.168.1.1:1234/devtools/browser/test", "wss://127.0.0.1:1234/devtools/browser/test",
		"ws://127.0.0.1:1234/devtools/page/test", "ws://127.0.0.1:1234/devtools/browser/",
		"ws://user@127.0.0.1:1234/devtools/browser/test", good + "?x=1", good + "#fragment",
		good + "/more", good + "%2f", "ws://127.0.0.1.evil:1234/devtools/browser/test",
	} {
		data, _ := json.Marshal(map[string]any{"connection": "connected", "mode": "managed", "endpoint": map[string]any{"ws_url": address}})
		endpoint, err := parseBrowserStatus(data)
		want := address == good || strings.Contains(address, ":65535/")
		if (err == nil) != want {
			t.Errorf("endpoint %q validation success=%v, want %v", address, err == nil, want)
		}
		if want && endpoint != address || !want && (err != errBrowser || endpoint != "") {
			t.Errorf("endpoint %q returned unexpected result %q, %v", address, endpoint, err)
		}
	}
	state := `{"connection":"connected","mode":"managed",`
	valid := state + `"endpoint":{"ws_url":"` + good + `"}}`
	for _, data := range []string{"", "null", "[]", `{}`, valid + valid, "noise\n" + valid,
		`{"connection":"connected","mode":"managed"}`,
		state + `"endpoint":null}`,
		state + `"endpoint":[]}`,
		state + `"endpoint":{}}`,
		state + `"endpoint":{"ws_url":null}}`,
		state + `"endpoint":{"ws_url":123}}`,
		state + `"endpoint":{"ws_url":"` + good + `","ws_url":"` + good + `"}}`,
		state + `"endpoint":{"ws_url":"` + good + `"},"endpoint":{"ws_url":"` + good + `"}}`,
		strings.Repeat(" ", maxBrowserOutput) + valid,
	} {
		if endpoint, err := parseBrowserStatus([]byte(data)); err != errBrowser || endpoint != "" {
			t.Fatal("accepted malformed status")
		}
	}
}

func TestParseBrowserStatusState(t *testing.T) {
	good := "ws://127.0.0.1:1234/devtools/browser/test-id"
	for _, tt := range []struct {
		name, fields string
		valid        bool
	}{
		{"managed", `"connection":"connected","mode":"managed",`, true},
		{"metadata", `"connection":"connected","mode":"managed","version":"0.1.8",`, true},
		{"whitespace", "\"connection\": \"connected\",\n\"mode\": \"managed\",", true},
		{"escaped strings", `"connection":"\u0063onnected","mode":"\u006danaged",`, true},
		{"missing state", "", false},
		{"legacy connected flag", `"connected":true,`, false},
		{"missing connection", `"mode":"managed",`, false},
		{"missing mode", `"connection":"connected",`, false},
		{"disconnected", `"connection":"disconnected","mode":"managed",`, false},
		{"disconnected with legacy flag", `"connected":true,"connection":"disconnected","mode":"managed",`, false},
		{"connecting", `"connection":"connecting","mode":"managed",`, false},
		{"empty connection", `"connection":"","mode":"managed",`, false},
		{"null connection", `"connection":null,"mode":"managed",`, false},
		{"boolean connection", `"connection":true,"mode":"managed",`, false},
		{"numeric connection", `"connection":1,"mode":"managed",`, false},
		{"object connection", `"connection":{},"mode":"managed",`, false},
		{"array connection", `"connection":["connected"],"mode":"managed",`, false},
		{"connection case", `"connection":"Connected","mode":"managed",`, false},
		{"connection whitespace", `"connection":"connected ","mode":"managed",`, false},
		{"connection field case", `"Connection":"connected","mode":"managed",`, false},
		{"unmanaged", `"connection":"connected","mode":"unmanaged",`, false},
		{"attached", `"connection":"connected","mode":"attached",`, false},
		{"empty mode", `"connection":"connected","mode":"",`, false},
		{"null mode", `"connection":"connected","mode":null,`, false},
		{"boolean mode", `"connection":"connected","mode":true,`, false},
		{"numeric mode", `"connection":"connected","mode":1,`, false},
		{"object mode", `"connection":"connected","mode":{},`, false},
		{"array mode", `"connection":"connected","mode":["managed"],`, false},
		{"mode case", `"connection":"connected","mode":"Managed",`, false},
		{"mode whitespace", `"connection":"connected","mode":" managed",`, false},
		{"mode field case", `"connection":"connected","Mode":"managed",`, false},
		{"duplicate connection", `"connection":"disconnected","connection":"connected","mode":"managed",`, false},
		{"duplicate mode", `"connection":"connected","mode":"unmanaged","mode":"managed",`, false},
		{"escaped duplicate", `"connection":"connected","\u0063onnection":"connected","mode":"managed",`, false},
	} {
		t.Run(tt.name, func(t *testing.T) {
			data := `{` + tt.fields + `"endpoint":{"ws_url":"` + good + `"}}`
			endpoint, err := parseBrowserStatus([]byte(data))
			if tt.valid {
				if err != nil || endpoint != good {
					t.Fatalf("managed status returned %q, %v", endpoint, err)
				}
			} else if err != errBrowser || endpoint != "" {
				t.Fatal("status did not fail closed")
			}
		})
	}
}

func TestCommandRunnerCannotStart(t *testing.T) {
	cfg := configTestValid()
	cfg.BrowserCommand = "/not/a/real/browser-executable"
	runner := commandRunner{cfg: cfg}
	if result, err := runner.Run(t.Context(), browserRequest{Method: "GET", Path: "/tasks"}); err != errBrowser || result.Status != 0 || !runner.unavailable.Load() {
		t.Fatal("start failure not redacted and latched")
	}
}

func TestParseBrowserResult(t *testing.T) {
	good := `{"status":200,"body":{"ok":true}}`
	for _, tt := range []struct {
		name, output string
		wantErr      bool
	}{
		{"object", good, false},
		{"noise rejected", "raw secret output\n" + good + "\nmore secret output", true},
		{"crlf", good + "\r\n", false},
		{"nested text", `{"status":200,"body":{"text":"hello"}}`, false},
		{"null body", `{"status":200,"body":null}`, false},
		{"array body", `{"status":422,"body":["invalid"]}`, false},
		{"empty", "", true},
		{"plain json", `{"status":200,"body":{}}`, false},
		{"prefixed json", "prefix " + good, true},
		{"indented json", " " + good, false},
		{"two objects", good + "\n" + good, true},
		{"bad then good", "invalid\n" + good, true},
		{"multiline", "{\n\"status\":200,\"body\":{}}", false},
		{"trailing json", good + " {}", true},
		{"missing body", `{"status":200}`, true},
		{"missing status", `{"body":{}}`, true},
		{"duplicate status", `{"status":200,"status":201,"body":{}}`, true},
		{"duplicate body", `{"status":200,"body":{},"body":{}}`, true},
		{"headers", `{"status":200,"body":{},"headers":{"Authorization":"secret"}}`, true},
		{"token", `{"status":200,"body":{},"token":"secret"}`, true},
		{"case variant", `{"Status":200,"body":{}}`, true},
		{"float status", `{"status":200.5,"body":{}}`, true},
		{"string status", `{"status":"200","body":{}}`, true},
		{"null envelope", `null`, true},
		{"array envelope", `[]`, true},
		{"utf8", "{\"status\":200,\"body\":\"\xff\"}", true},
		{"too much noise", strings.Repeat("x", maxBrowserOutput) + "\n" + good, true},
	} {
		t.Run(tt.name, func(t *testing.T) {
			_, err := parseBrowserResult([]byte(tt.output))
			if (err != nil) != tt.wantErr {
				t.Fatalf("parseBrowserResult error = %v, wantErr %v", err, tt.wantErr)
			}
		})
	}
	for status := 0; status <= 700; status++ {
		_, err := parseBrowserResult(fmt.Appendf(nil, `{"status":%d,"body":{}}`, status))
		want := status >= 200 && status <= 599 && (status < 300 || status >= 400) && status != 205
		if (err == nil) != want {
			t.Errorf("status %d accepted=%v, want %v", status, err == nil, want)
		}
	}
}

func TestOutputBudget(t *testing.T) {
	ctx, cancel := context.WithCancel(t.Context())
	defer cancel()
	budget := &outputBudget{cancel: cancel}
	stdout := outputStream{budget: budget, stdout: true}
	stderr := outputStream{budget: budget}
	if n, err := stdout.Write(bytes.Repeat([]byte{'a'}, maxBrowserOutput/2)); err != nil || n != maxBrowserOutput/2 {
		t.Fatal("stdout budget write failed")
	}
	if n, err := stderr.Write(bytes.Repeat([]byte{'b'}, maxBrowserOutput/2)); err != nil || n != maxBrowserOutput/2 {
		t.Fatal("stderr budget write failed")
	}
	if budget.stdout.Len() != maxBrowserOutput/2 || budget.total != maxBrowserOutput || ctx.Err() != nil {
		t.Fatal("budget miscounted output")
	}
	if _, err := stderr.Write([]byte{'c'}); err != errBrowser || ctx.Err() == nil {
		t.Fatal("overflow did not cancel context")
	}
	if _, err := stdout.Write([]byte{'c'}); err != errBrowser {
		t.Fatal("accepted output after overflow")
	}
}

func TestConcurrentOutputBudget(t *testing.T) {
	ctx, cancel := context.WithCancel(t.Context())
	defer cancel()
	budget := &outputBudget{cancel: cancel}
	var workers sync.WaitGroup
	for i := range 8 {
		workers.Go(func() {
			stream := outputStream{budget: budget, stdout: i%2 == 0}
			for range 300 {
				_, _ = stream.Write(make([]byte, 1024))
			}
		})
	}
	workers.Wait()
	if budget.total > maxBrowserOutput || !budget.exceeded || ctx.Err() == nil {
		t.Fatal("concurrent streams exceeded shared limit")
	}
}
