package main

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"strconv"
	"strings"
	"sync"
	"testing"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
)

type primerResult struct {
	Data    string `json:"data"`
	IsError bool   `json:"is_error"`
}

func loadPrimer(t *testing.T, ctx context.Context, configuration map[string]string) api.Module {
	t.Helper()
	moduleBytes, err := os.ReadFile("../../target/wasm32-wasip1/release/primer_wasm.wasm")
	if err != nil {
		t.Fatalf("build the Primer WASM artifact before this test: %v", err)
	}
	runtime := wazero.NewRuntime(ctx)
	t.Cleanup(func() { _ = runtime.Close(ctx) })
	wasi_snapshot_preview1.MustInstantiate(ctx, runtime)
	_, err = runtime.NewHostModuleBuilder("env").
		NewFunctionBuilder().WithFunc(hostHTTPRequest).Export("host_http_request").
		NewFunctionBuilder().WithFunc(hostLog).Export("host_log").Instantiate(ctx)
	if err != nil {
		t.Fatal(err)
	}
	module, err := runtime.InstantiateWithConfig(ctx, moduleBytes, wazero.NewModuleConfig().WithStartFunctions())
	if err != nil {
		t.Fatal(err)
	}
	if initialize := module.ExportedFunction("_initialize"); initialize != nil {
		if _, err := initialize.Call(ctx); err != nil {
			t.Fatal(err)
		}
	}
	data, _ := json.Marshal(configuration)
	if result := callWithInput(ctx, module, "configure", data); result != "" {
		t.Fatalf("fixture configuration rejected: %s", result)
	}
	return module
}

func executePrimer(t *testing.T, ctx context.Context, module api.Module, tool string, args map[string]any) primerResult {
	t.Helper()
	input, _ := json.Marshal(map[string]any{"tool_name": tool, "args": args})
	var result primerResult
	if err := json.Unmarshal([]byte(callWithInput(ctx, module, "execute", input)), &result); err != nil {
		t.Fatal(err)
	}
	return result
}

type recordedRequest struct {
	Method, Path, Query string
	Header              http.Header
	Body                []byte
}

func TestPrimerReadingLibraryWASM(t *testing.T) {
	const planID = "9d3a8f62-7f4d-4f5e-9a51-0e8e6c2b7a10"
	epub := append([]byte("PK\x03\x04"), bytes.Repeat([]byte("chapter"), 2048)...)
	digest := sha256.Sum256(epub)
	var mu sync.Mutex
	var requests []recordedRequest
	source := httptest.NewTLSServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		body, _ := io.ReadAll(request.Body)
		mu.Lock()
		requests = append(requests, recordedRequest{request.Method, "source:" + request.URL.Path, request.URL.RawQuery, request.Header.Clone(), body})
		mu.Unlock()
		writer.Header().Set("Content-Type", "application/epub+zip")
		_, _ = writer.Write(epub)
	}))
	defer source.Close()
	lms := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		body, _ := io.ReadAll(request.Body)
		mu.Lock()
		requests = append(requests, recordedRequest{request.Method, request.URL.EscapedPath(), request.URL.RawQuery, request.Header.Clone(), body})
		mu.Unlock()
		writer.Header().Set("Content-Type", "application/json")
		switch {
		case request.Method == http.MethodDelete:
			writer.WriteHeader(http.StatusNoContent)
		case request.URL.Path == "/api/v1/reading/plans/"+planID+"/confirmations" && request.Method == http.MethodPost:
			writer.WriteHeader(http.StatusConflict)
			_, _ = writer.Write([]byte(`{"title":"Conflict","status":409,"detail":"reading plan version conflict"}`))
		default:
			_, _ = writer.Write([]byte(`{"items":[],"echo":"ok"}`))
		}
	}))
	defer lms.Close()
	previous := httpClient
	httpClient = source.Client()
	defer func() { httpClient = previous }()

	ctx := context.Background()
	module := loadPrimer(t, ctx, map[string]string{
		"tv_base_url": lms.URL + "/tv", "tv_admin_key": "fixture-tv-key",
		"tasks_base_url": lms.URL + "/tasks/api", "tasks_api_key": "fixture-oauth-token",
		"lms_base_url": lms.URL + "/api/v1/",
	})
	var tools []struct {
		Name     string
		Required []string
	}
	if err := json.Unmarshal([]byte(callString(ctx, module, "tools")), &tools); err != nil {
		t.Fatal(err)
	}
	registered := map[string]bool{}
	for _, tool := range tools {
		registered[tool.Name] = true
	}
	for _, name := range []string{"primer_list_reading_works", "primer_create_quick_reading_plan", "primer_publish_reading_assignment", "primer_import_reading_epub", "primer_list_lms_students"} {
		if !registered[name] {
			t.Fatalf("%s missing from exported tools", name)
		}
	}

	body := `{"title":"Call of the Wild","author":"Jack London","language":"en"}`
	for _, call := range []struct {
		tool string
		args map[string]any
	}{
		{"primer_list_reading_works", map[string]any{"q": "Call & Wild", "limit": 2}},
		{"primer_list_reading_plans", map[string]any{"student_id": "student-1"}},
		{"primer_create_reading_work", map[string]any{"body": body}},
		{"primer_create_reading_work", map[string]any{"body": body}},
		{"primer_create_reading_work", map[string]any{"body": body, "idempotency_key": "parent-retry-2"}},
		{"primer_schedule_reading_segment", map[string]any{"id": planID, "segment_id": "seg/1", "body": `{"version":3,"localDate":"2026-11-02","timezone":"America/Chicago"}`}},
		{"primer_delete_reading_plan", map[string]any{"id": planID, "version": 3}},
		{"primer_import_reading_epub", map[string]any{"work_id": "w1", "edition_id": "e1", "source_url": source.URL + "/call-of-the-wild.epub", "rights_source": "Standard Ebooks", "rights_license": "Public domain (US)", "rights_acknowledged": true}},
	} {
		result := executePrimer(t, ctx, module, call.tool, call.args)
		if result.IsError {
			t.Fatalf("%s failed: %s", call.tool, result.Data)
		}
	}
	conflict := executePrimer(t, ctx, module, "primer_confirm_reading_range", map[string]any{"id": planID, "body": `{"version":1,"startNodeId":"a","endNodeId":"b","source":"manual"}`})
	if !conflict.IsError || !strings.Contains(conflict.Data, "409") || !strings.Contains(conflict.Data, "version conflict") {
		t.Fatalf("LMS conflict not surfaced: %+v", conflict)
	}
	for _, invalid := range []struct {
		tool string
		args map[string]any
	}{
		{"primer_create_reading_work", map[string]any{"body": "{not json"}},
		{"primer_delete_reading_plan", map[string]any{"id": planID}},
		{"primer_get_reading_plan", map[string]any{}},
		{"primer_import_reading_epub", map[string]any{"work_id": "w1", "edition_id": "e1", "source_url": "http://insecure.test/book.epub", "rights_source": "x", "rights_license": "y", "rights_acknowledged": true}},
	} {
		if result := executePrimer(t, ctx, module, invalid.tool, invalid.args); !result.IsError {
			t.Fatalf("%s accepted invalid arguments", invalid.tool)
		}
	}

	mu.Lock()
	defer mu.Unlock()
	want := []struct{ method, path, query string }{
		{"GET", "/api/v1/reading/works", "q=Call%20%26%20Wild&limit=2"},
		{"GET", "/api/v1/reading/plans", "studentId=student-1"},
		{"POST", "/api/v1/reading/works", ""},
		{"POST", "/api/v1/reading/works", ""},
		{"POST", "/api/v1/reading/works", ""},
		{"PUT", "/api/v1/reading/plans/" + planID + "/segments/seg%2F1/day", ""},
		{"DELETE", "/api/v1/reading/plans/" + planID, "version=3"},
		{"GET", "source:/call-of-the-wild.epub", ""},
		{"POST", "/api/v1/reading/works/w1/editions/e1/imports", ""},
		{"POST", "/api/v1/reading/plans/" + planID + "/confirmations", ""},
	}
	if len(requests) != len(want) {
		t.Fatalf("requests = %d, want %d: %+v", len(requests), len(want), requests)
	}
	for index, expected := range want {
		got := requests[index]
		if got.Method != expected.method || got.Path != expected.path || got.Query != expected.query {
			t.Fatalf("request %d = %s %s?%s, want %s %s?%s", index, got.Method, got.Path, got.Query, expected.method, expected.path, expected.query)
		}
		if strings.HasPrefix(got.Path, "source:") {
			if got.Header.Get("Authorization") != "" || got.Header.Get("X-Admin-Key") != "" {
				t.Fatal("EPUB download must not carry Primer credentials")
			}
			continue
		}
		if got.Header.Get("Authorization") != "Bearer fixture-oauth-token" || got.Header.Get("X-Admin-Key") != "" {
			t.Fatalf("request %d must use only the delegated Primer token", index)
		}
	}
	first, second, explicit := requests[2].Header.Get("Idempotency-Key"), requests[3].Header.Get("Idempotency-Key"), requests[4].Header.Get("Idempotency-Key")
	if !strings.HasPrefix(first, "sb-") || first != second || explicit != "parent-retry-2" {
		t.Fatalf("idempotency keys: %q %q %q", first, second, explicit)
	}
	var created map[string]string
	if err := json.Unmarshal(requests[2].Body, &created); err != nil || created["author"] != "Jack London" {
		t.Fatalf("create body not forwarded: %s", requests[2].Body)
	}
	if requests[5].Header.Get("Idempotency-Key") != "" {
		t.Fatal("non-idempotent route sent an idempotency key")
	}
	upload := requests[8]
	if !bytes.Equal(upload.Body, epub) {
		t.Fatal("uploaded EPUB bytes differ from the download")
	}
	for header, value := range map[string]string{
		"Content-Type":          "application/epub+zip",
		"X-Content-Sha256":      hex.EncodeToString(digest[:]),
		"X-Content-Length":      strconv.Itoa(len(epub)),
		"X-Rights-Source":       "Standard Ebooks",
		"X-Rights-License":      "Public domain (US)",
		"X-Rights-Acknowledged": "true",
	} {
		if upload.Header.Get(header) != value {
			t.Fatalf("upload header %s = %q, want %q", header, upload.Header.Get(header), value)
		}
	}
	if !strings.HasPrefix(upload.Header.Get("Idempotency-Key"), "sb-") {
		t.Fatal("upload idempotency key missing")
	}
}

func TestPrimerReadingRequiresLMSConfiguration(t *testing.T) {
	var calls int
	server := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { calls++ }))
	defer server.Close()
	ctx := context.Background()
	module := loadPrimer(t, ctx, map[string]string{"tv_base_url": server.URL, "tv_admin_key": "k", "tasks_base_url": server.URL, "tasks_api_key": "t"})
	result := executePrimer(t, ctx, module, "primer_list_reading_works", map[string]any{})
	if !result.IsError || !strings.Contains(result.Data, "lms_base_url") {
		t.Fatalf("missing LMS configuration not reported: %+v", result)
	}
	if calls != 0 {
		t.Fatal("unconfigured LMS request reached the network")
	}
}

func TestPrimerConfirmReadingLinkUsesTasksOAuth(t *testing.T) {
	var got recordedRequest
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		body, _ := io.ReadAll(request.Body)
		got = recordedRequest{request.Method, request.URL.Path, request.URL.RawQuery, request.Header.Clone(), body}
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"state":"linked"}`))
	}))
	defer server.Close()
	ctx := context.Background()
	module := loadPrimer(t, ctx, map[string]string{"tv_base_url": server.URL + "/tv", "tv_admin_key": "fixture-tv-key", "tasks_base_url": server.URL + "/tasks/api", "tasks_api_key": "fixture-oauth-token", "lms_base_url": server.URL + "/api/v1"})
	result := executePrimer(t, ctx, module, "primer_confirm_reading_tasks_link", map[string]any{"body": `{"requestId":"req-1","secret":"one-time"}`})
	if result.IsError || !strings.Contains(result.Data, "linked") {
		t.Fatalf("confirmation failed: %+v", result)
	}
	if got.Method != http.MethodPost || got.Path != "/tasks/api/reading/links/confirm" || got.Header.Get("Authorization") != "Bearer fixture-oauth-token" || got.Header.Get("X-Admin-Key") != "" {
		t.Fatalf("unexpected request: %s %s", got.Method, got.Path)
	}
	var body map[string]string
	if err := json.Unmarshal(got.Body, &body); err != nil || body["requestId"] != "req-1" || body["secret"] != "one-time" {
		t.Fatalf("body not forwarded: %s", got.Body)
	}
	if missing := executePrimer(t, ctx, module, "primer_confirm_reading_tasks_link", map[string]any{}); !missing.IsError {
		t.Fatal("missing body accepted")
	}
}
