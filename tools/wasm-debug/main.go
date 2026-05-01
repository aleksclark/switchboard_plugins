package main

import (
	"bytes"
	"context"
	"crypto/tls"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
	"golang.org/x/net/http2"
)

type httpRequest struct {
	Method     string            `json:"method"`
	URL        string            `json:"url"`
	Headers    map[string]string `json:"headers,omitempty"`
	Body       string            `json:"body,omitempty"`
	BodyBase64 string            `json:"body_base64,omitempty"`
}

type httpResponse struct {
	Status     int               `json:"status"`
	Headers    map[string]string `json:"headers,omitempty"`
	Body       string            `json:"body"`
	BodyBase64 string            `json:"body_base64,omitempty"`
}

var (
	httpClient = &http.Client{Timeout: 30 * time.Second}
	h2cClient  = &http.Client{
		Timeout: 30 * time.Second,
		Transport: &http2.Transport{
			AllowHTTP: true,
			DialTLSContext: func(ctx context.Context, network, addr string, _ *tls.Config) (net.Conn, error) {
				return (&net.Dialer{}).DialContext(ctx, network, addr)
			},
		},
	}
	verbose bool
)

func main() {
	if len(os.Args) < 3 {
		fmt.Fprintf(os.Stderr, "Usage: wasm-debug <plugin.wasm> <command> [args...]\n\n")
		fmt.Fprintf(os.Stderr, "Commands:\n")
		fmt.Fprintf(os.Stderr, "  name                     Call name()\n")
		fmt.Fprintf(os.Stderr, "  metadata                 Call metadata()\n")
		fmt.Fprintf(os.Stderr, "  tools                    Call tools()\n")
		fmt.Fprintf(os.Stderr, "  configure <json>         Call configure() with credentials JSON\n")
		fmt.Fprintf(os.Stderr, "  execute <tool> [json]    Call execute() with tool name and args\n")
		fmt.Fprintf(os.Stderr, "  healthy                  Call healthy()\n")
		fmt.Fprintf(os.Stderr, "\nFlags:\n")
		fmt.Fprintf(os.Stderr, "  -v                       Verbose: log HTTP requests/responses\n")
		os.Exit(1)
	}

	args := os.Args[1:]
	for i, a := range args {
		if a == "-v" {
			verbose = true
			args = append(args[:i], args[i+1:]...)
			break
		}
	}

	wasmPath := args[0]
	command := args[1]

	wasmBytes, err := os.ReadFile(wasmPath)
	if err != nil {
		log.Fatalf("read wasm: %v", err)
	}

	ctx := context.Background()
	rt := wazero.NewRuntime(ctx)
	defer rt.Close(ctx)

	wasi_snapshot_preview1.MustInstantiate(ctx, rt)

	_, err = rt.NewHostModuleBuilder("env").
		NewFunctionBuilder().WithFunc(hostHTTPRequest).WithParameterNames("ptr_size").Export("host_http_request").
		NewFunctionBuilder().WithFunc(hostLog).WithParameterNames("ptr", "size").Export("host_log").
		Instantiate(ctx)
	if err != nil {
		log.Fatalf("host module: %v", err)
	}

	compiled, err := rt.CompileModule(ctx, wasmBytes)
	if err != nil {
		log.Fatalf("compile: %v", err)
	}

	mod, err := rt.InstantiateModule(ctx, compiled, wazero.NewModuleConfig().WithStartFunctions().WithName(""))
	if err != nil {
		log.Fatalf("instantiate: %v", err)
	}
	defer mod.Close(ctx)

	initFn := mod.ExportedFunction("_rt0_wasm_wasip1")
	if initFn == nil {
		initFn = mod.ExportedFunction("_initialize")
	}
	if initFn != nil {
		if _, err := initFn.Call(ctx); err != nil {
			log.Fatalf("init: %v", err)
		}
	}

	switch command {
	case "name":
		fmt.Println(callString(ctx, mod, "name"))

	case "metadata":
		fmt.Println(callString(ctx, mod, "metadata"))

	case "tools":
		fmt.Println(callString(ctx, mod, "tools"))

	case "healthy":
		fn := mod.ExportedFunction("healthy")
		results, err := fn.Call(ctx)
		if err != nil {
			log.Fatalf("healthy: %v", err)
		}
		fmt.Printf("healthy: %d\n", int32(results[0]))

	case "configure":
		if len(args) < 3 {
			log.Fatal("configure requires JSON argument")
		}
		result := callWithInput(ctx, mod, "configure", []byte(args[2]))
		if result == "" {
			fmt.Println("configured OK (returned 0)")
		} else {
			fmt.Printf("configure error: %s\n", result)
		}

	case "execute":
		if len(args) < 3 {
			log.Fatal("execute requires tool name")
		}
		toolName := args[2]
		toolArgs := "{}"
		if len(args) >= 4 {
			toolArgs = args[3]
		}

		execReq := map[string]any{
			"tool_name": toolName,
			"args":      json.RawMessage(toolArgs),
		}
		execJSON, _ := json.Marshal(execReq)
		result := callWithInput(ctx, mod, "execute", execJSON)
		fmt.Println(result)

	case "run":
		if len(args) < 4 {
			log.Fatal("run requires: <creds-json> <tool> [args-json]")
		}
		credsJSON := args[2]
		toolName := args[3]
		toolArgs := "{}"
		if len(args) >= 5 {
			toolArgs = args[4]
		}

		cfgResult := callWithInput(ctx, mod, "configure", []byte(credsJSON))
		if cfgResult != "" {
			log.Fatalf("configure failed: %s", cfgResult)
		}

		execReq := map[string]any{
			"tool_name": toolName,
			"args":      json.RawMessage(toolArgs),
		}
		execJSON, _ := json.Marshal(execReq)
		result := callWithInput(ctx, mod, "execute", execJSON)
		fmt.Println(result)

	default:
		log.Fatalf("unknown command: %s", command)
	}
}

func callString(ctx context.Context, mod api.Module, fnName string) string {
	fn := mod.ExportedFunction(fnName)
	if fn == nil {
		log.Fatalf("function %q not exported", fnName)
	}
	results, err := fn.Call(ctx)
	if err != nil {
		log.Fatalf("%s: %v", fnName, err)
	}
	if len(results) == 0 || results[0] == 0 {
		return ""
	}
	ptr, size := unpack(results[0])
	data, ok := mod.Memory().Read(ptr, size)
	if !ok {
		log.Fatalf("%s: memory read failed at %d+%d", fnName, ptr, size)
	}
	return string(data)
}

func callWithInput(ctx context.Context, mod api.Module, fnName string, input []byte) string {
	fn := mod.ExportedFunction(fnName)
	if fn == nil {
		log.Fatalf("function %q not exported", fnName)
	}

	ptr, size := writeGuest(ctx, mod, input)
	packed := pack(ptr, size)

	results, err := fn.Call(ctx, packed)
	if err != nil {
		log.Fatalf("%s: %v", fnName, err)
	}

	if len(results) == 0 || results[0] == 0 {
		return ""
	}

	rPtr, rSize := unpack(results[0])
	if rSize == 0 {
		return ""
	}
	data, ok := mod.Memory().Read(rPtr, rSize)
	if !ok {
		log.Fatalf("%s: memory read failed at %d+%d", fnName, rPtr, rSize)
	}
	return string(data)
}

func writeGuest(ctx context.Context, mod api.Module, data []byte) (uint32, uint32) {
	malloc := mod.ExportedFunction("malloc")
	if malloc == nil {
		malloc = mod.ExportedFunction("guest_malloc")
	}
	if malloc == nil {
		log.Fatal("module does not export malloc/guest_malloc")
	}
	results, err := malloc.Call(ctx, uint64(len(data)))
	if err != nil {
		log.Fatalf("malloc: %v", err)
	}
	ptr := uint32(results[0])
	if !mod.Memory().Write(ptr, data) {
		log.Fatalf("memory write failed at %d", ptr)
	}
	return ptr, uint32(len(data))
}

func pack(ptr, size uint32) uint64   { return (uint64(ptr) << 32) | uint64(size) }
func unpack(v uint64) (uint32, uint32) { return uint32(v >> 32), uint32(v) }

// Host functions

func hostHTTPRequest(ctx context.Context, mod api.Module, ptrSize uint64) uint64 {
	ptr, size := unpack(ptrSize)
	reqData, ok := mod.Memory().Read(ptr, size)
	if !ok {
		return writeError(ctx, mod, "read request: out of range")
	}

	var req httpRequest
	if err := json.Unmarshal(reqData, &req); err != nil {
		return writeError(ctx, mod, fmt.Sprintf("parse request: %v", err))
	}

	if verbose {
		fmt.Fprintf(os.Stderr, "[HTTP] %s %s\n", req.Method, req.URL)
		for k, v := range req.Headers {
			fmt.Fprintf(os.Stderr, "[HTTP]   %s: %s\n", k, v)
		}
		if req.BodyBase64 != "" {
			dec, _ := base64.StdEncoding.DecodeString(req.BodyBase64)
			fmt.Fprintf(os.Stderr, "[HTTP]   body_base64: %d bytes\n", len(dec))
		} else if req.Body != "" {
			fmt.Fprintf(os.Stderr, "[HTTP]   body: %s\n", truncate(req.Body, 200))
		}
	}

	var bodyReader io.Reader
	if req.BodyBase64 != "" {
		decoded, err := base64.StdEncoding.DecodeString(req.BodyBase64)
		if err != nil {
			return writeError(ctx, mod, fmt.Sprintf("decode body_base64: %v", err))
		}
		bodyReader = bytes.NewReader(decoded)
	} else if req.Body != "" {
		bodyReader = strings.NewReader(req.Body)
	}

	httpReq, err := http.NewRequestWithContext(ctx, req.Method, req.URL, bodyReader)
	if err != nil {
		return writeError(ctx, mod, fmt.Sprintf("create request: %v", err))
	}

	useH2C := req.Headers["X-H2C"] != ""
	rawBody := req.Headers["X-Raw-Body"] != ""
	for k, v := range req.Headers {
		if k == "X-H2C" || k == "X-Raw-Body" {
			continue
		}
		httpReq.Header.Set(k, v)
	}

	client := httpClient
	if useH2C {
		client = h2cClient
	}

	resp, err := client.Do(httpReq)
	if err != nil {
		return writeError(ctx, mod, fmt.Sprintf("http: %v", err))
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(io.LimitReader(resp.Body, 10*1024*1024))
	if err != nil {
		return writeError(ctx, mod, fmt.Sprintf("read response: %v", err))
	}

	headers := make(map[string]string)
	for k := range resp.Header {
		headers[k] = resp.Header.Get(k)
	}

	result := &httpResponse{Status: resp.StatusCode, Headers: headers}
	if rawBody {
		result.BodyBase64 = base64.StdEncoding.EncodeToString(body)
	} else {
		result.Body = string(body)
	}

	if verbose {
		fmt.Fprintf(os.Stderr, "[HTTP] <- %d (%d bytes", resp.StatusCode, len(body))
		if rawBody {
			fmt.Fprintf(os.Stderr, ", base64")
		}
		fmt.Fprintf(os.Stderr, ")\n")
		for k, v := range headers {
			fmt.Fprintf(os.Stderr, "[HTTP]   %s: %s\n", k, v)
		}
		if !rawBody && len(body) > 0 {
			fmt.Fprintf(os.Stderr, "[HTTP]   body: %s\n", truncate(string(body), 200))
		}
	}

	resultData, _ := json.Marshal(result)
	rPtr, rSize := writeGuest(ctx, mod, resultData)
	return pack(rPtr, rSize)
}

func writeError(ctx context.Context, mod api.Module, msg string) uint64 {
	result := httpResponse{Status: 0, Body: msg}
	data, _ := json.Marshal(result)
	rPtr, rSize := writeGuest(ctx, mod, data)
	return pack(rPtr, rSize)
}

func hostLog(_ context.Context, mod api.Module, ptr, size uint32) {
	data, ok := mod.Memory().Read(ptr, size)
	if !ok {
		return
	}
	fmt.Fprintf(os.Stderr, "[LOG] %s\n", string(data))
}

func truncate(s string, max int) string {
	if len(s) <= max {
		return s
	}
	return s[:max] + "..."
}
