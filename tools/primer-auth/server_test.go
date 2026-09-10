package main

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

const (
	serverTestReadFailure  = `{"error":"Browser session or upstream request unavailable."}`
	serverTestWriteFailure = `{"error":"Browser request failed; write outcome unknown. Check state before any manual retry."}`
)

type serverTestRunner struct {
	mu       sync.Mutex
	requests []browserRequest
	run      func(context.Context, browserRequest) (browserResult, error)
}

var _ browserRunner = (*serverTestRunner)(nil)

func (runner *serverTestRunner) Run(ctx context.Context, request browserRequest) (browserResult, error) {
	request.Body = bytes.Clone(request.Body)
	runner.mu.Lock()
	runner.requests = append(runner.requests, request)
	runner.mu.Unlock()
	if runner.run != nil {
		return runner.run(ctx, request)
	}
	return browserResult{Status: http.StatusOK, Body: json.RawMessage(`{"synthetic":true}`)}, nil
}

func (runner *serverTestRunner) calls() []browserRequest {
	runner.mu.Lock()
	defer runner.mu.Unlock()
	requests := make([]browserRequest, len(runner.requests))
	for i, request := range runner.requests {
		requests[i] = request
		requests[i].Body = bytes.Clone(request.Body)
	}
	return requests
}

type serverTestLog struct {
	mu   sync.Mutex
	data bytes.Buffer
}

func (log *serverTestLog) Write(data []byte) (int, error) {
	log.mu.Lock()
	defer log.mu.Unlock()
	return log.data.Write(data)
}

func (log *serverTestLog) String() string {
	log.mu.Lock()
	defer log.mu.Unlock()
	return log.data.String()
}

func serverTestNew(runner browserRunner) (*companion, *serverTestLog) {
	logs := new(serverTestLog)
	logger := slog.New(slog.NewJSONHandler(logs, nil))
	return newCompanion(configTestValid(), runner, logger), logs
}

func serverTestRequest(method, target, body string) *http.Request {
	request := httptest.NewRequest(method, target, strings.NewReader(body))
	request.Host = "127.0.0.1:9876"
	request.Header.Set("Authorization", "Bearer "+configTestValid().LocalAPIKey)
	if body != "" {
		request.Header.Set("Content-Type", "application/json")
	}
	return request
}

func serverTestResponse(t *testing.T, response *httptest.ResponseRecorder, status int, body string) {
	t.Helper()
	if response.Code != status || response.Body.String() != body {
		t.Errorf("response = %d %q; want %d %q", response.Code, response.Body.String(), status, body)
	}
	for key, want := range map[string]string{"Content-Type": "application/json", "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff"} {
		if got := response.Header().Get(key); got != want {
			t.Errorf("header %s = %q, want %q", key, got, want)
		}
	}
	if !json.Valid(response.Body.Bytes()) {
		t.Errorf("invalid response JSON: %q", response.Body.String())
	}
	for _, key := range []string{"Access-Control-Allow-Origin", "Access-Control-Allow-Credentials", "Location", "Set-Cookie"} {
		if hasHeader(response.Header(), key) {
			t.Errorf("unexpected response header %s", key)
		}
	}
}

func serverTestRejection(t *testing.T, response *httptest.ResponseRecorder, status int) {
	t.Helper()
	data, err := json.Marshal(map[string]string{"error": http.StatusText(status)})
	if err != nil {
		t.Fatal(err)
	}
	serverTestResponse(t, response, status, string(data))
	wantChallenge := ""
	if status == http.StatusUnauthorized {
		wantChallenge = "Bearer"
	}
	if got := response.Header().Get("WWW-Authenticate"); got != wantChallenge {
		t.Errorf("authentication challenge = %q, want %q", got, wantChallenge)
	}
}

func serverTestLogEntry(t *testing.T, logs *serverTestLog, event string, status int) {
	t.Helper()
	decoder := json.NewDecoder(strings.NewReader(logs.String()))
	var fields map[string]any
	if err := decoder.Decode(&fields); err != nil {
		t.Fatalf("decode log: %v; log=%q", err, logs.String())
	}
	if len(fields) != 4 || fields["level"] != "INFO" || fields["msg"] != event || fields["status"] != float64(status) {
		t.Errorf("unexpected log fields: %#v", fields)
	}
	if _, ok := fields["time"].(string); !ok {
		t.Errorf("log has no timestamp: %#v", fields)
	}
	if err := decoder.Decode(new(any)); err != io.EOF {
		t.Errorf("expected exactly one log entry, got %v", err)
	}
}

func TestNewCompanion(t *testing.T) {
	runner := new(serverTestRunner)
	server, logs := serverTestNew(runner)
	if server.host != "127.0.0.1:9876" || server.runner != runner || server.logger == nil || server.timeout != browserTimeout || server.busy.Load() {
		t.Fatalf("unexpected companion configuration: %#v", server)
	}
	wantHash := sha256.Sum256([]byte("Bearer " + configTestValid().LocalAPIKey))
	if server.keyHash != wantHash {
		t.Error("companion did not hash the complete bearer value")
	}
	if len(runner.calls()) != 0 || logs.String() != "" {
		t.Error("construction invoked browser or logged private configuration")
	}
}

func TestCompanionAuthorization(t *testing.T) {
	key := configTestValid().LocalAPIKey
	tests := []struct {
		name   string
		values []string
		want   bool
	}{
		{"missing", nil, false},
		{"empty list", []string{}, false},
		{"empty", []string{""}, false},
		{"valid", []string{"Bearer " + key}, true},
		{"wrong key", []string{"Bearer " + strings.Repeat("x", len(key))}, false},
		{"short key", []string{"Bearer x"}, false},
		{"long key", []string{"Bearer " + strings.Repeat("x", 1024)}, false},
		{"empty bearer", []string{"Bearer "}, false},
		{"bare key", []string{key}, false},
		{"lowercase scheme", []string{"bearer " + key}, false},
		{"basic", []string{"Basic " + key}, false},
		{"missing space", []string{"Bearer" + key}, false},
		{"extra space", []string{"Bearer  " + key}, false},
		{"leading space", []string{" Bearer " + key}, false},
		{"trailing space", []string{"Bearer " + key + " "}, false},
		{"tab", []string{"Bearer\t" + key}, false},
		{"trailing newline", []string{"Bearer " + key + "\n"}, false},
		{"duplicate valid", []string{"Bearer " + key, "Bearer " + key}, false},
		{"valid then wrong", []string{"Bearer " + key, "Bearer wrong"}, false},
		{"wrong then valid", []string{"Bearer wrong", "Bearer " + key}, false},
		{"comma joined", []string{"Bearer " + key + ", Bearer " + key}, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			runner := new(serverTestRunner)
			server, _ := serverTestNew(runner)
			request := serverTestRequest(http.MethodGet, "/tasks", "")
			request.Header["Authorization"] = tt.values
			if got := server.authorized(request); got != tt.want {
				t.Errorf("authorized = %v, want %v", got, tt.want)
			}
			response := httptest.NewRecorder()
			server.ServeHTTP(response, request)
			if tt.want {
				serverTestResponse(t, response, 200, `{"synthetic":true}`)
				if len(runner.calls()) != 1 {
					t.Error("authorized request was not forwarded once")
				}
			} else {
				serverTestRejection(t, response, 401)
				if len(runner.calls()) != 0 {
					t.Error("unauthorized request reached browser")
				}
			}
		})
	}
}

type serverTestObservedBody struct {
	reads atomic.Int32
	body  io.Reader
}

func (body *serverTestObservedBody) Read(data []byte) (int, error) {
	body.reads.Add(1)
	return body.body.Read(data)
}

func (*serverTestObservedBody) Close() error { return nil }

func TestCompanionAuthenticatesBeforeValidation(t *testing.T) {
	for name, change := range map[string]func(*http.Request, *companion){
		"wrong host":       func(r *http.Request, _ *companion) { r.Host = "evil.example" },
		"origin":           func(r *http.Request, _ *companion) { r.Header.Set("Origin", "https://evil.example") },
		"unknown route":    func(r *http.Request, _ *companion) { r.URL.Path = "/private-unknown" },
		"nil URL":          func(r *http.Request, _ *companion) { r.URL = nil },
		"bad query":        func(r *http.Request, _ *companion) { r.URL.RawQuery = "unknown=private" },
		"upgrade":          func(r *http.Request, _ *companion) { r.Header.Set("Upgrade", "websocket") },
		"expect":           func(r *http.Request, _ *companion) { r.Header.Set("Expect", "100-continue") },
		"bad content type": func(r *http.Request, _ *companion) { r.Header.Set("Content-Type", "text/plain") },
		"encoding":         func(r *http.Request, _ *companion) { r.Header.Set("Content-Encoding", "gzip") },
		"busy":             func(_ *http.Request, s *companion) { s.busy.Store(true) },
		"preflight": func(r *http.Request, _ *companion) {
			r.Method = http.MethodOptions
			r.Header.Set("Origin", "https://evil.example")
		},
	} {
		t.Run(name, func(t *testing.T) {
			runner := new(serverTestRunner)
			server, logs := serverTestNew(runner)
			request := serverTestRequest(http.MethodPost, "/tasks", "")
			request.Header.Set("Authorization", "Bearer synthetic-wrong-secret")
			body := &serverTestObservedBody{body: strings.NewReader(strings.Repeat("x", maxRequestBody+1))}
			request.Body = body
			change(request, server)
			response := httptest.NewRecorder()
			server.ServeHTTP(response, request)
			serverTestRejection(t, response, http.StatusUnauthorized)
			serverTestLogEntry(t, logs, "request_rejected", http.StatusUnauthorized)
			if len(runner.calls()) != 0 || body.reads.Load() != 0 {
				t.Error("unauthenticated request read a body or invoked browser")
			}
		})
	}
}

func TestCompanionHostOriginAndTransportGuards(t *testing.T) {
	for _, path := range []string{"/tasks", "/health"} {
		for _, host := range []string{"", "localhost:9876", "127.0.0.1", "127.0.0.1:9877", "127.0.0.1:09876", "127.0.0.1:9876.", "127.0.0.1:9876 ", "[::1]:9876", "evil.example:9876", "127.0.0.1:9876@evil.example"} {
			t.Run(path+"/host="+host, func(t *testing.T) {
				runner := new(serverTestRunner)
				server, _ := serverTestNew(runner)
				request := serverTestRequest(http.MethodGet, path, "")
				request.Host = host
				request.Header.Set("X-Forwarded-Host", server.host)
				request.Header.Set("Host", server.host)
				response := httptest.NewRecorder()
				server.ServeHTTP(response, request)
				serverTestRejection(t, response, http.StatusForbidden)
				if len(runner.calls()) != 0 {
					t.Error("wrong host reached browser")
				}
			})
		}
		for _, header := range []string{"Origin", "oRiGiN", "Upgrade", "uPgRaDe", "Expect", "eXpEcT"} {
			for _, value := range [][]string{nil, {""}, {"null"}, {"http://127.0.0.1:9876"}, {"https://evil.example"}} {
				t.Run(fmt.Sprintf("%s/%s=%v", path, header, value), func(t *testing.T) {
					runner := new(serverTestRunner)
					server, _ := serverTestNew(runner)
					request := serverTestRequest(http.MethodGet, path, "")
					request.Header[header] = value
					status := http.StatusBadRequest
					if strings.EqualFold(header, "Origin") {
						status = http.StatusForbidden
					}
					response := httptest.NewRecorder()
					server.ServeHTTP(response, request)
					serverTestRejection(t, response, status)
					if len(runner.calls()) != 0 {
						t.Error("forbidden transport reached browser")
					}
				})
			}
		}
	}
}

func TestCompanionHealthIsStaticPublicAndBrowserIndependent(t *testing.T) {
	for _, busy := range []bool{false, true} {
		for _, auth := range []string{"", "Bearer invalid", "Bearer " + configTestValid().LocalAPIKey} {
			t.Run(fmt.Sprintf("busy=%v/auth=%q", busy, auth), func(t *testing.T) {
				server, logs := serverTestNew(nil)
				server.busy.Store(busy)
				request := serverTestRequest(http.MethodGet, "/health", "")
				request.Header.Del("Authorization")
				if auth != "" {
					request.Header.Set("Authorization", auth)
				}
				response := httptest.NewRecorder()
				server.ServeHTTP(response, request)
				serverTestResponse(t, response, http.StatusOK, `{"ready":true}`)
				if server.busy.Load() != busy || logs.String() != "" {
					t.Error("health changed busy state or produced logs")
				}
			})
		}
	}
	for _, method := range []string{http.MethodHead, http.MethodPost, http.MethodOptions, http.MethodDelete} {
		for _, authenticated := range []bool{false, true} {
			t.Run(fmt.Sprintf("%s/auth=%v", method, authenticated), func(t *testing.T) {
				server, _ := serverTestNew(nil)
				request := serverTestRequest(method, "/health", "")
				want := http.StatusNotFound
				if !authenticated {
					request.Header.Del("Authorization")
					want = http.StatusUnauthorized
				}
				response := httptest.NewRecorder()
				server.ServeHTTP(response, request)
				serverTestRejection(t, response, want)
			})
		}
	}
}

func TestCompanionHealthRejectsNoncanonicalRequests(t *testing.T) {
	for name, change := range map[string]func(*http.Request){
		"absolute":       func(r *http.Request) { r.URL.Scheme = "http"; r.URL.Host = r.Host },
		"host":           func(r *http.Request) { r.URL.Host = r.Host },
		"opaque":         func(r *http.Request) { r.URL.Opaque = "//evil.example/health" },
		"raw path":       func(r *http.Request) { r.URL.RawPath = "/%68ealth"; r.RequestURI = "/%68ealth" },
		"query":          func(r *http.Request) { r.URL.RawQuery = "q=secret" },
		"empty query":    func(r *http.Request) { r.URL.ForceQuery = true },
		"fragment":       func(r *http.Request) { r.URL.Fragment = "private" },
		"mismatched URI": func(r *http.Request) { r.RequestURI = "/tasks" },
		"body":           func(r *http.Request) { r.Body = io.NopCloser(strings.NewReader("{}")) },
		"whitespace body": func(r *http.Request) {
			r.Body = io.NopCloser(strings.NewReader(" "))
			r.ContentLength = -1
		},
		"trailer":  func(r *http.Request) { r.Header.Set("Trailer", "X-Private") },
		"encoding": func(r *http.Request) { r.Header.Set("Content-Encoding", "gzip") },
	} {
		t.Run(name, func(t *testing.T) {
			server, _ := serverTestNew(nil)
			request := serverTestRequest(http.MethodGet, "/health", "")
			request.Header.Del("Authorization")
			change(request)
			response := httptest.NewRecorder()
			server.ServeHTTP(response, request)
			serverTestRejection(t, response, http.StatusBadRequest)
		})
	}
}

func TestCompanionForwardsAllSixteenRoutes(t *testing.T) {
	for _, tt := range routeTestCases() {
		t.Run(tt.method+" "+tt.path, func(t *testing.T) {
			runner := new(serverTestRunner)
			server, logs := serverTestNew(runner)
			body, wantBody := "", ""
			switch tt.body {
			case objectBody:
				body, wantBody = " \n{\"private\":\"synthetic-body-secret\"}\t", `{"private":"synthetic-body-secret"}`
			case emptyObjectBody:
				wantBody = "{}"
			}
			request := serverTestRequest(tt.method, tt.path, body)
			request.Header.Set("Cookie", "synthetic-cookie-secret")
			request.Header.Set("X-Private", "synthetic-header-secret")
			response := httptest.NewRecorder()
			server.ServeHTTP(response, request)
			serverTestResponse(t, response, http.StatusOK, `{"synthetic":true}`)
			serverTestLogEntry(t, logs, "upstream_completed", http.StatusOK)
			calls := runner.calls()
			if len(calls) != 1 {
				t.Fatalf("browser calls = %d, want 1", len(calls))
			}
			want := browserRequest{Method: tt.method, Path: tt.path}
			if wantBody != "" {
				want.Body = json.RawMessage(wantBody)
			}
			if !reflect.DeepEqual(calls[0], want) {
				t.Errorf("browser request = %#v, want %#v", calls[0], want)
			}
			if server.busy.Load() {
				t.Error("browser slot was not released")
			}
		})
	}
}

func TestCompanionForwardsValidatedQueryValues(t *testing.T) {
	for _, tt := range []struct{ method, target, want string }{
		{"GET", "/students?filter=status%3aactive&q=a%20b&offset=0&limit=200&sort=name&dir=asc", "/students?dir=asc&filter=status%3Aactive&limit=200&offset=0&q=a+b&sort=name"},
		{"GET", "/schedules?filter=active", "/schedules?filter=active"},
		{"GET", "/occurrences?q=%C3%A9%26x%3D1", "/occurrences?q=%C3%A9%26x%3D1"},
		{"GET", "/tasks?view=templates&status=published", "/tasks?status=published&view=templates"},
		{"POST", "/occurrences/" + routeTestUUID + "/retry?requirementId=" + routeTestUUID + "&attemptId=" + routeTestUUID, "/occurrences/" + routeTestUUID + "/retry?attemptId=" + routeTestUUID + "&requirementId=" + routeTestUUID},
	} {
		t.Run(tt.target, func(t *testing.T) {
			runner := new(serverTestRunner)
			server, _ := serverTestNew(runner)
			response := httptest.NewRecorder()
			server.ServeHTTP(response, serverTestRequest(tt.method, tt.target, ""))
			serverTestResponse(t, response, 200, `{"synthetic":true}`)
			calls := runner.calls()
			if len(calls) != 1 || calls[0].Method != tt.method || calls[0].Path != tt.want || calls[0].Body != nil {
				t.Errorf("browser calls = %#v", calls)
			}
		})
	}
}

func TestCompanionRejectsRequestsWithoutBrowser(t *testing.T) {
	tests := []struct {
		name   string
		method string
		target string
		body   string
		status int
	}{
		{"unknown route", "GET", "/private-unknown", "", 404},
		{"wrong method", "PUT", "/tasks", "{}", 404},
		{"traversal", "GET", "/students/../tasks", "", 404},
		{"encoded traversal", "GET", "/%2e%2e/tasks", "", 400},
		{"encoded route", "GET", "/%74asks", "", 400},
		{"absolute target", "GET", "http://127.0.0.1:9876/tasks", "", 400},
		{"unknown query", "GET", "/tasks?secret_unknown=secret-query", "", 400},
		{"duplicate query", "GET", "/tasks?limit=1&limit=2", "", 400},
		{"invalid limit", "GET", "/tasks?limit=201", "", 400},
		{"invalid offset", "GET", "/students?offset=-1", "", 400},
		{"invalid direction", "GET", "/schedules?dir=up", "", 400},
		{"invalid retry UUID", "POST", "/occurrences/" + routeTestUUID + "/retry?attemptId=invalid", "", 400},
		{"invalid route UUID", "POST", "/tasks/invalid/publish", "", 404},
		{"missing object", "POST", "/tasks", "", 415},
		{"array object", "POST", "/tasks", "[]", 400},
		{"null object", "PATCH", "/schedules/" + routeTestUUID, "null", 400},
		{"trailing object", "POST", "/tasks", "{}{}", 400},
		{"nonempty publish", "POST", "/tasks/" + routeTestUUID + "/publish", `{"secret":"private"}`, 400},
		{"retry body", "POST", "/occurrences/" + routeTestUUID + "/retry", "{}", 400},
		{"delete body", "DELETE", "/schedules/" + routeTestUUID, "{}", 400},
		{"oversized object", "POST", "/tasks", "{}" + strings.Repeat(" ", maxRequestBody-1), 413},
	}
	for _, tt := range routeTestCases() {
		if tt.method == http.MethodGet {
			tests = append(tests, struct {
				name, method, target, body string
				status                     int
			}{"GET body " + tt.path, tt.method, tt.path, "{}", 400})
		}
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			runner := new(serverTestRunner)
			server, logs := serverTestNew(runner)
			request := serverTestRequest(tt.method, tt.target, tt.body)
			request.ContentLength = -1
			response := httptest.NewRecorder()
			server.ServeHTTP(response, request)
			serverTestRejection(t, response, tt.status)
			serverTestLogEntry(t, logs, "request_rejected", tt.status)
			if len(runner.calls()) != 0 || server.busy.Load() {
				t.Error("invalid request invoked browser or retained busy slot")
			}
			followup := httptest.NewRecorder()
			server.ServeHTTP(followup, serverTestRequest(http.MethodGet, "/tasks", ""))
			serverTestResponse(t, followup, 200, `{"synthetic":true}`)
			if len(runner.calls()) != 1 {
				t.Error("valid follow-up was not forwarded once")
			}
		})
	}
}

func TestCompanionAcceptsMaximumWriteBody(t *testing.T) {
	for _, tt := range routeTestCases() {
		if tt.body != objectBody {
			continue
		}
		t.Run(tt.method+" "+tt.path, func(t *testing.T) {
			body := `{"value":"` + strings.Repeat("a", maxRequestBody-len(`{"value":""}`)) + `"}`
			runner := new(serverTestRunner)
			server, _ := serverTestNew(runner)
			request := serverTestRequest(tt.method, tt.path, body)
			request.ContentLength = -1
			request.TransferEncoding = []string{"chunked"}
			response := httptest.NewRecorder()
			server.ServeHTTP(response, request)
			serverTestResponse(t, response, 200, `{"synthetic":true}`)
			calls := runner.calls()
			if len(calls) != 1 || string(calls[0].Body) != body {
				t.Error("maximum-length JSON object was not forwarded exactly once intact")
			}
		})
	}
}

func TestCompanionBrowserStatusPassthrough(t *testing.T) {
	for status := 100; status <= 600; status++ {
		t.Run(fmt.Sprint(status), func(t *testing.T) {
			body := json.RawMessage(`{"private":"synthetic-upstream-secret","nested":[1,true,null]}`)
			runner := &serverTestRunner{run: func(context.Context, browserRequest) (browserResult, error) {
				return browserResult{Status: status, Body: body}, nil
			}}
			server, logs := serverTestNew(runner)
			response := httptest.NewRecorder()
			server.ServeHTTP(response, serverTestRequest(http.MethodGet, "/tasks?q=synthetic-query-secret", ""))
			allowed := status >= 200 && status <= 599 && !(status >= 300 && status <= 399) && status != 205
			if status == http.StatusNoContent {
				if response.Code != http.StatusNoContent || response.Body.Len() != 0 {
					t.Fatal("HTTP 204 must preserve the upstream status with no response body")
				}
				serverTestLogEntry(t, logs, "upstream_completed", status)
			} else if allowed {
				serverTestResponse(t, response, status, string(body))
				serverTestLogEntry(t, logs, "upstream_completed", status)
			} else {
				serverTestResponse(t, response, http.StatusBadGateway, serverTestReadFailure)
				serverTestLogEntry(t, logs, "browser_failed", http.StatusBadGateway)
			}
			if len(runner.calls()) != 1 || server.busy.Load() {
				t.Error("browser was retried or busy slot retained")
			}
		})
	}
}

func TestCompanionBrowserResultValidation(t *testing.T) {
	tests := []struct {
		name   string
		result browserResult
		valid  bool
	}{
		{"object", browserResult{200, json.RawMessage(`{}`)}, true},
		{"array", browserResult{200, json.RawMessage(`[1,true,null]`)}, true},
		{"null", browserResult{200, json.RawMessage(`null`)}, true},
		{"string", browserResult{200, json.RawMessage(`"é🙂"`)}, true},
		{"number", browserResult{200, json.RawMessage(`1.5`)}, true},
		{"boolean", browserResult{200, json.RawMessage(`false`)}, true},
		{"whitespace preserved", browserResult{200, json.RawMessage(" \n{\"x\":1}\t")}, true},
		{"maximum body", browserResult{200, json.RawMessage(`"` + strings.Repeat("a", maxBrowserOutput-2) + `"`)}, true},
		{"oversized body", browserResult{200, json.RawMessage(`"` + strings.Repeat("a", maxBrowserOutput-1) + `"`)}, false},
		{"nil body", browserResult{200, nil}, false},
		{"empty body", browserResult{200, json.RawMessage{}}, false},
		{"whitespace body", browserResult{200, json.RawMessage(" \n")}, false},
		{"invalid JSON", browserResult{200, json.RawMessage(`{"secret":`)}, false},
		{"trailing JSON", browserResult{200, json.RawMessage(`{} {}`)}, false},
		{"invalid utf8", browserResult{200, json.RawMessage("{\"secret\":\"\xff\"}")}, false},
		{"zero status", browserResult{0, json.RawMessage(`{}`)}, false},
		{"negative status", browserResult{-1, json.RawMessage(`{}`)}, false},
		{"huge status", browserResult{999, json.RawMessage(`{}`)}, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			runner := &serverTestRunner{run: func(context.Context, browserRequest) (browserResult, error) { return tt.result, nil }}
			server, _ := serverTestNew(runner)
			response := httptest.NewRecorder()
			server.ServeHTTP(response, serverTestRequest(http.MethodGet, "/tasks", ""))
			if tt.valid {
				serverTestResponse(t, response, tt.result.Status, string(tt.result.Body))
			} else {
				serverTestResponse(t, response, http.StatusBadGateway, serverTestReadFailure)
			}
			if len(runner.calls()) != 1 {
				t.Error("browser result was retried")
			}
		})
	}
}

func TestCompanionRedactsFailuresAndLogs(t *testing.T) {
	for _, method := range []string{http.MethodGet, http.MethodPost} {
		for _, withResult := range []bool{false, true} {
			t.Run(fmt.Sprintf("%s/result=%v", method, withResult), func(t *testing.T) {
				runner := &serverTestRunner{run: func(context.Context, browserRequest) (browserResult, error) {
					result := browserResult{}
					if withResult {
						result = browserResult{200, json.RawMessage(`{"secret":"synthetic-result-secret"}`)}
					}
					return result, errors.New("synthetic-browser-error-secret " + configTestValid().LocalAPIKey)
				}}
				server, logs := serverTestNew(runner)
				target, body, want := "/tasks?q=synthetic-query-secret", "", serverTestReadFailure
				if method == http.MethodPost {
					target, body, want = "/tasks", `{"secret":"synthetic-body-secret"}`, serverTestWriteFailure
				}
				request := serverTestRequest(method, target, body)
				request.Header.Set("Cookie", "synthetic-cookie-secret")
				response := httptest.NewRecorder()
				server.ServeHTTP(response, request)
				serverTestResponse(t, response, http.StatusBadGateway, want)
				serverTestLogEntry(t, logs, "browser_failed", http.StatusBadGateway)
				for _, secret := range []string{configTestValid().LocalAPIKey, configTestValid().ClerkUserID, configTestValid().Email, configTestValid().TenantID, configTestValid().ParentSubject, "synthetic-browser-error-secret", "synthetic-result-secret", "synthetic-body-secret", "synthetic-query-secret", "synthetic-cookie-secret"} {
					if strings.Contains(response.Body.String()+logs.String(), secret) {
						t.Errorf("response or logs leaked %q", secret)
					}
				}
				if len(runner.calls()) != 1 || server.busy.Load() {
					t.Error("failure retried browser or retained busy slot")
				}
			})
		}
	}
	server := newCompanion(configTestValid(), new(serverTestRunner), nil)
	response := httptest.NewRecorder()
	server.ServeHTTP(response, serverTestRequest(http.MethodGet, "/tasks", ""))
	serverTestResponse(t, response, 200, `{"synthetic":true}`)
}

func TestCompanionNeverRetriesWrites(t *testing.T) {
	for _, tt := range routeTestCases() {
		if tt.method == http.MethodGet {
			continue
		}
		for _, outcome := range []string{"runner error", "invalid result", "deadline", "rate limited", "unavailable"} {
			t.Run(tt.method+" "+tt.path+"/"+outcome, func(t *testing.T) {
				runner := &serverTestRunner{run: func(ctx context.Context, _ browserRequest) (browserResult, error) {
					switch outcome {
					case "runner error":
						return browserResult{}, errors.New("synthetic-write-error")
					case "invalid result":
						return browserResult{204, nil}, nil
					case "deadline":
						<-ctx.Done()
						return browserResult{}, ctx.Err()
					case "rate limited":
						return browserResult{429, json.RawMessage(`{"error":"rate limited"}`)}, nil
					default:
						return browserResult{503, json.RawMessage(`{"error":"unavailable"}`)}, nil
					}
				}}
				server, _ := serverTestNew(runner)
				if outcome == "deadline" {
					server.timeout = 5 * time.Millisecond
				}
				body := ""
				if tt.body == objectBody {
					body = `{"synthetic":true}`
				}
				response := httptest.NewRecorder()
				server.ServeHTTP(response, serverTestRequest(tt.method, tt.path, body))
				wantStatus, wantBody := http.StatusBadGateway, serverTestWriteFailure
				switch outcome {
				case "deadline":
					wantStatus = http.StatusGatewayTimeout
				case "rate limited":
					wantStatus, wantBody = 429, `{"error":"rate limited"}`
				case "unavailable":
					wantStatus, wantBody = 503, `{"error":"unavailable"}`
				}
				serverTestResponse(t, response, wantStatus, wantBody)
				if len(runner.calls()) != 1 || server.busy.Load() {
					t.Error("write was retried or busy slot retained")
				}
			})
		}
	}
}

func serverTestServeAsync(server *companion, request *http.Request) <-chan *httptest.ResponseRecorder {
	done := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		response := httptest.NewRecorder()
		server.ServeHTTP(response, request)
		done <- response
	}()
	return done
}

func serverTestWaitResponse(t *testing.T, done <-chan *httptest.ResponseRecorder) *httptest.ResponseRecorder {
	t.Helper()
	select {
	case response := <-done:
		return response
	case <-time.After(3 * time.Second):
		t.Fatal("handler did not return promptly")
		return nil
	}
}

func TestCompanionBusyDoesNotQueue(t *testing.T) {
	entered := make(chan struct{}, 1)
	release := make(chan struct{})
	var count atomic.Int32
	runner := &serverTestRunner{run: func(ctx context.Context, _ browserRequest) (browserResult, error) {
		if count.Add(1) == 1 {
			entered <- struct{}{}
			select {
			case <-release:
			case <-ctx.Done():
				return browserResult{}, ctx.Err()
			}
		}
		return browserResult{200, json.RawMessage(`{"synthetic":true}`)}, nil
	}}
	server, _ := serverTestNew(runner)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	first := serverTestServeAsync(server, serverTestRequest(http.MethodPost, "/tasks", "{}").WithContext(ctx))
	select {
	case <-entered:
	case <-time.After(3 * time.Second):
		t.Fatal("first request did not enter browser")
	}
	if !server.busy.Load() {
		t.Fatal("active browser did not occupy busy slot")
	}
	for i := range 16 {
		request := serverTestRequest(http.MethodPost, "/tasks", "")
		body := &serverTestObservedBody{body: strings.NewReader(`{"queued":true}`)}
		request.Body = body
		response := serverTestWaitResponse(t, serverTestServeAsync(server, request))
		serverTestRejection(t, response, http.StatusServiceUnavailable)
		if body.reads.Load() != 0 || len(runner.calls()) != 1 {
			t.Fatalf("busy request %d read body or entered browser", i)
		}
	}
	health := serverTestRequest(http.MethodGet, "/health", "")
	health.Header.Del("Authorization")
	serverTestResponse(t, serverTestWaitResponse(t, serverTestServeAsync(server, health)), 200, `{"ready":true}`)
	unauthorized := serverTestRequest(http.MethodGet, "/tasks", "")
	unauthorized.Header.Del("Authorization")
	serverTestRejection(t, serverTestWaitResponse(t, serverTestServeAsync(server, unauthorized)), 401)
	serverTestRejection(t, serverTestWaitResponse(t, serverTestServeAsync(server, serverTestRequest(http.MethodGet, "/unknown", ""))), 404)
	close(release)
	serverTestResponse(t, serverTestWaitResponse(t, first), 200, `{"synthetic":true}`)
	if len(runner.calls()) != 1 || server.busy.Load() {
		t.Fatal("rejected requests were queued or busy slot was not released")
	}
	response := httptest.NewRecorder()
	server.ServeHTTP(response, serverTestRequest(http.MethodGet, "/tasks", ""))
	serverTestResponse(t, response, 200, `{"synthetic":true}`)
	if len(runner.calls()) != 2 {
		t.Errorf("browser calls after follow-up = %d, want 2", len(runner.calls()))
	}
}

func TestCompanionBusyRejectsConcurrentBurst(t *testing.T) {
	const total = 32
	start := make(chan struct{})
	entered := make(chan struct{}, total)
	release := make(chan struct{})
	var active atomic.Int32
	var overlap atomic.Bool
	runner := &serverTestRunner{run: func(ctx context.Context, _ browserRequest) (browserResult, error) {
		if active.Add(1) != 1 {
			overlap.Store(true)
		}
		defer active.Add(-1)
		entered <- struct{}{}
		select {
		case <-release:
			return browserResult{200, json.RawMessage(`{"synthetic":true}`)}, nil
		case <-ctx.Done():
			return browserResult{}, ctx.Err()
		}
	}}
	server, _ := serverTestNew(runner)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	responses := make(chan *httptest.ResponseRecorder, total)
	var workers sync.WaitGroup
	defer func() {
		cancel()
		workers.Wait()
	}()
	for range total {
		workers.Add(1)
		go func() {
			defer workers.Done()
			<-start
			response := httptest.NewRecorder()
			server.ServeHTTP(response, serverTestRequest(http.MethodPost, "/tasks", "{}").WithContext(ctx))
			responses <- response
		}()
	}
	close(start)
	select {
	case <-entered:
	case <-time.After(3 * time.Second):
		t.Fatal("no browser request entered")
	}
	for range total - 1 {
		serverTestRejection(t, serverTestWaitResponse(t, responses), http.StatusServiceUnavailable)
	}
	if len(runner.calls()) != 1 || overlap.Load() {
		t.Fatal("concurrent burst invoked more than one browser")
	}
	close(release)
	serverTestResponse(t, serverTestWaitResponse(t, responses), 200, `{"synthetic":true}`)
	workers.Wait()
	if len(runner.calls()) != 1 || server.busy.Load() || active.Load() != 0 {
		t.Error("burst requests were queued or browser slot was retained")
	}
}

func TestCompanionCancellationReleasesBrowserSlot(t *testing.T) {
	for _, method := range []string{http.MethodGet, http.MethodPost} {
		t.Run(method, func(t *testing.T) {
			entered := make(chan context.Context, 1)
			var count atomic.Int32
			runner := &serverTestRunner{run: func(ctx context.Context, _ browserRequest) (browserResult, error) {
				if count.Add(1) == 1 {
					entered <- ctx
					<-ctx.Done()
					return browserResult{200, json.RawMessage(`{"late":"synthetic-secret"}`)}, nil
				}
				return browserResult{200, json.RawMessage(`{"synthetic":true}`)}, nil
			}}
			server, logs := serverTestNew(runner)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			body, want := "", serverTestReadFailure
			if method == http.MethodPost {
				body, want = "{}", serverTestWriteFailure
			}
			done := serverTestServeAsync(server, serverTestRequest(method, "/tasks", body).WithContext(ctx))
			var browserContext context.Context
			select {
			case browserContext = <-entered:
			case <-time.After(3 * time.Second):
				t.Fatal("browser did not receive request")
			}
			cancel()
			serverTestResponse(t, serverTestWaitResponse(t, done), 504, want)
			serverTestLogEntry(t, logs, "browser_failed", 504)
			if browserContext.Err() != context.Canceled || len(runner.calls()) != 1 || server.busy.Load() {
				t.Errorf("cancellation not propagated or browser slot retained: %v", browserContext.Err())
			}
			response := httptest.NewRecorder()
			server.ServeHTTP(response, serverTestRequest(http.MethodGet, "/tasks", ""))
			serverTestResponse(t, response, 200, `{"synthetic":true}`)
			if len(runner.calls()) != 2 {
				t.Error("canceled request was retried or follow-up was lost")
			}
		})
	}
}

func TestCompanionTimeoutCancelsBrowser(t *testing.T) {
	for _, lateSuccess := range []bool{false, true} {
		t.Run(fmt.Sprintf("lateSuccess=%v", lateSuccess), func(t *testing.T) {
			var browserContext context.Context
			runner := &serverTestRunner{run: func(ctx context.Context, _ browserRequest) (browserResult, error) {
				browserContext = ctx
				<-ctx.Done()
				if lateSuccess {
					return browserResult{200, json.RawMessage(`{"late":"synthetic-secret"}`)}, nil
				}
				return browserResult{}, ctx.Err()
			}}
			server, logs := serverTestNew(runner)
			server.timeout = 10 * time.Millisecond
			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			defer cancel()
			done := serverTestServeAsync(server, serverTestRequest(http.MethodGet, "/tasks", "").WithContext(ctx))
			serverTestResponse(t, serverTestWaitResponse(t, done), 504, serverTestReadFailure)
			serverTestLogEntry(t, logs, "browser_failed", 504)
			if browserContext == nil || browserContext.Err() != context.DeadlineExceeded || ctx.Err() != nil {
				t.Fatal("server timeout did not independently expire browser context")
			}
			if len(runner.calls()) != 1 || server.busy.Load() {
				t.Error("timed-out browser was retried or busy slot retained")
			}
		})
	}
}

func TestCompanionAlreadyCanceledRequestContext(t *testing.T) {
	for _, expired := range []bool{false, true} {
		t.Run(fmt.Sprintf("expired=%v", expired), func(t *testing.T) {
			var ctx context.Context
			var cancel context.CancelFunc
			wantErr := context.Canceled
			if expired {
				ctx, cancel = context.WithDeadline(context.Background(), time.Now().Add(-time.Second))
				wantErr = context.DeadlineExceeded
			} else {
				ctx, cancel = context.WithCancel(context.Background())
				cancel()
			}
			defer cancel()
			var observed error
			runner := &serverTestRunner{run: func(ctx context.Context, _ browserRequest) (browserResult, error) {
				observed = ctx.Err()
				return browserResult{200, json.RawMessage(`{"late":"synthetic-secret"}`)}, nil
			}}
			server, logs := serverTestNew(runner)
			response := httptest.NewRecorder()
			server.ServeHTTP(response, serverTestRequest(http.MethodGet, "/tasks", "").WithContext(ctx))
			serverTestResponse(t, response, 504, serverTestReadFailure)
			serverTestLogEntry(t, logs, "browser_failed", 504)
			if len(runner.calls()) > 1 || len(runner.calls()) == 1 && !errors.Is(observed, wantErr) || server.busy.Load() {
				t.Errorf("canceled request was retried, kept busy slot, or lost context error: %v", observed)
			}
		})
	}
}

func TestCompanionDeadlineBoundsAndContextPropagation(t *testing.T) {
	type contextKey struct{}
	for _, configured := range []time.Duration{-time.Second, 0, 2 * time.Second, browserTimeout, browserTimeout + time.Second} {
		for _, parentLimited := range []bool{false, true} {
			t.Run(fmt.Sprintf("timeout=%s/parent=%v", configured, parentLimited), func(t *testing.T) {
				parent := context.WithValue(context.Background(), contextKey{}, "synthetic-context-value")
				if parentLimited {
					var cancel context.CancelFunc
					parent, cancel = context.WithTimeout(parent, time.Second)
					defer cancel()
				}
				var observed context.Context
				runner := &serverTestRunner{run: func(ctx context.Context, _ browserRequest) (browserResult, error) {
					observed = ctx
					return browserResult{200, json.RawMessage(`{}`)}, nil
				}}
				server, _ := serverTestNew(runner)
				server.timeout = configured
				before := time.Now()
				response := httptest.NewRecorder()
				server.ServeHTTP(response, serverTestRequest(http.MethodGet, "/tasks", "").WithContext(parent))
				after := time.Now()
				serverTestResponse(t, response, 200, `{}`)
				if observed == nil {
					t.Fatal("browser did not receive context")
				}
				deadline, ok := observed.Deadline()
				if !ok {
					t.Fatal("browser context has no deadline")
				}
				if parentLimited {
					want, _ := parent.Deadline()
					if !deadline.Equal(want) {
						t.Errorf("browser deadline %v does not respect parent deadline %v", deadline, want)
					}
				} else {
					want := configured
					if want <= 0 || want > browserTimeout {
						want = browserTimeout
					}
					if deadline.Before(before.Add(want)) || deadline.After(after.Add(want)) {
						t.Errorf("deadline %v is outside configured bounds", deadline)
					}
				}
				if observed.Value(contextKey{}) != "synthetic-context-value" || observed.Err() != context.Canceled || parent.Err() != nil {
					t.Error("context values or cancellation lifetime were not preserved")
				}
			})
		}
	}
}
