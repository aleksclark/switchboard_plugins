package main

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"net/url"
	"reflect"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"golang.org/x/net/websocket"
)

type cdpTestCommand struct {
	ID      int            `json:"id"`
	Method  string         `json:"method"`
	Session string         `json:"sessionId"`
	Params  map[string]any `json:"params"`
}

type cdpTestServer struct {
	mu          sync.Mutex
	commands    []cdpTestCommand
	connections int
	done        chan struct{}
}

func (fake *cdpTestServer) snapshot() ([]cdpTestCommand, int) {
	fake.mu.Lock()
	defer fake.mu.Unlock()
	return append([]cdpTestCommand(nil), fake.commands...), fake.connections
}

func newCDPTestServer(t *testing.T, outcome string) (*cdpTestServer, string) {
	t.Helper()
	fake := &cdpTestServer{done: make(chan struct{}, 4)}
	handler := websocket.Server{Handler: func(ws *websocket.Conn) {
		defer ws.Close()
		defer func() { fake.done <- struct{}{} }()
		for _, header := range []string{"Origin", "Authorization", "Proxy-Authorization", "Cookie", "Referer"} {
			if hasHeader(ws.Request().Header, header) {
				t.Error("native CDP client sent a forbidden handshake header")
			}
		}
		if err := ws.SetDeadline(time.Now().Add(5 * time.Second)); err != nil {
			t.Error("fake CDP deadline failed")
			return
		}
		fake.mu.Lock()
		fake.connections++
		fake.mu.Unlock()
		for {
			var command cdpTestCommand
			if websocket.JSON.Receive(ws, &command) != nil {
				return
			}
			fake.mu.Lock()
			fake.commands = append(fake.commands, command)
			fake.mu.Unlock()
			if command.Method == "Runtime.evaluate" {
				switch outcome {
				case "lost":
					return
				case "cancel":
					var next cdpTestCommand
					if websocket.JSON.Receive(ws, &next) == nil {
						t.Error("sent a second command while evaluation was pending")
					}
					return
				case "oversize":
					_ = websocket.Message.Send(ws, strings.Repeat("x", maxBrowserOutput+1))
					return
				case "event_flood":
					event := `{"method":"ignored","params":{"data":"` + strings.Repeat("x", maxBrowserOutput-100) + `"}}`
					for range 10 {
						if websocket.Message.Send(ws, event) != nil {
							return
						}
					}
					return
				}
			}
			var result any
			switch command.Method {
			case "Target.getTargets":
				targets := []cdpTarget{{ID: "decoy", Type: "page", URL: "https://api.primerlms.com.evil/tasks/"}, {ID: "chosen", Type: "page", URL: "https://api.primerlms.com/tasks/"}}
				if outcome == "ambiguous" {
					targets = append(targets, targets[1])
				}
				result = map[string]any{"targetInfos": targets}
			case "Target.attachToTarget":
				if command.Params["targetId"] != "chosen" || command.Params["flatten"] != true || len(command.Params) != 2 {
					t.Error("attach must target the existing tab with flatten true")
				}
				result = map[string]any{"sessionId": "attached"}
			case "Runtime.evaluate":
				if command.Session != "attached" || command.Params["awaitPromise"] != true || command.Params["returnByValue"] != true || len(command.Params) != 3 {
					t.Error("evaluation must await and return by value on the attached session only")
				}
				status := 201
				if outcome == "browser_failure" {
					status = 502
				}
				result = map[string]any{"result": map[string]any{"type": "object", "value": map[string]any{"status": status, "body": map[string]any{"ok": true}}}}
				if outcome == "exception" {
					result = map[string]any{"exceptionDetails": map[string]any{"text": "synthetic-secret-exception"}}
				}
				if outcome == "token_envelope" {
					result = map[string]any{"result": map[string]any{"type": "object", "value": map[string]any{"status": 201, "body": nil, "token": "synthetic-secret-token"}}}
				}
			case "Target.detachFromTarget":
				if command.Session != "" || command.Params["sessionId"] != "attached" || len(command.Params) != 1 {
					t.Error("detach must release the attached session")
				}
				if outcome == "detach_stall" {
					var next cdpTestCommand
					if websocket.JSON.Receive(ws, &next) == nil {
						t.Error("sent another command while detach was pending")
					}
					return
				}
				result = map[string]any{}
			default:
				t.Error("unexpected CDP command: no activation, navigation, capture, or retries allowed")
				return
			}
			if websocket.JSON.Send(ws, map[string]any{"method": "Target.syntheticEvent", "params": map[string]any{"secret": "never-log-event"}}) != nil {
				return
			}
			reply := map[string]any{"id": command.ID, "result": result}
			if command.Session != "" {
				reply["sessionId"] = command.Session
			}
			if command.Method == "Runtime.evaluate" {
				switch outcome {
				case "wrong_id":
					reply["id"] = command.ID + 1
				case "wrong_session":
					reply["sessionId"] = "other-session"
				case "protocol_error":
					delete(reply, "result")
					reply["error"] = map[string]any{"message": "synthetic-secret-CDP-error"}
				case "malformed":
					_ = websocket.Message.Send(ws, "synthetic-secret-invalid-json")
					return
				}
			}
			if websocket.JSON.Send(ws, reply) != nil {
				return
			}
		}
	}}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if hasHeader(r.Header, "Origin") {
			http.Error(w, "Rejected WebSocket connection with Origin", http.StatusForbidden)
			return
		}
		if r.URL.Path != "/devtools/browser/test" {
			t.Error("did not use the discovered browser endpoint")
			http.NotFound(w, r)
			return
		}
		handler.ServeHTTP(w, r)
	}))
	t.Cleanup(server.Close)
	return fake, "ws" + strings.TrimPrefix(server.URL, "http") + "/devtools/browser/test"
}

func waitCDPClosed(t *testing.T, fake *cdpTestServer) {
	t.Helper()
	select {
	case <-fake.done:
	case <-time.After(2 * time.Second):
		t.Fatal("CDP socket was not closed promptly")
	}
}

func TestCDPChromeStyleOriginPolicy(t *testing.T) {
	fake, endpoint := newCDPTestServer(t, "success")
	client := &http.Client{Transport: &http.Transport{}}
	defer client.CloseIdleConnections()
	for _, origin := range []string{"", "null", "http://127.0.0.1", "https://api.primerlms.com"} {
		t.Run(origin, func(t *testing.T) {
			request, err := http.NewRequestWithContext(t.Context(), http.MethodGet, "http"+strings.TrimPrefix(endpoint, "ws"), nil)
			if err != nil {
				t.Fatal(err)
			}
			request.Header.Set("Origin", origin)
			request.Header.Set("Connection", "Upgrade")
			request.Header.Set("Upgrade", "websocket")
			request.Header.Set("Sec-WebSocket-Version", "13")
			request.Header.Set("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
			response, err := client.Do(request)
			if err != nil {
				t.Fatal(err)
			}
			defer response.Body.Close()
			if response.StatusCode != http.StatusForbidden {
				t.Fatal("fake Chrome accepted an Origin header")
			}
		})
	}
	if commands, connections := fake.snapshot(); len(commands) != 0 || connections != 0 {
		t.Fatal("rejected handshake submitted CDP commands")
	}
	result, err := evaluateCDP(t.Context(), endpoint, "synthetic expression")
	if err != nil || result.Status != 201 || string(result.Body) != `{"ok":true}` {
		t.Fatal("native no-Origin CDP evaluation failed")
	}
	waitCDPClosed(t, fake)
	if commands, connections := fake.snapshot(); len(commands) != 4 || connections != 1 {
		t.Fatal("native CDP client retried or sent unexpected commands")
	}
}

func TestCDPTransport(t *testing.T) {
	for _, outcome := range []string{"success", "lost", "oversize", "event_flood", "wrong_id", "wrong_session", "protocol_error", "malformed", "exception", "token_envelope", "ambiguous", "browser_failure"} {
		t.Run(outcome, func(t *testing.T) {
			fake, endpoint := newCDPTestServer(t, outcome)
			fixture, _ := json.Marshal(map[string]any{"connection": "connected", "mode": "managed", "endpoint": map[string]any{"ws_url": endpoint}})
			cfg := scriptTestConfig()
			cfg.BrowserCommand = statusTestCommand(t, string(fixture))
			runner := commandRunner{cfg: cfg}
			request := scriptTestRequest("POST")
			if outcome == "success" {
				request.Body = json.RawMessage(`{"data":"` + strings.Repeat("a", maxRequestBody-len(`{"data":""}`)) + `"}`)
			}
			start := time.Now()
			result, err := runner.Run(t.Context(), request)
			wantSuccess := outcome == "success" || outcome == "browser_failure"
			if (err == nil) != wantSuccess {
				t.Fatalf("Run success=%v, want %v", err == nil, wantSuccess)
			}
			if !wantSuccess && (err != errBrowser || result.Status != 0 || len(result.Body) != 0) {
				t.Fatal("failed transport exposed private details")
			}
			if wantSuccess {
				status := 201
				if outcome == "browser_failure" {
					status = 502
				}
				if result.Status != status || string(result.Body) != `{"ok":true}` {
					t.Fatal("incorrect CDP return-by-value result")
				}
			}
			waitCDPClosed(t, fake)
			commands, connections := fake.snapshot()
			if connections != 1 {
				t.Fatal("transport reconnected")
			}
			var methods []string
			for i, command := range commands {
				methods = append(methods, command.Method)
				if command.ID != i+1 {
					t.Error("command IDs are not unique and sequential")
				}
				if command.Method == "Runtime.evaluate" {
					expression, ok := command.Params["expression"].(string)
					if !ok {
						t.Fatal("expression not a string")
					}
					payload := testScriptPayload(t, expression)
					var original browserRequest
					if json.Unmarshal(payload["request"], &original) != nil || original.Method != "POST" || original.Path != request.Path {
						t.Fatal("evaluation changed the request")
					}
					var body string
					if json.Unmarshal(payload["bodyText"], &body) != nil || body != string(request.Body) {
						t.Fatal("maximum body was not preserved exclusively in the WebSocket payload")
					}
					var expiry int64
					if json.Unmarshal(payload["expiresAt"], &expiry) != nil || expiry < start.UnixMilli() || expiry > start.Add(browserTimeout).UnixMilli()+10 {
						t.Error("expiry was not issued by Go before discovery")
					}
					if strings.Contains(expression, cfg.LocalAPIKey) || strings.Contains(expression, cfg.BrowserCommand) {
						t.Error("evaluation contains local-only secrets")
					}
				}
			}
			want := []string{"Target.getTargets", "Target.attachToTarget", "Runtime.evaluate"}
			switch outcome {
			case "ambiguous":
				want = want[:1]
			case "success", "browser_failure", "exception", "protocol_error", "token_envelope":
				want = append(want, "Target.detachFromTarget")
			}
			if !reflect.DeepEqual(methods, want) {
				t.Errorf("commands=%v, want %v; never activate, navigate, or resend", methods, want)
			}
			if outcome != "success" {
				if !runner.unavailable.Load() {
					t.Error("failure did not latch unavailable")
				}
				if _, err := runner.Run(t.Context(), request); err != errBrowser {
					t.Error("runner allowed execution after uncertain result")
				}
				again, connectionsAgain := fake.snapshot()
				if len(again) != len(commands) || connectionsAgain != 1 {
					t.Error("latched runner resubmitted commands")
				}
			}
		})
	}
}

func TestCDPCancellation(t *testing.T) {
	fake, endpoint := newCDPTestServer(t, "cancel")
	ctx, cancel := context.WithCancel(t.Context())
	defer cancel()
	done := make(chan error, 1)
	go func() {
		_, err := evaluateCDP(ctx, endpoint, "synthetic expression")
		done <- err
	}()
	deadline := time.After(2 * time.Second)
	for {
		commands, _ := fake.snapshot()
		if len(commands) == 3 {
			break
		}
		select {
		case <-deadline:
			t.Fatal("evaluation not submitted")
		case <-time.After(time.Millisecond):
		}
	}
	cancel()
	select {
	case err := <-done:
		if err != errBrowser {
			t.Error("canceled transport did not fail closed")
		}
	case <-time.After(time.Second):
		t.Fatal("cancellation did not close CDP socket")
	}
	waitCDPClosed(t, fake)
	commands, count := fake.snapshot()
	if len(commands) != 3 || count != 1 {
		t.Fatal("cancellation sent another command or reconnected")
	}
}

func TestCDPDetachDeadline(t *testing.T) {
	fake, endpoint := newCDPTestServer(t, "detach_stall")
	start := time.Now()
	result, err := evaluateCDP(t.Context(), endpoint, "synthetic expression")
	if err != nil || result.Status != 201 {
		t.Fatal("best-effort detach changed a successful evaluation")
	}
	if time.Since(start) > time.Second {
		t.Fatal("detach exceeded its cleanup deadline")
	}
	waitCDPClosed(t, fake)
	commands, connections := fake.snapshot()
	if len(commands) != 4 || commands[3].Method != "Target.detachFromTarget" || connections != 1 {
		t.Fatal("stalled detach retried or sent unexpected commands")
	}
}

func TestCDPTargetSelection(t *testing.T) {
	valid := cdpTarget{ID: "chosen", Type: "page", URL: "https://api.primerlms.com/tasks/"}
	for _, address := range []string{
		valid.URL, "https://api.primerlms.com/tasks/students?q=1#details",
		"http://api.primerlms.com/tasks/", "https://api.primerlms.com.evil/tasks/",
		"https://evil.api.primerlms.com/tasks/", "https://api.primerlms.com@evil.test/tasks/",
		"https://user@api.primerlms.com/tasks/", "https://api.primerlms.com:8443/tasks/",
		"https://api.primerlms.com/tasks", "https://api.primerlms.com/tasks-other/",
		"https://api.primerlms.com/Tasks/", "https://api.primerlms.com/%74asks/",
		"https://api.primerlms.com/?next=/tasks/", "https://api.primerlms.com/#/tasks/",
	} {
		t.Run(address, func(t *testing.T) {
			target := cdpTarget{ID: "candidate", Type: "page", URL: address}
			_, err := selectPrimerTarget([]cdpTarget{target})
			want := address == valid.URL || strings.Contains(address, "/tasks/students")
			if (err == nil) != want {
				t.Error("incorrect exact Primer tab matching")
			}
			if !want {
				if id, err := selectPrimerTarget([]cdpTarget{target, valid}); err != nil || id != valid.ID {
					t.Error("decoy target interfered with exact match")
				}
			}
		})
	}
	for i, targets := range [][]cdpTarget{
		nil, {valid, valid}, {valid, {Type: "page", URL: valid.URL}},
		{{Type: "page", URL: valid.URL}}, {{ID: "worker", Type: "service_worker", URL: valid.URL}},
	} {
		t.Run(fmt.Sprint(i), func(t *testing.T) {
			if _, err := selectPrimerTarget(targets); err != errBrowser {
				t.Fatal("accepted missing/ambiguous/non-page tab")
			}
		})
	}
}

func testScriptPayload(t *testing.T, expression string) map[string]json.RawMessage {
	t.Helper()
	_, literal, ok := strings.Cut(expression, "const input = JSON.parse(")
	if !ok {
		t.Fatal("no script payload")
	}
	var data string
	if json.NewDecoder(strings.NewReader(literal)).Decode(&data) != nil {
		t.Fatal("invalid script payload literal")
	}
	fields, err := objectFields([]byte(data))
	if err != nil {
		t.Fatal("invalid script payload")
	}
	return fields
}

func TestCDPIgnoresHTTPDefaultsAndProxyEnvironment(t *testing.T) {
	var defaultRequests, proxyRequests atomic.Int32
	proxy := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		proxyRequests.Add(1)
		http.Error(w, "unexpected proxy request", http.StatusBadGateway)
	}))
	defer proxy.Close()
	for _, name := range []string{"HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"} {
		t.Setenv(name, "http://synthetic-user:synthetic-secret@"+strings.TrimPrefix(proxy.URL, "http://"))
	}
	t.Setenv("NO_PROXY", "")
	t.Setenv("no_proxy", "")
	transport := &http.Transport{Proxy: func(r *http.Request) (*url.URL, error) {
		defaultRequests.Add(1)
		r.Header.Set("Authorization", "Bearer synthetic-secret")
		r.Header.Set("Cookie", "session=synthetic-secret")
		return nil, nil
	}}
	defer transport.CloseIdleConnections()
	originalClient, originalTransport := http.DefaultClient, http.DefaultTransport
	http.DefaultClient = &http.Client{Transport: transport}
	http.DefaultTransport = transport
	defer func() {
		http.DefaultClient, http.DefaultTransport = originalClient, originalTransport
	}()
	fake, endpoint := newCDPTestServer(t, "success")
	result, err := evaluateCDP(t.Context(), endpoint, "synthetic expression")
	if err != nil || result.Status != 201 {
		t.Fatal("native CDP client did not use its own direct transport")
	}
	waitCDPClosed(t, fake)
	if defaultRequests.Load() != 0 || proxyRequests.Load() != 0 {
		t.Fatal("CDP client used HTTP defaults or a proxy")
	}
}

func TestCDPRejectsInvalidEndpointsBeforeHandshake(t *testing.T) {
	var attempts atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts.Add(1)
		http.Error(w, "unexpected handshake", http.StatusForbidden)
	}))
	defer server.Close()
	endpoint := "ws" + strings.TrimPrefix(server.URL, "http") + "/devtools/browser/test"
	for _, address := range []string{
		strings.Replace(endpoint, "127.0.0.1", "localhost", 1),
		strings.Replace(endpoint, "127.0.0.1", "[::1]", 1),
		strings.Replace(endpoint, "127.0.0.1", "192.0.2.1", 1),
		strings.Replace(endpoint, "ws://", "wss://", 1),
		strings.Replace(endpoint, "ws://", "http://", 1),
		strings.Replace(endpoint, "ws://", "ws://synthetic-user:synthetic-secret@", 1),
		strings.Replace(endpoint, "/browser/", "/page/", 1),
		endpoint + "?token=synthetic-secret", endpoint + "#synthetic-secret",
		endpoint + "/more", endpoint + "%2f", endpoint + "\r\nOrigin: synthetic-secret",
		"ws://127.0.0.1:0/devtools/browser/test", "ws://127.0.0.1:65536/devtools/browser/test",
	} {
		result, err := evaluateCDP(t.Context(), address, "unused")
		if err != errBrowser || result.Status != 0 || len(result.Body) != 0 {
			t.Fatal("invalid endpoint did not fail closed")
		}
	}
	if attempts.Load() != 0 {
		t.Fatal("invalid endpoint reached a handshake")
	}
}

func TestCDPHandshakeFailures(t *testing.T) {
	for _, outcome := range []string{"forbidden", "oversized_headers", "stalled_error_body"} {
		t.Run(outcome, func(t *testing.T) {
			var attempts atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				attempts.Add(1)
				switch outcome {
				case "oversized_headers":
					w.Header().Set("X-Synthetic-Secret", strings.Repeat("x", maxBrowserOutput+1))
				case "stalled_error_body":
					w.Header().Set("Content-Length", "1024")
					w.WriteHeader(http.StatusForbidden)
					w.(http.Flusher).Flush()
					<-r.Context().Done()
					return
				}
				http.Error(w, "synthetic-secret-handshake-error", http.StatusForbidden)
			}))
			defer server.Close()
			ctx, cancel := context.WithTimeout(t.Context(), 150*time.Millisecond)
			defer cancel()
			start := time.Now()
			endpoint := "ws" + strings.TrimPrefix(server.URL, "http") + "/devtools/browser/test"
			result, err := evaluateCDP(ctx, endpoint, "unused")
			if err != errBrowser || result.Status != 0 || len(result.Body) != 0 {
				t.Fatal("failed handshake exposed private details")
			}
			if attempts.Load() != 1 {
				t.Fatal("failed handshake was retried")
			}
			if time.Since(start) > time.Second {
				t.Fatal("failed handshake ignored context deadline")
			}
		})
	}
}

func TestCDPRejectsHandshakeRedirects(t *testing.T) {
	for _, status := range []int{300, 301, 302, 303, 304, 305, 306, 307, 308, 399} {
		t.Run(fmt.Sprint(status), func(t *testing.T) {
			var redirected atomic.Int32
			target := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				redirected.Add(1)
				http.Error(w, "unexpected redirect", http.StatusInternalServerError)
			}))
			defer target.Close()
			var attempts atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				attempts.Add(1)
				w.Header().Set("Location", target.URL+"/devtools/browser/redirected")
				w.WriteHeader(status)
			}))
			defer server.Close()
			endpoint := "ws" + strings.TrimPrefix(server.URL, "http") + "/devtools/browser/test"
			result, err := evaluateCDP(t.Context(), endpoint, "unused")
			if err != errBrowser || result.Status != 0 || len(result.Body) != 0 {
				t.Fatal("redirect handshake did not fail closed")
			}
			if attempts.Load() != 1 || redirected.Load() != 0 {
				t.Fatal("transport retried or followed a handshake redirect")
			}
		})
	}
}

func TestCDPHandshakeDeadline(t *testing.T) {
	listener, err := net.Listen("tcp4", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	closed := make(chan struct{})
	go func() {
		defer close(closed)
		conn, err := listener.Accept()
		if err != nil {
			return
		}
		defer conn.Close()
		_ = conn.SetDeadline(time.Now().Add(2 * time.Second))
		_, _ = io.Copy(io.Discard, conn)
	}()
	ctx, cancel := context.WithTimeout(t.Context(), 150*time.Millisecond)
	defer cancel()
	start := time.Now()
	endpoint := "ws://" + listener.Addr().String() + "/devtools/browser/test"
	if result, err := evaluateCDP(ctx, endpoint, "unused"); err != errBrowser || result.Status != 0 {
		t.Fatal("stalled handshake not rejected")
	}
	if time.Since(start) > time.Second {
		t.Fatal("handshake ignored context deadline")
	}
	select {
	case <-closed:
	case <-time.After(time.Second):
		t.Fatal("canceled handshake socket not closed")
	}
}
