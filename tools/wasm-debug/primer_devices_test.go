package main

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
)

func TestPrimerDeviceDiscoveryWASM(t *testing.T) {
	moduleBytes, err := os.ReadFile("../../target/wasm32-wasip1/release/primer_wasm.wasm")
	if err != nil {
		t.Fatalf("build the Primer WASM artifact before this test: %v", err)
	}
	const deviceID = "4b06b1d8-944d-4668-a2e5-a682355ea541"
	var calls atomic.Int32
	var status atomic.Int32
	status.Store(http.StatusOK)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		calls.Add(1)
		if request.Method != http.MethodGet || request.Header.Get("X-Admin-Key") != "fixture-tv-key" || request.Header.Get("Authorization") != "" {
			t.Error("device discovery must use read-only TV administrator authentication, never Tasks OAuth")
		}
		writer.Header().Set("Content-Type", "application/json")
		writer.WriteHeader(int(status.Load()))
		if status.Load() != http.StatusOK {
			_, _ = writer.Write([]byte(`{"error":"fixture-private-key pairingCode=fixture-pairing"}`))
			return
		}
		device := map[string]any{"id": deviceID, "name": "Family TV", "kind": "tv_box", "pairedAt": "2026-09-01T12:00:00Z", "pairingCode": "fixture-pairing", "tokenHash": "fixture-private-key"}
		switch request.URL.Path {
		case "/api/v1/devices":
			if request.URL.Query().Get("q") != "Family & TV" || request.URL.Query().Get("filter") != "kind:tv_box" || request.URL.Query().Get("offset") != "4" {
				t.Error("device search parameters were not forwarded")
			}
			_ = json.NewEncoder(writer).Encode(map[string]any{"items": []any{device}, "totalCount": 7, "limit": 2, "offset": 4})
		case "/api/v1/devices/" + deviceID:
			_ = json.NewEncoder(writer).Encode(device)
		default:
			t.Errorf("unexpected route: %s", request.URL.Path)
		}
	}))
	defer server.Close()
	ctx := context.Background()
	runtime := wazero.NewRuntime(ctx)
	defer runtime.Close(ctx)
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
	configuration, _ := json.Marshal(map[string]string{"tv_base_url": server.URL + "/api/v1", "tv_admin_key": "fixture-tv-key", "tasks_base_url": server.URL + "/tasks/api", "tasks_api_key": "fixture-tasks-key"})
	if result := callWithInput(ctx, module, "configure", configuration); result != "" {
		t.Fatal("fixture configuration rejected")
	}
	var tools []struct{ Name string }
	if err := json.Unmarshal([]byte(callString(ctx, module, "tools")), &tools); err != nil {
		t.Fatal(err)
	}
	registered := map[string]bool{}
	for _, tool := range tools {
		registered[tool.Name] = true
	}
	for _, scenario := range []struct {
		name string
		args map[string]any
	}{
		{"primer_list_tv_devices", map[string]any{"limit": 2, "offset": 4, "q": "Family & TV", "filter": "kind:tv_box"}},
		{"primer_get_tv_device", map[string]any{"id": deviceID}},
	} {
		t.Run(scenario.name, func(t *testing.T) {
			if !registered[scenario.name] {
				t.Fatal("device tool missing from exported discovery")
			}
			input, _ := json.Marshal(map[string]any{"tool_name": scenario.name, "args": scenario.args})
			for _, code := range []int{http.StatusOK, http.StatusUnauthorized, http.StatusForbidden, http.StatusNotFound, http.StatusServiceUnavailable} {
				status.Store(int32(code))
				output := callWithInput(ctx, module, "execute", input)
				if strings.Contains(output, "fixture-private-key") || strings.Contains(output, "fixture-pairing") || strings.Contains(output, "pairingCode") {
					t.Fatal("device credentials escaped the WASM response boundary")
				}
				var result struct {
					Data    string `json:"data"`
					IsError bool   `json:"is_error"`
				}
				if err := json.Unmarshal([]byte(output), &result); err != nil {
					t.Fatal(err)
				}
				if result.IsError != (code != http.StatusOK) {
					t.Fatalf("incorrect result for status %d: %s", code, output)
				}
				if code == http.StatusOK && (!strings.Contains(result.Data, deviceID) || !strings.Contains(result.Data, `"assignmentReady":false`)) {
					t.Fatal("safe device identity/readiness missing")
				}
			}
		})
	}
	if calls.Load() != 10 {
		t.Fatalf("HTTP calls = %d, want 10", calls.Load())
	}
}
