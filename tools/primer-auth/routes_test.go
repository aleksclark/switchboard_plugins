package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"reflect"
	"strings"
	"testing"
)

const routeTestUUID = "12345678-abcd-4def-8123-123456789abc"

type routeTestCase struct {
	method string
	path   string
	body   bodyPolicy
	query  string
}

func routeTestCases() []routeTestCase {
	return []routeTestCase{
		{http.MethodGet, "/students", noBody, "list"},
		{http.MethodGet, "/tasks", noBody, "tasks"},
		{http.MethodPost, "/tasks", objectBody, ""},
		{http.MethodPost, "/tasks/" + routeTestUUID + "/revisions", objectBody, ""},
		{http.MethodPost, "/tasks/" + routeTestUUID + "/publish", emptyObjectBody, ""},
		{http.MethodPost, "/tasks/" + routeTestUUID + "/retire", emptyObjectBody, ""},
		{http.MethodGet, "/schedules", noBody, "list"},
		{http.MethodPost, "/schedules", objectBody, ""},
		{http.MethodPatch, "/schedules/" + routeTestUUID, objectBody, ""},
		{http.MethodDelete, "/schedules/" + routeTestUUID, noBody, ""},
		{http.MethodGet, "/occurrences", noBody, "list"},
		{http.MethodGet, "/occurrences/" + routeTestUUID, noBody, ""},
		{http.MethodPost, "/occurrences/" + routeTestUUID + "/decision", objectBody, ""},
		{http.MethodPost, "/occurrences/" + routeTestUUID + "/skip", emptyObjectBody, ""},
		{http.MethodPost, "/occurrences/" + routeTestUUID + "/cancel", emptyObjectBody, ""},
		{http.MethodPost, "/occurrences/" + routeTestUUID + "/retry", noBody, "retry"},
	}
}

func TestMatchRouteAllowlist(t *testing.T) {
	cases := routeTestCases()
	if len(cases) != 16 {
		t.Fatalf("route coverage = %d, want 16", len(cases))
	}
	allowed := make(map[string]route)
	paths := make(map[string]bool)
	for _, tt := range cases {
		allowed[tt.method+" "+tt.path] = route{body: tt.body, query: tt.query}
		paths[tt.path] = true
	}
	for path := range paths {
		for _, method := range []string{http.MethodGet, http.MethodHead, http.MethodPost, http.MethodPut, http.MethodPatch, http.MethodDelete, http.MethodOptions, http.MethodConnect, http.MethodTrace, "get", "", "CUSTOM"} {
			t.Run(method+" "+path, func(t *testing.T) {
				want, wantOK := allowed[method+" "+path]
				got, ok := matchRoute(method, path)
				if ok != wantOK || got != want {
					t.Errorf("matchRoute = %#v, %v; want %#v, %v", got, ok, want, wantOK)
				}
			})
		}
	}
	for _, path := range []string{"", "/", "/health", "/unknown", "/Students", "students", "/students/", "//students", "/tasks/" + routeTestUUID, "/students/" + routeTestUUID, "/tasks/" + routeTestUUID + "/unknown", "/tasks/" + routeTestUUID + "/publish/extra", "/tasks//publish", "/tasks/not-a-uuid/publish", "/schedules/" + routeTestUUID + "/publish", "/occurrences/" + routeTestUUID + "/publish"} {
		for _, method := range []string{http.MethodGet, http.MethodPost, http.MethodPatch, http.MethodDelete} {
			if _, ok := matchRoute(method, path); ok {
				t.Errorf("unexpected allowed route %s %q", method, path)
			}
		}
	}
}

func TestValidUUID(t *testing.T) {
	for _, value := range []string{routeTestUUID, strings.ToUpper(routeTestUUID), "00000000-0000-0000-0000-000000000000", "ffffffff-ffff-ffff-ffff-ffffffffffff"} {
		if !validUUID(value) {
			t.Errorf("validUUID(%q) = false", value)
		}
	}
	for _, value := range []string{"", "not-a-uuid", strings.ReplaceAll(routeTestUUID, "-", ""), "{" + routeTestUUID + "}", "urn:uuid:" + routeTestUUID, routeTestUUID + " ", " " + routeTestUUID, routeTestUUID[:35], routeTestUUID + "0", strings.Replace(routeTestUUID, "a", "g", 1), strings.Replace(routeTestUUID, "a", "é", 1), strings.Replace(routeTestUUID, "a", "\x00", 1)} {
		if validUUID(value) {
			t.Errorf("validUUID(%q) = true", value)
		}
	}
	for index := range len(routeTestUUID) {
		value := []byte(routeTestUUID)
		if value[index] == '-' {
			value[index] = 'a'
		} else {
			value[index] = '-'
		}
		if validUUID(string(value)) {
			t.Errorf("accepted invalid UUID at index %d: %q", index, value)
		}
	}
}

func TestValidateTargetAllRoutes(t *testing.T) {
	for _, tt := range routeTestCases() {
		t.Run(tt.method+" "+tt.path, func(t *testing.T) {
			request := httptest.NewRequest(tt.method, tt.path, nil)
			got, path, status := validateTarget(request)
			if status != 0 || got != (route{body: tt.body, query: tt.query}) || path != tt.path {
				t.Fatalf("validateTarget = %#v, %q, %d", got, path, status)
			}
			query := ""
			switch tt.query {
			case "list":
				query = "filter=status%3Aactive&q=hello%20world&limit=20&offset=0&sort=name&dir=asc"
			case "tasks":
				query = "status=published&view=templates&q=hello%20world&limit=20&offset=0&sort=name&dir=desc"
			case "retry":
				query = "requirementId=" + routeTestUUID + "&attemptId=" + strings.ToUpper(routeTestUUID)
			}
			if query != "" {
				request = httptest.NewRequest(tt.method, tt.path+"?"+query, nil)
				_, path, status = validateTarget(request)
				values, err := url.ParseQuery(query)
				if err != nil {
					t.Fatal(err)
				}
				if status != 0 || path != tt.path+"?"+values.Encode() {
					t.Errorf("filtered target = %q, %d", path, status)
				}
			}
			for _, query := range []string{"unknown=value", "limit=1&limit=2"} {
				request = httptest.NewRequest(tt.method, tt.path+"?"+query, nil)
				_, _, status = validateTarget(request)
				if status != http.StatusBadRequest {
					t.Errorf("query %q status = %d, want 400", query, status)
				}
			}
		})
	}
}

func TestValidateTargetRejectsUnsafeTargets(t *testing.T) {
	tests := []struct {
		target string
		status int
	}{
		{"/../tasks", 404},
		{"/students/../tasks", 404},
		{"/./tasks", 404},
		{"//tasks", 404},
		{"/tasks/", 404},
		{"/TASKS", 404},
		{"/tasks/" + routeTestUUID + "/publish/", 404},
		{"/%74asks", 400},
		{"/tasks%2f", 400},
		{"/tasks%2F..%2Fstudents", 400},
		{"/%2e%2e/tasks", 400},
		{"/%252e%252e/tasks", 400},
		{"/tasks%5c", 400},
		{"/tasks%25", 400},
		{"/occurrences/%31" + routeTestUUID[1:], 400},
		{"/tasks\\..\\students", 400},
		{"/tasks?", 400},
		{"/tasks?limit", 400},
		{"/tasks?limit=201", 400},
		{"/tasks?%6cimit=1", 400},
		{"/tasks?limit=1&limit=2", 400},
		{"/tasks?unknown=value", 400},
		{"/tasks?filter=active", 400},
		{"http://127.0.0.1:9876/tasks", 400},
		{"https://evil.example/tasks", 400},
	}
	for _, tt := range tests {
		t.Run(tt.target, func(t *testing.T) {
			request := httptest.NewRequest(http.MethodGet, tt.target, nil)
			got, path, status := validateTarget(request)
			if status != tt.status || path != "" || got != (route{}) {
				t.Errorf("validateTarget = %#v, %q, %d; want empty route and %d", got, path, status, tt.status)
			}
		})
	}
	for name, change := range map[string]func(*http.Request){
		"nil URL":           func(r *http.Request) { r.URL = nil },
		"URL host":          func(r *http.Request) { r.URL.Host = "evil.example" },
		"URL scheme":        func(r *http.Request) { r.URL.Scheme = "https" },
		"opaque":            func(r *http.Request) { r.URL.Opaque = "//evil.example/tasks" },
		"fragment":          func(r *http.Request) { r.URL.Fragment = "secret" },
		"raw path":          func(r *http.Request) { r.URL.RawPath = "/tasks" },
		"percent path":      func(r *http.Request) { r.URL.Path = "/%tasks" },
		"mismatched URI":    func(r *http.Request) { r.RequestURI = "/students" },
		"mismatched query":  func(r *http.Request) { r.RequestURI += "?limit=1" },
		"force empty query": func(r *http.Request) { r.URL.ForceQuery = true; r.RequestURI = "" },
	} {
		t.Run(name, func(t *testing.T) {
			request := httptest.NewRequest(http.MethodGet, "/tasks", nil)
			change(request)
			if _, _, status := validateTarget(request); status != http.StatusBadRequest {
				t.Errorf("status = %d, want 400", status)
			}
		})
	}
}

func TestQueryKeyAllowlist(t *testing.T) {
	allowed := map[string]map[string]bool{
		"list":  {"limit": true, "offset": true, "q": true, "sort": true, "dir": true, "filter": true},
		"tasks": {"limit": true, "offset": true, "q": true, "sort": true, "dir": true, "status": true, "view": true},
		"retry": {"requirementId": true, "attemptId": true},
		"":      {},
		"other": {},
	}
	for kind, keys := range allowed {
		for _, key := range []string{"limit", "offset", "q", "sort", "dir", "filter", "status", "view", "requirementId", "attemptId", "Limit", "unknown", "", "tenant_id", "url"} {
			if got := queryKeyAllowed(kind, key); got != keys[key] {
				t.Errorf("queryKeyAllowed(%q, %q) = %v, want %v", kind, key, got, keys[key])
			}
		}
	}
}

func TestValidateQueryCanonicalizationAndDefaultOmission(t *testing.T) {
	tests := []struct {
		name string
		kind string
		raw  string
		want string
	}{
		{"list no defaults", "list", "", ""},
		{"tasks no defaults", "tasks", "", ""},
		{"retry no defaults", "retry", "", ""},
		{"no query", "", "", ""},
		{"only supplied limit", "list", "limit=1", "limit=1"},
		{"maximum pagination", "list", "offset=2147483647&limit=200", "limit=200&offset=2147483647"},
		{"zero offset", "tasks", "offset=0", "offset=0"},
		{"leading zeroes", "list", "limit=001&offset=000", "limit=001&offset=000"},
		{"filter", "list", "filter=status%3aactive%20AND%20name%3aAlice", "filter=status%3Aactive+AND+name%3AAlice"},
		{"tasks filters", "tasks", "view=templates&status=published", "status=published&view=templates"},
		{"ascending sort", "list", "sort=name&dir=asc", "dir=asc&sort=name"},
		{"descending sort", "tasks", "sort=created_at&dir=desc", "dir=desc&sort=created_at"},
		{"encoded value", "list", "q=a%26b%3Dc%2B%20%25%2f%3F%23", "q=a%26b%3Dc%2B+%25%2F%3F%23"},
		{"unicode", "list", "q=%C3%A9%F0%9F%99%82", "q=%C3%A9%F0%9F%99%82"},
		{"empty text", "list", "sort=&q=&filter=", "filter=&q=&sort="},
		{"empty status", "tasks", "status=", "status="},
		{"retry requirement", "retry", "requirementId=" + routeTestUUID, "requirementId=" + routeTestUUID},
		{"retry attempt", "retry", "attemptId=" + routeTestUUID, "attemptId=" + routeTestUUID},
		{"retry both", "retry", "requirementId=" + routeTestUUID + "&attemptId=" + strings.ToUpper(routeTestUUID), "attemptId=" + strings.ToUpper(routeTestUUID) + "&requirementId=" + routeTestUUID},
		{"text maximum", "list", "q=" + strings.Repeat("a", 2048), "q=" + strings.Repeat("a", 2048)},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := validateQuery(tt.raw, tt.kind)
			if err != nil || got != tt.want {
				t.Errorf("validateQuery = %q, %v; want %q", got, err, tt.want)
			}
		})
	}
	values := url.Values{"q": {strings.Repeat(" ", 1364)}, "filter": {strings.Repeat(" ", 1363) + "a"}}
	raw := "q=" + strings.Repeat("%20", 1364) + "&filter=" + strings.Repeat("%20", 1363) + "a"
	if len(raw) != 8192 {
		t.Fatalf("boundary fixture size = %d", len(raw))
	}
	if got, err := validateQuery(raw, "list"); err != nil || got != values.Encode() {
		t.Errorf("8192-byte query = %q, %v", got, err)
	}
	if _, err := validateQuery(raw+"a", "list"); err == nil {
		t.Error("accepted query over 8192 bytes")
	}
}

func TestValidateQueryRejectsInvalidInput(t *testing.T) {
	tests := []struct{ kind, raw string }{
		{"list", "unknown=secret"}, {"", "limit=1"}, {"retry", "q=secret"},
		{"tasks", "filter=active"}, {"list", "status=active"}, {"list", "view=templates"},
		{"list", "Limit=1"}, {"list", "limit=1&limit=1"}, {"list", "q=a&q=b"},
		{"list", "limit"}, {"list", "=value"}, {"list", "&limit=1"}, {"list", "limit=1&"}, {"list", "limit=1&&offset=0"},
		{"list", "%6cimit=1"}, {"list", "q=ok&%71=again"}, {"list", "q=%"}, {"list", "q=%GG"},
		{"list", "q=a;b"}, {"list", "q=%00"}, {"list", "q=%0A"}, {"list", "q=%0D"}, {"list", "q=%09"}, {"list", "q=%7F"}, {"list", "q=%C2%85"}, {"list", "q=%FF"},
		{"list", "q=" + strings.Repeat("a", 2049)}, {"list", "q=" + strings.Repeat("é", 1025)},
		{"list", "limit="}, {"list", "limit=0"}, {"list", "limit=201"}, {"list", "limit=-1"}, {"list", "limit=%2B1"}, {"list", "limit=+1"}, {"list", "limit=1.0"}, {"list", "limit=1e2"}, {"list", "limit=0x10"}, {"list", "limit=١"}, {"list", "limit=2147483648"},
		{"list", "offset="}, {"list", "offset=-1"}, {"list", "offset=1.5"}, {"list", "offset=%2B1"}, {"list", "offset=2147483648"}, {"list", "offset=9999999999999999999999999"},
		{"list", "dir="}, {"list", "dir=ASC"}, {"list", "dir=DESC"}, {"list", "dir=ascending"}, {"list", "dir=asc+"},
		{"tasks", "view="}, {"tasks", "view=template"}, {"tasks", "view=Templates"},
		{"retry", "requirementId="}, {"retry", "attemptId="}, {"retry", "requirementId=not-a-uuid"}, {"retry", "attemptId=" + strings.ReplaceAll(routeTestUUID, "-", "")},
		{"retry", "requirementId=" + routeTestUUID + "&requirementId=" + routeTestUUID},
		{"retry", "attemptId=" + routeTestUUID + "&attemptId=" + routeTestUUID},
		{"retry", "requirementId=" + routeTestUUID + "&attemptId=invalid"},
	}
	for _, tt := range tests {
		name := tt.kind + "/" + tt.raw
		if len(name) > 100 {
			name = name[:100]
		}
		t.Run(name, func(t *testing.T) {
			got, err := validateQuery(tt.raw, tt.kind)
			if err == nil || got != "" {
				t.Fatalf("validateQuery = %q, %v; want empty query and error", got, err)
			}
			if err.Error() != "invalid query" {
				t.Errorf("query error is not redacted: %v", err)
			}
		})
	}
}

type routeTestErrorReader struct{}

func (routeTestErrorReader) Read([]byte) (int, error) {
	return 0, errors.New("synthetic-private-read-error")
}

func TestReadBodyPolicies(t *testing.T) {
	tests := []struct {
		name   string
		policy bodyPolicy
		body   string
		typeOf string
		want   string
		status int
	}{
		{"object", objectBody, `{"name":"é🙂","nested":{"x":1},"array":[true,null]}`, "application/json", `{"name":"é🙂","nested":{"x":1},"array":[true,null]}`, 0},
		{"trimmed object", objectBody, " \r\n {\"x\": 1} \t", "application/json", `{"x": 1}`, 0},
		{"empty object", objectBody, "{}", "application/json", "{}", 0},
		{"utf8 charset", objectBody, "{}", "application/json; charset=utf-8", "{}", 0},
		{"uppercase charset", objectBody, "{}", `application/json; charset="UTF-8"`, "{}", 0},
		{"uppercase media type", objectBody, "{}", "Application/JSON", "{}", 0},
		{"missing type", objectBody, "{}", "", "", 415},
		{"wrong type", objectBody, "{}", "text/plain", "", 415},
		{"suffix type", objectBody, "{}", "application/problem+json", "", 415},
		{"trailing media type semicolon", objectBody, "{}", "application/json;", "{}", 0},
		{"malformed type", objectBody, "{}", "application/json; charset", "", 415},
		{"multiple media types", objectBody, "{}", "application/json, application/json", "", 415},
		{"wrong charset", objectBody, "{}", "application/json; charset=latin1", "", 415},
		{"unknown parameter", objectBody, "{}", "application/json; boundary=secret", "", 415},
		{"empty required", objectBody, "", "application/json", "", 400},
		{"whitespace required", objectBody, " \t\r\n", "application/json", "", 400},
		{"array", objectBody, "[]", "application/json", "", 400},
		{"null", objectBody, "null", "application/json", "", 400},
		{"string", objectBody, `"secret"`, "application/json", "", 400},
		{"number", objectBody, "1", "application/json", "", 400},
		{"boolean", objectBody, "true", "application/json", "", 400},
		{"malformed", objectBody, `{"x":`, "application/json", "", 400},
		{"trailing object", objectBody, "{}{}", "application/json", "", 400},
		{"trailing scalar", objectBody, "{} null", "application/json", "", 400},
		{"trailing garbage", objectBody, "{} secret", "application/json", "", 400},
		{"invalid utf8", objectBody, "{\"x\":\"\xff\"}", "application/json", "", 400},
		{"bom", objectBody, "\ufeff{}", "application/json", "", 400},
		{"optional omitted", emptyObjectBody, "", "", "{}", 0},
		{"optional empty", emptyObjectBody, "{}", "application/json", "{}", 0},
		{"optional spaced", emptyObjectBody, " \n{ }\t", "application/json", "{ }", 0},
		{"optional missing type", emptyObjectBody, "{}", "", "", 415},
		{"optional whitespace", emptyObjectBody, " ", "application/json", "", 400},
		{"optional nonempty", emptyObjectBody, `{"x":null}`, "application/json", "", 400},
		{"optional array", emptyObjectBody, "[]", "application/json", "", 400},
		{"optional null", emptyObjectBody, "null", "application/json", "", 400},
		{"no body", noBody, "", "", "", 0},
		{"no body with type", noBody, "", "application/json", "", 0},
		{"forbidden object", noBody, "{}", "application/json", "", 400},
		{"forbidden whitespace", noBody, " ", "", "", 400},
		{"forbidden null byte", noBody, "\x00", "", "", 400},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			request := httptest.NewRequest(http.MethodPost, "/tasks", strings.NewReader(tt.body))
			if tt.typeOf != "" {
				request.Header.Set("Content-Type", tt.typeOf)
			}
			body, status := readBody(httptest.NewRecorder(), request, tt.policy)
			if status != tt.status || string(body) != tt.want {
				t.Errorf("readBody = %q, %d; want %q, %d", body, status, tt.want, tt.status)
			}
			if tt.policy == noBody && body != nil {
				t.Error("no-body policy returned a body")
			}
		})
	}
}

func TestReadBodySizeLimit(t *testing.T) {
	if maxRequestBody != 256*1024 {
		t.Fatalf("maxRequestBody = %d, want 256 KiB", maxRequestBody)
	}
	for _, policy := range []bodyPolicy{objectBody, emptyObjectBody, noBody} {
		for _, size := range []int{maxRequestBody - 1, maxRequestBody, maxRequestBody + 1} {
			for _, unknownLength := range []bool{false, true} {
				t.Run(fmt.Sprintf("policy=%d/size=%d/chunked=%v", policy, size, unknownLength), func(t *testing.T) {
					data := "{}" + strings.Repeat(" ", size-2)
					request := httptest.NewRequest(http.MethodPost, "/tasks", strings.NewReader(data))
					request.Header.Set("Content-Type", "application/json")
					if unknownLength {
						request.ContentLength = -1
						request.TransferEncoding = []string{"chunked"}
					}
					wantStatus := 0
					if policy == noBody {
						wantStatus = http.StatusBadRequest
					} else if size > maxRequestBody {
						wantStatus = http.StatusRequestEntityTooLarge
					}
					body, status := readBody(httptest.NewRecorder(), request, policy)
					if status != wantStatus {
						t.Fatalf("status = %d, want %d", status, wantStatus)
					}
					if status == 0 && string(body) != "{}" || status != 0 && body != nil {
						t.Errorf("unexpected body length %d", len(body))
					}
				})
			}
		}
	}
	data := `{"value":"` + strings.Repeat("a", maxRequestBody-len(`{"value":""}`)) + `"}`
	request := httptest.NewRequest(http.MethodPost, "/tasks", strings.NewReader(data))
	request.Header.Set("Content-Type", "application/json")
	body, status := readBody(httptest.NewRecorder(), request, objectBody)
	if status != 0 || string(body) != data || len(body) != maxRequestBody {
		t.Errorf("maximum object: status=%d length=%d", status, len(body))
	}
}

func TestReadBodyTransportValidation(t *testing.T) {
	for _, policy := range []bodyPolicy{noBody, objectBody, emptyObjectBody} {
		for name, change := range map[string]func(*http.Request){
			"declared trailer":      func(r *http.Request) { r.Header.Set("Trailer", "X-Secret") },
			"empty trailer header":  func(r *http.Request) { r.Header["tRaIlEr"] = nil },
			"trailer map":           func(r *http.Request) { r.Trailer = http.Header{"X-Secret": nil} },
			"content encoding":      func(r *http.Request) { r.Header.Set("Content-Encoding", "gzip") },
			"identity encoding":     func(r *http.Request) { r.Header.Set("Content-Encoding", "identity") },
			"empty encoding header": func(r *http.Request) { r.Header["cOnTeNt-EnCoDiNg"] = nil },
			"reader error":          func(r *http.Request) { r.Body = io.NopCloser(routeTestErrorReader{}) },
		} {
			t.Run(fmt.Sprintf("%d/%s", policy, name), func(t *testing.T) {
				data := "{}"
				if policy == noBody {
					data = ""
				}
				request := httptest.NewRequest(http.MethodPost, "/tasks", strings.NewReader(data))
				request.Header.Set("Content-Type", "application/json")
				change(request)
				if body, status := readBody(httptest.NewRecorder(), request, policy); status != http.StatusBadRequest || body != nil {
					t.Errorf("readBody = %q, %d; want nil, 400", body, status)
				}
			})
		}
	}
	for _, policy := range []bodyPolicy{objectBody, emptyObjectBody} {
		request := httptest.NewRequest(http.MethodPost, "/tasks", strings.NewReader("{}"))
		request.Header["Content-Type"] = []string{"application/json", "application/json"}
		if _, status := readBody(httptest.NewRecorder(), request, policy); status != http.StatusUnsupportedMediaType {
			t.Errorf("duplicate Content-Type status = %d", status)
		}
	}
	for _, policy := range []bodyPolicy{noBody, emptyObjectBody} {
		request := httptest.NewRequest(http.MethodPost, "/tasks", nil)
		request.Body = nil
		body, status := readBody(httptest.NewRecorder(), request, policy)
		if status != 0 || policy == noBody && body != nil || policy == emptyObjectBody && string(body) != "{}" {
			t.Errorf("nil body, policy %d = %q, %d", policy, body, status)
		}
	}
}

func TestBrowserRequestBodyOmission(t *testing.T) {
	for _, body := range []json.RawMessage{nil, json.RawMessage(`{}`), json.RawMessage(`{"x":1}`)} {
		request := browserRequest{Method: http.MethodGet, Path: "/tasks", Body: body}
		data, err := json.Marshal(request)
		if err != nil {
			t.Fatal(err)
		}
		var got map[string]json.RawMessage
		if err := json.Unmarshal(data, &got); err != nil {
			t.Fatal(err)
		}
		want := map[string]json.RawMessage{"method": json.RawMessage(`"GET"`), "path": json.RawMessage(`"/tasks"`)}
		if body != nil {
			want["body"] = body
		}
		if !reflect.DeepEqual(got, want) {
			t.Errorf("serialized request = %s, want %#v", data, want)
		}
	}
}
