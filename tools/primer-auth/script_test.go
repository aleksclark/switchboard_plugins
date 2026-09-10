package main

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os/exec"
	"reflect"
	"strings"
	"testing"
	"time"
)

const (
	scriptTestToken        = "synthetic-script-test-bearer-secret"
	scriptTestException    = "synthetic-private-exception-detail"
	scriptTestUnavailable  = "Browser session or upstream request unavailable."
	scriptTestUnknownWrite = "Browser request failed; write outcome unknown. Check state before any manual retry."
	scriptTestBusy         = "Browser operation still in progress."
)

func scriptTestConfig() config {
	return config{
		ClerkUserID:    "user_expected",
		Email:          "parent@example.test",
		TenantID:       "tenant_expected",
		ParentSubject:  "parent_expected",
		LocalAPIKey:    "local-key-must-not-appear-in-generated-code",
		BrowserCommand: "/never/execute/a/real/browser",
		Port:           9876,
	}
}

func scriptTestRequest(method string) browserRequest {
	request := browserRequest{Method: method, Path: "/tasks?limit=2&q=hello%20world"}
	if method == "POST" || method == "PATCH" {
		request.Body = json.RawMessage(`{"title":"Test é🙂","enabled":true,"nested":{"value":null}}`)
	}
	return request
}

const scriptTestExpiry int64 = 1700000026000
const scriptTestRequestID = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

func scriptTestGenerate(t *testing.T, cfg config, request browserRequest) (string, string) {
	t.Helper()
	script, err := browserScriptPayload(cfg, request, scriptTestRequestID, scriptTestExpiry)
	if err != nil {
		t.Fatalf("browserScriptPayload: %v", err)
	}
	return script, script
}

func scriptTestJSONEqual(t *testing.T, got, want any) {
	t.Helper()
	normalize := func(value any) any {
		data, err := json.Marshal(value)
		if err != nil {
			t.Fatalf("marshal comparison value: %v", err)
		}
		var decoded any
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("decode comparison value: %v", err)
		}
		return decoded
	}
	if actual, expected := normalize(got), normalize(want); !reflect.DeepEqual(actual, expected) {
		t.Errorf("JSON mismatch\ngot:  %v\nwant: %v", actual, expected)
	}
}

func TestBrowserScriptPayload(t *testing.T) {
	for _, body := range []json.RawMessage{
		nil,
		json.RawMessage(`{}`),
		json.RawMessage(`null`),
		json.RawMessage(" \n{\"integer\":9007199254740993,\"fraction\":1.2300,\"exponent\":1E+09,\"zero\":-0}\t "),
		json.RawMessage(`{"text":"\"'\\\n\r\t</script> é🙂\u2028\u2029 __PAYLOAD__","number":123,"nested":[true,null]}`),
	} {
		t.Run(fmt.Sprintf("body_%s", body), func(t *testing.T) {
			cfg := scriptTestConfig()
			cfg.ClerkUserID += "\"'\\\n\u2028\u2029🙂"
			cfg.Email += "\n'); throw new Error('injected'); //"
			cfg.TenantID += "</script>\t"
			cfg.ParentSubject += "__PAYLOAD__"
			request := browserRequest{Method: "POST", Path: "/tasks?q=%22&label=é🙂", Body: body}
			script, expression := scriptTestGenerate(t, cfg, request)
			_, literal, ok := strings.Cut(expression, "const input = JSON.parse(")
			if !ok {
				t.Fatal("generated expression has no JSON payload")
			}
			var payload string
			decoder := json.NewDecoder(strings.NewReader(literal))
			if err := decoder.Decode(&payload); err != nil {
				t.Fatalf("decode payload literal: %v", err)
			}
			if !strings.HasPrefix(literal[decoder.InputOffset():], ");") {
				t.Fatal("payload escapes its JSON.parse call")
			}
			var decoded map[string]any
			if err := json.Unmarshal([]byte(payload), &decoded); err != nil {
				t.Fatalf("decode payload: %v", err)
			}
			want := map[string]any{
				"userId": cfg.ClerkUserID, "email": cfg.Email, "tenant": cfg.TenantID,
				"subject": cfg.ParentSubject, "request": request, "requestID": scriptTestRequestID, "expiresAt": scriptTestExpiry,
			}
			if len(body) > 0 {
				want["bodyText"] = string(body)
				if got, ok := decoded["bodyText"].(string); !ok || got != string(body) {
					t.Errorf("bodyText=%q, want exact request bytes %q", got, body)
				}
			} else if _, present := decoded["bodyText"]; present {
				t.Error("bodyText must be omitted when there is no body")
			}
			scriptTestJSONEqual(t, decoded, want)
			for _, private := range []string{cfg.LocalAPIKey, cfg.BrowserCommand} {
				if strings.Contains(script, private) {
					t.Error("generated script contains local-only configuration")
				}
			}
		})
	}
}

func TestBrowserScriptRejectsInvalidJSONBody(t *testing.T) {
	for _, body := range []string{`{`, `{"unterminated":`, `{} {}`, `NaN`, `undefined`} {
		t.Run(body, func(t *testing.T) {
			script, err := browserScript(scriptTestConfig(), browserRequest{Method: "POST", Path: "/tasks", Body: json.RawMessage(body)}, time.Now().Add(browserTimeout))
			if err == nil || script != "" {
				t.Errorf("invalid JSON generated a script: error=%v, script length=%d", err, len(script))
			}
		})
	}
}

func scriptTestRuntime(t *testing.T, name string) string {
	t.Helper()
	path, err := exec.LookPath(name)
	if err != nil {
		t.Skipf("optional %s runtime unavailable: %v", name, err)
	}
	return path
}

func scriptTestRun(t *testing.T, runtime, flag, harness string, input, output any) {
	t.Helper()
	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("marshal harness input: %v", err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	command := exec.CommandContext(ctx, runtime, flag, harness)
	command.Stdin = bytes.NewReader(data)
	var stdout, stderr bytes.Buffer
	command.Stdout, command.Stderr = &stdout, &stderr
	if err := command.Run(); err != nil {
		t.Fatalf("offline runtime harness failed: %v (context: %v)\nstdout: %.2000s\nstderr: %.2000s", err, ctx.Err(), stdout.String(), stderr.String())
	}
	if stderr.Len() != 0 {
		t.Fatalf("unexpected runtime stderr: %.2000s", stderr.String())
	}
	if err := json.Unmarshal(stdout.Bytes(), output); err != nil {
		t.Fatalf("decode harness output: %v\nstdout: %.2000s", err, stdout.String())
	}
}

const scriptTestNodeHarness = `const fs = require('node:fs');
const vm = require('node:vm');
const cases = JSON.parse(fs.readFileSync(0, 'utf8'));
const secret = 'synthetic-private-exception-detail synthetic-script-test-bearer-secret';

async function run(c) {
    const s = c.settings || {};
    const calls = [];
    const streams = [];
    const timeouts = [];
    const signals = [];
    const timers = [];
    let clearedTimers = 0;
    let tokenCalls = 0;
    let releaseToken;
    const tokenGate = new Promise(resolve => { releaseToken = resolve; });
    let mutations = 0;
    let token = 'synthetic-script-test-bearer-secret';
    if (s.tokenKind === 'empty') token = '';
    if (s.tokenKind === 'null') token = null;
    if (s.tokenKind === 'number') token = 42;
    if (s.tokenKind === 'object') token = {};
    if (s.tokenKind === 'oversize') token = 'x'.repeat(65537);
    if (s.tokenKind === 'maximum') token = 'x'.repeat(65536);
    if (s.tokenKind === 'minimum') token = 'Z';
    const location = {origin: 'https://api.primerlms.com', pathname: '/tasks/dashboard'};
    const clerk = {
        loaded: true,
        user: {id: 'user_expected', primaryEmailAddress: {emailAddress: 'parent@example.test'}},
        session: {id: 'session_expected', status: 'active', user: {id: 'user_expected'}, async getToken() {
            tokenCalls++;
            checkLock();
            mutate('afterToken');
            if (s.overlap) await tokenGate;
            if (s.tokenKind === 'reject') throw new Error(secret);
            if (s.tokenKind === 'timeout') return new Promise(() => {});
            return token;
        }},
    };
    const lockKey = Symbol.for('primer-auth.in-flight');
    const foreignOwner = {};
    const lock = {acquired: 0, deleted: 0, missingDuringOperation: 0};
    const window = new Proxy({Clerk: clerk, ...(s.busy ? {[lockKey]: foreignOwner} : {})}, {
        set(target, key, value) {
            if (key === lockKey) lock.acquired++;
            return Reflect.set(target, key, value);
        },
        deleteProperty(target, key) {
            if (key === lockKey) lock.deleted++;
            return Reflect.deleteProperty(target, key);
        },
    });
    function checkLock() {
        if (!Object.hasOwn(window, lockKey) || !window[lockKey]) lock.missingDuringOperation++;
    }
    let now = 1700000000000 + (s.initialDelay || 0);
    const replayKey = Symbol.for('primer-auth.requests');
    if (s.seedReplay) {
        const seen = new Map();
        for (let i = 0; i < s.seedReplay; i++) seen.set('seed-' + i, {state: 'completed', retainUntil: now + (s.seedExpired ? 0 : 60000)});
        window[replayKey] = seen;
    }
    const document = {baseURI: s.baseURI || 'https://api.primerlms.com/tasks/dashboard'};
    function mutate(phase) {
        now += s.advanceClock?.[phase] || 0;
        if (s.phase !== phase) return;
        mutations++;
        switch (s.mutation) {
            case 'noClerk': window.Clerk = undefined; break;
            case 'notLoaded': clerk.loaded = false; break;
            case 'truthyLoaded': clerk.loaded = 'true'; break;
            case 'noSession': clerk.session = null; break;
            case 'noUser': clerk.user = null; break;
            case 'noSessionStatus': delete clerk.session.status; break;
            case 'pendingSession': clerk.session.status = 'pending'; break;
            case 'expiredSession': clerk.session.status = 'expired'; break;
            case 'revokedSession': clerk.session.status = 'revoked'; break;
            case 'endedSession': clerk.session.status = 'ended'; break;
            case 'sessionStatusCase': clerk.session.status = 'Active'; break;
            case 'noSessionUser': clerk.session.user = null; break;
            case 'noSessionUserId': clerk.session.user = {}; break;
            case 'wrongSessionUser': clerk.session.user.id = 'user_someone_else'; break;
            case 'numericSessionUserId': clerk.session.user.id = 42; break;
            case 'replacedLock': window[lockKey] = foreignOwner; break;
            case 'hostileBase': document.baseURI = 'https://attacker.example/redirect/'; break;
            case 'wrongId': clerk.user.id = 'user_someone_else'; break;
            case 'wrongEmail': clerk.user.primaryEmailAddress.emailAddress = 'other@example.test'; break;
            case 'emailCase': clerk.user.primaryEmailAddress.emailAddress = 'Parent@example.test'; break;
            case 'noEmail': clerk.user.primaryEmailAddress = null; break;
            case 'emptySessionId': clerk.session.id = ''; break;
            case 'numericSessionId': clerk.session.id = 42; break;
            case 'changedSessionId': clerk.session.id = 'session_changed'; break;
            case 'replacedSession': clerk.session = {...clerk.session}; break;
            case 'replacedClerk': window.Clerk = {...clerk}; break;
            case 'wrongOrigin': location.origin = 'https://api.primerlms.com.evil.test'; break;
            case 'httpOrigin': location.origin = 'http://api.primerlms.com'; break;
            case 'portOrigin': location.origin = 'https://api.primerlms.com:8443'; break;
            case 'wrongPath': location.pathname = '/tasks-other/'; break;
            case 'missingSlash': location.pathname = '/tasks'; break;
            case 'rootPath': location.pathname = '/'; break;
            default: throw new Error('unknown harness mutation: ' + s.mutation);
        }
    }
    function response(stage, description, fallback) {
        const d = description || {};
        const stats = {stage, acquired: 0, reads: 0, cancels: 0, releases: 0};
        streams.push(stats);
        let text = Object.hasOwn(d, 'raw') ? d.raw : JSON.stringify(Object.hasOwn(d, 'body') ? d.body : fallback);
        if (d.expandNumbers) text = '[' + Array(d.expandNumbers).fill('1e9').join(',') + ']';
        if (d.unicodeCharacters) text = JSON.stringify({padding: 'é'.repeat(d.unicodeCharacters)});
        let bytes = Object.hasOwn(d, 'bytes') ? Uint8Array.from(d.bytes) : new TextEncoder().encode(text);
        if (d.byteLength) {
            const padded = new Uint8Array(d.byteLength).fill(32);
            padded.set(bytes);
            bytes = padded;
        }
        const headers = {'content-type': 'application/json', ...d.headers};
        const body = d.noBody ? null : {getReader() {
            stats.acquired++;
            let offset = 0;
            return {
                async read() {
                    stats.reads++;
                    if (d.readError) throw new Error(secret);
                    if (offset === bytes.length) {
                        mutate(stage === 'verification' ? 'afterVerification' : 'afterResponse');
                        return {done: true};
                    }
                    const end = Math.min(bytes.length, offset + (d.chunkSize || 4096));
                    const value = bytes.slice(offset, end);
                    offset = end;
                    return {done: false, value};
                },
                async cancel() {
                    stats.cancels++;
                    if (d.cancelError) throw new Error(secret);
                },
                releaseLock() { stats.releases++; },
            };
        }};
        return {
            status: Object.hasOwn(d, 'status') ? d.status : 200,
            headers: {get(name) {return Object.hasOwn(headers, name.toLowerCase()) ? headers[name.toLowerCase()] : null;}},
            body,
        };
    }
    const sandbox = {
        window, location, document, TextEncoder, TextDecoder,
        Date: class extends Date {
            static now() {
                const value = now;
                now += s.clockTick || 0;
                return value;
            }
        },
        AbortSignal: {timeout(ms) {
            timeouts.push(ms);
            const signal = AbortSignal.timeout(ms);
            signals.push(signal);
            return signal;
        }},
        setTimeout(fn, ms) {
            timers.push(ms);
            return setTimeout(fn, s.tokenKind === 'timeout' ? 0 : ms);
        },
        clearTimeout(timer) {clearedTimers++; clearTimeout(timer);},
        async fetch(url, options) {
            checkLock();
            const stage = calls.length % 2 === 0 ? 'verification' : 'request';
            const resolvedURL = new URL(url, document.baseURI);
            calls.push({
                url, resolvedURL: resolvedURL.href, method: options.method, headers: options.headers,
                credentials: options.credentials, redirect: options.redirect, mode: options.mode, cache: options.cache,
                hasBody: Object.hasOwn(options, 'body'), body: options.body,
                isAbortSignal: options.signal instanceof AbortSignal,
                signalIndex: signals.indexOf(options.signal),
            });
            if (options.mode === 'same-origin' && resolvedURL.origin !== location.origin) throw new TypeError('cross-origin fetch blocked');
            if (calls.length > (s.overlap ? 4 : 2)) throw new Error('unexpected extra fetch');
            if (s.abortAt === stage) throw new DOMException(secret, 'AbortError');
            if (s.rejectAt === stage) throw new Error(secret);
            return stage === 'verification'
                ? response(stage, s.verification, {tenantId: 'tenant_expected', subjectRef: 'parent_expected'})
                : response(stage, s.response, {items: [{id: 'task-1', label: 'é🙂'}]});
        },
    };
    mutate('initial');
    const first = vm.runInNewContext(c.script, sandbox, {timeout: 1000});
    let overlap;
    if (s.overlap) {
        const owner = window[lockKey];
        const busyResult = await vm.runInNewContext(c.script.replace('0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'), sandbox, {timeout: 1000});
        overlap = {busyResult, ownerPreserved: window[lockKey] === owner && !!owner,
            tokensWhileBusy: tokenCalls, fetchesWhileBusy: calls.length};
        releaseToken();
    }
    const result = await first;
    if (s.overlap) {
        overlap.releasedAfterFirst = !Object.hasOwn(window, lockKey);
        overlap.replayResult = await vm.runInNewContext(c.script, sandbox, {timeout: 1000});
    }
    lock.held = Object.hasOwn(window, lockKey);
    lock.foreignOwnerPreserved = window[lockKey] === foreignOwner;
    return {name: c.name, result, calls, streams, tokenCalls, timeouts, timers, clearedTimers, mutations, lock, overlap, replayEntries: window[replayKey] ? Array.from(window[replayKey]) : []};
}

(async () => {
    const results = [];
    for (const c of cases) results.push(await run(c));
    process.stdout.write(JSON.stringify(results));
})().catch(error => {process.stderr.write(String(error.stack)); process.exitCode = 1;});
`

type scriptTestNodeCase struct {
	name          string
	method        string
	requestBody   json.RawMessage
	settings      map[string]any
	fetches       int
	tokens        int
	status        int
	body          any
	unknownWrite  bool
	canceledStage string
}

type scriptTestNodeOutput struct {
	Name          string          `json:"name"`
	ReplayEntries json.RawMessage `json:"replayEntries"`
	Result        json.RawMessage `json:"result"`
	Calls         []struct {
		URL           string            `json:"url"`
		ResolvedURL   string            `json:"resolvedURL"`
		Method        string            `json:"method"`
		Headers       map[string]string `json:"headers"`
		Credentials   string            `json:"credentials"`
		Redirect      string            `json:"redirect"`
		Mode          string            `json:"mode"`
		Cache         string            `json:"cache"`
		HasBody       bool              `json:"hasBody"`
		Body          string            `json:"body"`
		IsAbortSignal bool              `json:"isAbortSignal"`
		SignalIndex   int               `json:"signalIndex"`
	} `json:"calls"`
	Streams []struct {
		Stage    string `json:"stage"`
		Acquired int    `json:"acquired"`
		Reads    int    `json:"reads"`
		Cancels  int    `json:"cancels"`
		Releases int    `json:"releases"`
	} `json:"streams"`
	TokenCalls    int   `json:"tokenCalls"`
	Timeouts      []int `json:"timeouts"`
	Timers        []int `json:"timers"`
	ClearedTimers int   `json:"clearedTimers"`
	Mutations     int   `json:"mutations"`
	Lock          struct {
		Acquired               int  `json:"acquired"`
		Deleted                int  `json:"deleted"`
		MissingDuringOperation int  `json:"missingDuringOperation"`
		Held                   bool `json:"held"`
		ForeignOwnerPreserved  bool `json:"foreignOwnerPreserved"`
	} `json:"lock"`
	Overlap struct {
		BusyResult         json.RawMessage `json:"busyResult"`
		ReplayResult       json.RawMessage `json:"replayResult"`
		OwnerPreserved     bool            `json:"ownerPreserved"`
		ReleasedAfterFirst bool            `json:"releasedAfterFirst"`
		TokensWhileBusy    int             `json:"tokensWhileBusy"`
		FetchesWhileBusy   int             `json:"fetchesWhileBusy"`
	} `json:"overlap"`
}

func scriptTestNodeCases(t *testing.T, cases []scriptTestNodeCase) {
	t.Helper()
	node := scriptTestRuntime(t, "node")
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if err := exec.CommandContext(ctx, node, "-e", "process.exit(typeof AbortSignal === 'function' && typeof AbortSignal.timeout === 'function' && typeof TextDecoder === 'function' && typeof Object.hasOwn === 'function' && typeof DOMException === 'function' ? 0 : 1)").Run(); err != nil {
		t.Skipf("optional Node runtime lacks required browser primitives: %v", err)
	}
	inputs := make([]map[string]any, len(cases))
	for i, tt := range cases {
		method := tt.method
		if method == "" {
			method = "GET"
		}
		request := scriptTestRequest(method)
		if tt.requestBody != nil {
			request.Body = tt.requestBody
		}
		_, expression := scriptTestGenerate(t, scriptTestConfig(), request)
		inputs[i] = map[string]any{"name": tt.name, "script": expression, "settings": tt.settings}
	}
	var outputs []scriptTestNodeOutput
	scriptTestRun(t, node, "-e", scriptTestNodeHarness, inputs, &outputs)
	if len(outputs) != len(cases) {
		t.Fatalf("Node returned %d cases, want %d", len(outputs), len(cases))
	}
	for i, tt := range cases {
		t.Run(tt.name, func(t *testing.T) {
			output := outputs[i]
			if output.Name != tt.name {
				t.Fatalf("received wrong case %q", output.Name)
			}
			status, body := tt.status, tt.body
			if status == 0 {
				status = 502
				message := scriptTestUnavailable
				if tt.unknownWrite {
					message = scriptTestUnknownWrite
				}
				body = map[string]any{"error": message}
			}
			scriptTestJSONEqual(t, output.Result, map[string]any{"status": status, "body": body})
			if strings.Contains(string(output.Result), scriptTestToken) || strings.Contains(string(output.Result), scriptTestException) {
				t.Error("result leaked bearer token or private exception")
			}
			if output.TokenCalls != tt.tokens {
				t.Errorf("getToken calls=%d, want %d", output.TokenCalls, tt.tokens)
			}
			if len(output.Calls) != tt.fetches {
				t.Errorf("fetch calls=%d, want %d; no retries or unverified requests allowed", len(output.Calls), tt.fetches)
			}
			if len(output.Timeouts) != len(output.Calls) {
				t.Error("each fetch must receive its own timeout signal")
			}
			for _, timeout := range output.Timeouts {
				if timeout != 12000 {
					t.Errorf("fetch timeout=%d, want 12000ms", timeout)
				}
			}
			if len(output.Timers) != tt.tokens || output.ClearedTimers != tt.tokens {
				t.Error("token deadline was not installed and cleared exactly once per acquisition")
			}
			for _, timeout := range output.Timers {
				if timeout != 2000 {
					t.Errorf("token timeout=%d, want 2000ms", timeout)
				}
			}
			wantMutations := 0
			if tt.settings["phase"] != nil {
				wantMutations = 1
			}
			if output.Mutations != wantMutations {
				t.Errorf("harness mutations=%d, want %d", output.Mutations, wantMutations)
			}
			method := tt.method
			if method == "" {
				method = "GET"
			}
			request := scriptTestRequest(method)
			if tt.requestBody != nil {
				request.Body = tt.requestBody
			}
			token := scriptTestToken
			if tt.settings["tokenKind"] == "maximum" {
				token = strings.Repeat("x", 65536)
			}
			if tt.settings["tokenKind"] == "minimum" {
				token = "Z"
			}
			for j, call := range output.Calls {
				wantMethod, wantURL := "GET", "https://api.primerlms.com/tasks/api/auth/session"
				wantHeaders := map[string]string{"Authorization": "Bearer " + token, "Accept": "application/json"}
				wantBody := j == 1 && len(request.Body) != 0
				if j == 1 {
					wantMethod, wantURL = method, "https://api.primerlms.com/tasks/api"+request.Path
				}
				if wantBody {
					wantHeaders["Content-Type"] = "application/json"
				}
				if call.ResolvedURL != wantURL {
					t.Errorf("fetch %d resolved against document base to %q, want %q", j, call.ResolvedURL, wantURL)
				}
				if call.URL != wantURL || call.Method != wantMethod {
					t.Errorf("fetch %d: %s %s, want %s %s", j, call.Method, call.URL, wantMethod, wantURL)
				}
				if !reflect.DeepEqual(call.Headers, wantHeaders) {
					t.Errorf("fetch %d has incorrect headers or uses a different bearer token", j)
				}
				if call.Credentials != "omit" || call.Redirect != "error" || call.Mode != "same-origin" || call.Cache != "no-store" || !call.IsAbortSignal {
					t.Errorf("fetch %d lacks safe credentials, redirect, same-origin mode, cache, or AbortSignal options", j)
				}
				if call.SignalIndex != j {
					t.Errorf("fetch %d did not use its own timeout signal", j)
				}
				if call.HasBody != wantBody {
					t.Errorf("fetch %d body presence=%v, want %v", j, call.HasBody, wantBody)
				}
				if wantBody && call.Body != string(request.Body) {
					t.Errorf("fetch %d changed request body bytes\ngot:  %q\nwant: %q", j, call.Body, request.Body)
				}
			}
			foreignLock := tt.settings["busy"] == true || tt.settings["mutation"] == "replacedLock"
			if output.Lock.Held != foreignLock || output.Lock.ForeignOwnerPreserved != foreignLock {
				t.Error("operation leaked its lock or removed another owner's lock")
			}
			if output.Lock.MissingDuringOperation != 0 {
				t.Error("token acquisition or fetch ran without the shared window lock")
			}
			if foreignLock {
				if output.Lock.Deleted != 0 {
					t.Error("operation attempted to delete another owner's lock")
				}
			} else if output.Lock.Acquired != output.Lock.Deleted {
				t.Error("operation did not delete its acquired lock exactly once")
			}
			if tt.tokens > 0 && output.Lock.Acquired == 0 {
				t.Error("operation did not acquire the window lock before getting a token")
			}
			if tt.settings["busy"] == true && output.Lock.Acquired != 0 {
				t.Error("busy operation overwrote the current lock owner")
			}
			canceled := false
			for _, stream := range output.Streams {
				if stream.Releases != stream.Acquired {
					t.Errorf("%s reader lock was not released", stream.Stage)
				}
				wantCancels := 0
				if stream.Stage == tt.canceledStage {
					wantCancels, canceled = 1, true
				}
				if stream.Cancels != wantCancels {
					t.Errorf("%s stream cancels=%d, want %d", stream.Stage, stream.Cancels, wantCancels)
				}
			}
			if tt.canceledStage != "" && !canceled {
				t.Error("expected canceled stream was never created")
			}
		})
	}
}

func scriptTestSuccessBody() any {
	return map[string]any{"items": []any{map[string]any{"id": "task-1", "label": "é🙂"}}}
}

func TestBrowserJavaScriptFetchContract(t *testing.T) {
	var cases []scriptTestNodeCase
	for _, method := range []string{"GET", "POST", "PATCH", "DELETE"} {
		cases = append(cases, scriptTestNodeCase{name: method, method: method, fetches: 2, tokens: 1, status: 200, body: scriptTestSuccessBody()})
	}
	cases = append(cases, scriptTestNodeCase{name: "empty_204", method: "DELETE", settings: map[string]any{"response": map[string]any{"status": 204, "noBody": true, "headers": map[string]any{"content-type": nil}}}, fetches: 2, tokens: 1, status: 204, body: map[string]any{"status": "success"}})
	for _, status := range []int{201, 400, 401, 403, 404, 409, 422, 429, 500, 599} {
		body := map[string]any{"message": "upstream JSON"}
		cases = append(cases, scriptTestNodeCase{name: fmt.Sprintf("status_%d", status), settings: map[string]any{"response": map[string]any{"status": status, "body": body}}, fetches: 2, tokens: 1, status: status, body: body})
	}
	for name, body := range map[string]any{"null": nil, "array": []any{1, "value", true}, "string": "é🙂", "number": 42, "boolean": false} {
		cases = append(cases, scriptTestNodeCase{name: "JSON_" + name, settings: map[string]any{"response": map[string]any{"body": body, "chunkSize": 1}}, fetches: 2, tokens: 1, status: 200, body: body})
	}
	for _, tokenKind := range []string{"minimum", "maximum"} {
		cases = append(cases, scriptTestNodeCase{name: "token_" + tokenKind, settings: map[string]any{"tokenKind": tokenKind}, fetches: 2, tokens: 1, status: 200, body: scriptTestSuccessBody()})
	}
	scriptTestNodeCases(t, cases)
}

func TestBrowserJavaScriptSessionGuards(t *testing.T) {
	var cases []scriptTestNodeCase
	for _, phase := range []string{"initial", "afterToken", "afterVerification", "afterResponse"} {
		for _, mutation := range []string{"noSessionStatus", "pendingSession", "expiredSession", "revokedSession", "endedSession", "sessionStatusCase", "noSessionUser", "noSessionUserId", "wrongSessionUser", "numericSessionUserId"} {
			for _, method := range []string{"GET", "POST", "PATCH", "DELETE"} {
				tokens := 1
				if phase == "initial" {
					tokens = 0
				}
				fetches := map[string]int{"initial": 0, "afterToken": 0, "afterVerification": 1, "afterResponse": 2}[phase]
				cases = append(cases, scriptTestNodeCase{
					name: phase + "/" + mutation + "/" + method, method: method,
					settings: map[string]any{"phase": phase, "mutation": mutation}, tokens: tokens, fetches: fetches,
					unknownWrite: phase == "afterResponse" && method != "GET",
				})
			}
		}
	}
	scriptTestNodeCases(t, cases)
}

func TestBrowserJavaScriptIdentityGuards(t *testing.T) {
	var cases []scriptTestNodeCase
	for _, mutation := range []string{"noClerk", "notLoaded", "truthyLoaded", "noSession", "noUser", "wrongId", "wrongEmail", "emailCase", "noEmail", "emptySessionId", "numericSessionId", "wrongOrigin", "httpOrigin", "portOrigin", "wrongPath", "missingSlash", "rootPath"} {
		cases = append(cases, scriptTestNodeCase{name: "initial/" + mutation, settings: map[string]any{"phase": "initial", "mutation": mutation}})
	}
	for _, phase := range []string{"afterToken", "afterVerification", "afterResponse"} {
		for _, mutation := range []string{"replacedSession", "changedSessionId", "replacedClerk", "noSession", "noUser", "wrongId", "wrongEmail", "notLoaded", "wrongOrigin", "wrongPath"} {
			for _, method := range []string{"GET", "POST"} {
				fetches := map[string]int{"afterToken": 0, "afterVerification": 1, "afterResponse": 2}[phase]
				cases = append(cases, scriptTestNodeCase{name: phase + "/" + mutation + "/" + method, method: method, settings: map[string]any{"phase": phase, "mutation": mutation}, tokens: 1, fetches: fetches, unknownWrite: phase == "afterResponse" && method != "GET"})
			}
		}
	}
	for name, body := range map[string]any{
		"wrong_tenant":      map[string]any{"tenantId": "tenant_other", "subjectRef": "parent_expected"},
		"wrong_subject":     map[string]any{"tenantId": "tenant_expected", "subjectRef": "parent_other"},
		"missing_tenant":    map[string]any{"subjectRef": "parent_expected"},
		"missing_subject":   map[string]any{"tenantId": "tenant_expected"},
		"null":              nil,
		"array":             []any{map[string]any{"tenantId": "tenant_expected", "subjectRef": "parent_expected"}},
		"string":            "tenant_expected",
		"number":            1,
		"wrong_field_types": map[string]any{"tenantId": 123, "subjectRef": true},
	} {
		cases = append(cases, scriptTestNodeCase{name: "verification/" + name, method: "POST", settings: map[string]any{"verification": map[string]any{"body": body}}, tokens: 1, fetches: 1})
	}
	scriptTestNodeCases(t, cases)
}

func TestBrowserJavaScriptFailuresAndRedaction(t *testing.T) {
	var cases []scriptTestNodeCase
	for _, kind := range []string{"empty", "null", "number", "object", "oversize", "reject", "timeout"} {
		cases = append(cases, scriptTestNodeCase{name: "token_" + kind, method: "POST", settings: map[string]any{"tokenKind": kind}, tokens: 1})
	}
	for _, method := range []string{"GET", "POST", "PATCH", "DELETE"} {
		for _, stage := range []string{"verification", "request"} {
			for _, failure := range []string{"abortAt", "rejectAt"} {
				fetches := 1
				if stage == "request" {
					fetches = 2
				}
				cases = append(cases, scriptTestNodeCase{name: method + "/" + stage + "/" + failure, method: method, settings: map[string]any{failure: stage}, tokens: 1, fetches: fetches, unknownWrite: stage == "request" && method != "GET"})
			}
		}
	}
	for _, status := range []int{201, 204, 301, 401, 403, 500} {
		cases = append(cases, scriptTestNodeCase{name: fmt.Sprintf("verification_status_%d", status), method: "POST", settings: map[string]any{"verification": map[string]any{"status": status}}, tokens: 1, fetches: 1})
	}
	for _, status := range []int{0, 199, 300, 302, 307, 399, 600} {
		cases = append(cases, scriptTestNodeCase{name: fmt.Sprintf("request_status_%d", status), method: "POST", settings: map[string]any{"response": map[string]any{"status": status}}, tokens: 1, fetches: 2, unknownWrite: true})
	}
	for name, body := range map[string]any{
		"string":       scriptTestToken,
		"nested_value": map[string]any{"data": []any{map[string]any{"echo": "Bearer " + scriptTestToken}}},
		"object_key":   map[string]any{scriptTestToken: "hidden in a key"},
	} {
		for _, method := range []string{"GET", "POST"} {
			cases = append(cases, scriptTestNodeCase{name: "token_echo/" + name + "/" + method, method: method, settings: map[string]any{"response": map[string]any{"body": body}}, tokens: 1, fetches: 2, unknownWrite: method != "GET"})
		}
	}
	cases = append(cases, scriptTestNodeCase{name: "token_echo_escaped_in_error_response", settings: map[string]any{"response": map[string]any{"status": 403, "raw": `{"error":"\u0073ynthetic-script-test-bearer-secret"}`}}, tokens: 1, fetches: 2})
	scriptTestNodeCases(t, cases)
}

func TestBrowserJavaScriptExactRequestBody(t *testing.T) {
	bodies := map[string]string{
		"unsafe_integer":          `{"id":9007199254740993,"negative":-9007199254740993,"uint64":18446744073709551615}`,
		"numeric_spelling":        " \r\n{ \"decimal\" : 1.2300, \"exponent\":1E+09, \"zero\":-0, \"small\":1e-400 }\t\n",
		"nonfinite_in_javascript": `{"positive":1e400,"negative":-1e400}`,
		"escapes_and_unicode":     `{"text":"é🙂</script>\u2028\u2029\\\"","nested":[9007199254740993,1.234567890123456789]}`,
		"null":                    " \nnull\t",
	}
	var cases []scriptTestNodeCase
	for _, method := range []string{"POST", "PATCH"} {
		for name, body := range bodies {
			cases = append(cases, scriptTestNodeCase{
				name: method + "/" + name, method: method, requestBody: json.RawMessage(body),
				tokens: 1, fetches: 2, status: 200, body: scriptTestSuccessBody(),
			})
		}
	}
	scriptTestNodeCases(t, cases)
}

func TestBrowserJavaScriptAbsoluteSameOriginURLs(t *testing.T) {
	var cases []scriptTestNodeCase
	for _, method := range []string{"GET", "POST", "PATCH", "DELETE"} {
		for _, base := range []string{"https://attacker.example/redirect/", "https://api.primerlms.com.evil.test/", "https://api.primerlms.com:8443/", "https://api.primerlms.com/unrelated/"} {
			cases = append(cases, scriptTestNodeCase{
				name: method + "/" + base, method: method, settings: map[string]any{"baseURI": base},
				tokens: 1, fetches: 2, status: 200, body: scriptTestSuccessBody(),
			})
		}
		cases = append(cases, scriptTestNodeCase{
			name: method + "/base_changed_before_write", method: method,
			settings: map[string]any{"phase": "afterVerification", "mutation": "hostileBase"},
			tokens:   1, fetches: 2, status: 200, body: scriptTestSuccessBody(),
		})
	}
	scriptTestNodeCases(t, cases)
}

func TestBrowserJavaScriptOwnerLock(t *testing.T) {
	var cases []scriptTestNodeCase
	for _, method := range []string{"GET", "POST", "PATCH", "DELETE"} {
		cases = append(cases, scriptTestNodeCase{
			name: method + "/busy", method: method, settings: map[string]any{"busy": true},
			status: 503, body: map[string]any{"error": scriptTestBusy},
		})
	}
	cases = append(cases,
		scriptTestNodeCase{name: "replacement_on_success", settings: map[string]any{"phase": "afterResponse", "mutation": "replacedLock"}, tokens: 1, fetches: 2, status: 200, body: scriptTestSuccessBody()},
		scriptTestNodeCase{name: "replacement_on_token_failure", settings: map[string]any{"phase": "afterToken", "mutation": "replacedLock", "tokenKind": "reject"}, tokens: 1},
	)
	scriptTestNodeCases(t, cases)
}

func TestBrowserJavaScriptConcurrentOperation(t *testing.T) {
	node := scriptTestRuntime(t, "node")
	_, expression := scriptTestGenerate(t, scriptTestConfig(), scriptTestRequest("POST"))
	var outputs []scriptTestNodeOutput
	scriptTestRun(t, node, "-e", scriptTestNodeHarness, []map[string]any{{
		"name": "overlap", "script": expression, "settings": map[string]any{"overlap": true},
	}}, &outputs)
	if len(outputs) != 1 {
		t.Fatalf("got %d outputs, want 1", len(outputs))
	}
	output := outputs[0]
	success := map[string]any{"status": 200, "body": scriptTestSuccessBody()}
	scriptTestJSONEqual(t, output.Result, success)
	scriptTestJSONEqual(t, output.Overlap.BusyResult, map[string]any{"status": 503, "body": map[string]any{"error": scriptTestBusy}})
	scriptTestJSONEqual(t, output.Overlap.ReplayResult, map[string]any{"status": 409, "body": map[string]any{"error": "Browser request already used."}})
	if !output.Overlap.OwnerPreserved || output.Overlap.TokensWhileBusy != 1 || output.Overlap.FetchesWhileBusy != 0 {
		t.Error("overlapping invocation changed the owner or attempted token acquisition/fetch")
	}
	if !output.Overlap.ReleasedAfterFirst || output.Lock.Held || output.Lock.Acquired != 1 || output.Lock.Deleted != 1 || output.Lock.MissingDuringOperation != 0 {
		t.Errorf("owner lock not held and cleaned up exactly once: %+v", output.Lock)
	}
	if output.TokenCalls != 1 || len(output.Calls) != 2 || output.ClearedTimers != 1 {
		t.Error("replay after success must not acquire another token or submit another mutation")
	}
	for i, call := range output.Calls {
		wantMethod := "GET"
		if i%2 == 1 {
			wantMethod = "POST"
		}
		if call.Method != wantMethod {
			t.Errorf("fetch %d method=%s, want %s (verify before each write)", i, call.Method, wantMethod)
		}
	}
}

func TestBrowserJavaScriptAbsoluteDeadline(t *testing.T) {
	var cases []scriptTestNodeCase
	for _, method := range []string{"GET", "POST", "PATCH", "DELETE"} {
		cases = append(cases, scriptTestNodeCase{
			name: method + "/expired_before_start", method: method, settings: map[string]any{"initialDelay": 26000},
		})
		for _, phase := range []string{"afterToken", "afterVerification", "afterResponse"} {
			for _, elapsed := range []int{25999, 26000, 26001} {
				tt := scriptTestNodeCase{
					name: fmt.Sprintf("%s/%s/%dms", method, phase, elapsed), method: method,
					settings: map[string]any{"advanceClock": map[string]int{phase: elapsed}}, tokens: 1,
				}
				if elapsed < 26000 {
					tt.fetches, tt.status, tt.body = 2, 200, scriptTestSuccessBody()
				} else {
					tt.fetches = map[string]int{"afterToken": 0, "afterVerification": 1, "afterResponse": 2}[phase]
					tt.unknownWrite = phase == "afterResponse" && method != "GET"
				}
				cases = append(cases, tt)
			}
		}
		cases = append(cases,
			scriptTestNodeCase{name: method + "/cumulative_before_write", method: method, settings: map[string]any{"advanceClock": map[string]int{"afterToken": 1500, "afterVerification": 24500}}, tokens: 1, fetches: 1},
			scriptTestNodeCase{name: method + "/cumulative_after_response", method: method, settings: map[string]any{"advanceClock": map[string]int{"afterToken": 2000, "afterVerification": 12000, "afterResponse": 12000}}, tokens: 1, fetches: 2, unknownWrite: method != "GET"},
		)
	}
	scriptTestNodeCases(t, cases)
}

func TestBrowserJavaScriptResponseNumbers(t *testing.T) {
	unsafe := map[string]string{
		"positive_infinity": "1e400", "negative_infinity": "-1e400",
		"first_unsafe_integer": "9007199254740992", "rounded_integer": "9007199254740993",
		"negative_unsafe_integer": "-9007199254740992", "negative_rounded_integer": "-9007199254740993",
		"uint64": "18446744073709551615", "large_finite_integer": "1e100",
		"unsafe_exponent": "9.007199254740992e15", "unsafe_decimal": "9007199254740992.0",
	}
	safe := map[string]string{
		"maximum_safe": "9007199254740991", "minimum_safe": "-9007199254740991",
		"fraction": "1.25", "tiny_fraction": "1e-100", "negative_zero": "-0",
		"numeric_strings": `["9007199254740993","1e400"]`,
	}
	var cases []scriptTestNodeCase
	for _, stage := range []string{"verification", "response"} {
		for _, method := range []string{"GET", "POST"} {
			for name, number := range unsafe {
				for _, nested := range []bool{false, true} {
					raw := number
					if nested {
						raw = `{"items":[{"number":` + number + `}]}`
					}
					fetches := 2
					if stage == "verification" {
						raw = `{"tenantId":"tenant_expected","subjectRef":"parent_expected","extra":` + raw + `}`
						fetches = 1
					}
					cases = append(cases, scriptTestNodeCase{
						name: fmt.Sprintf("%s/%s/%s/nested_%t", stage, method, name, nested), method: method,
						settings: map[string]any{stage: map[string]any{"raw": raw, "chunkSize": 1}},
						tokens:   1, fetches: fetches, unknownWrite: stage == "response" && method != "GET",
					})
				}
			}
			for name, raw := range safe {
				body := any(json.RawMessage(raw))
				if stage == "verification" {
					raw = `{"tenantId":"tenant_expected","subjectRef":"parent_expected","extra":` + raw + `}`
					body = scriptTestSuccessBody()
				}
				cases = append(cases, scriptTestNodeCase{
					name: stage + "/" + method + "/safe/" + name, method: method,
					settings: map[string]any{stage: map[string]any{"raw": raw, "chunkSize": 1}},
					tokens:   1, fetches: 2, status: 200, body: body,
				})
			}
		}
	}
	scriptTestNodeCases(t, cases)
}

func TestBrowserJavaScriptResponseValidation(t *testing.T) {
	type responseCase struct {
		name        string
		description map[string]any
		cancel      bool
	}
	tests := []responseCase{
		{"html", map[string]any{"headers": map[string]any{"content-type": "text/html"}, "raw": "<html>" + scriptTestException + "</html>"}, false},
		{"missing_content_type", map[string]any{"headers": map[string]any{"content-type": nil}}, false},
		{"json_prefix_not_json", map[string]any{"headers": map[string]any{"content-type": "application/jsonp"}}, false},
		{"text_json", map[string]any{"headers": map[string]any{"content-type": "text/json"}}, false},
		{"invalid_json", map[string]any{"raw": "{" + scriptTestException}, false},
		{"trailing_json", map[string]any{"raw": "{} {}"}, false},
		{"empty_json", map[string]any{"raw": ""}, false},
		{"no_body", map[string]any{"noBody": true}, false},
		{"negative_length", map[string]any{"headers": map[string]any{"content-length": "-1"}}, false},
		{"fractional_length", map[string]any{"headers": map[string]any{"content-length": "1.5"}}, false},
		{"nonnumeric_length", map[string]any{"headers": map[string]any{"content-length": "unknown"}}, false},
		{"empty_length", map[string]any{"headers": map[string]any{"content-length": ""}}, false},
		{"invalid_utf8", map[string]any{"bytes": []int{34, 255, 34}}, true},
		{"truncated_utf8", map[string]any{"bytes": []int{34, 226, 130}}, false},
		{"read_exception", map[string]any{"readError": true}, true},
		{"cancel_exception_redacted", map[string]any{"readError": true, "cancelError": true}, true},
	}
	var cases []scriptTestNodeCase
	for _, stage := range []string{"verification", "response"} {
		maximum, fetches, streamStage := 65536, 1, "verification"
		if stage == "response" {
			maximum, fetches, streamStage = 1048576, 2, "request"
		}
		stageTests := append([]responseCase{}, tests...)
		stageTests = append(stageTests,
			responseCase{"declared_oversize", map[string]any{"headers": map[string]any{"content-length": fmt.Sprint(maximum + 1)}}, false},
			responseCase{"stream_oversize", map[string]any{"byteLength": maximum + 1}, true},
			responseCase{"unicode_byte_oversize", map[string]any{"unicodeCharacters": maximum/2 + 1}, true},
			responseCase{"lying_content_length", map[string]any{"byteLength": maximum + 1, "headers": map[string]any{"content-length": "2"}}, true},
		)
		for _, tt := range stageTests {
			for _, method := range []string{"GET", "POST"} {
				canceled := ""
				if tt.cancel {
					canceled = streamStage
				}
				cases = append(cases, scriptTestNodeCase{name: stage + "/" + tt.name + "/" + method, method: method, settings: map[string]any{stage: tt.description}, tokens: 1, fetches: fetches, unknownWrite: stage == "response" && method != "GET", canceledStage: canceled})
			}
		}
		cases = append(cases, scriptTestNodeCase{name: stage + "/exact_size_limit", settings: map[string]any{stage: map[string]any{"byteLength": maximum, "headers": map[string]any{"content-length": fmt.Sprint(maximum)}}}, tokens: 1, fetches: 2, status: 200, body: scriptTestSuccessBody()})
		for _, contentType := range []string{"application/json; charset=utf-8", " Application/JSON ; charset=UTF-8", "application/problem+json", "application/vnd.primer+json"} {
			cases = append(cases, scriptTestNodeCase{name: stage + "/content_type/" + contentType, settings: map[string]any{stage: map[string]any{"headers": map[string]any{"content-type": contentType}, "chunkSize": 1}}, tokens: 1, fetches: 2, status: 200, body: scriptTestSuccessBody()})
		}
	}
	cases = append(cases, scriptTestNodeCase{name: "serialized_envelope_oversize", method: "POST", settings: map[string]any{"response": map[string]any{"expandNumbers": 150000}}, tokens: 1, fetches: 2, unknownWrite: true})
	scriptTestNodeCases(t, cases)
}

func TestBrowserScriptFreshRequestIDAndExpiry(t *testing.T) {
	expiry := time.Now().Add(20 * time.Second)
	seen := make(map[string]bool)
	for range 20 {
		script, err := browserScript(scriptTestConfig(), scriptTestRequest("POST"), expiry)
		if err != nil {
			t.Fatal(err)
		}
		fields := testScriptPayload(t, script)
		var id string
		var gotExpiry int64
		if json.Unmarshal(fields["requestID"], &id) != nil {
			t.Fatal("missing request ID")
		}
		decoded, err := hex.DecodeString(id)
		if err != nil || len(decoded) != 32 || seen[id] {
			t.Fatal("request ID not a fresh 256-bit value")
		}
		seen[id] = true
		if json.Unmarshal(fields["expiresAt"], &gotExpiry) != nil || gotExpiry != expiry.UnixMilli() {
			t.Fatal("Go expiry not preserved")
		}
	}
}

func TestBrowserJavaScriptReplayRetention(t *testing.T) {
	node := scriptTestRuntime(t, "node")
	_, expression := scriptTestGenerate(t, scriptTestConfig(), scriptTestRequest("POST"))
	inputs := []map[string]any{
		{"name": "full", "script": expression, "settings": map[string]any{"seedReplay": 1024}},
		{"name": "prune", "script": expression, "settings": map[string]any{"seedReplay": 1024, "seedExpired": true}},
		{"name": "inflight_and_completed", "script": expression, "settings": map[string]any{"overlap": true}},
	}
	var outputs []scriptTestNodeOutput
	scriptTestRun(t, node, "-e", scriptTestNodeHarness, inputs, &outputs)
	if len(outputs) != len(inputs) {
		t.Fatal("missing replay test outputs")
	}
	for i, output := range outputs {
		var entries [][]json.RawMessage
		if json.Unmarshal(output.ReplayEntries, &entries) != nil {
			t.Fatal("missing replay map")
		}
		if i == 0 {
			scriptTestJSONEqual(t, output.Result, map[string]any{"status": 503, "body": map[string]any{"error": scriptTestBusy}})
			if len(entries) != 1024 || output.TokenCalls != 0 || len(output.Calls) != 0 {
				t.Fatal("capacity evicted unexpired IDs or executed work")
			}
			continue
		}
		wantEntries := i
		if len(entries) != wantEntries || output.TokenCalls != 1 || len(output.Calls) != 2 {
			t.Fatal("replay pruning/completed guard did not execute exactly one verified mutation")
		}
		for _, entry := range entries {
			if len(entry) != 2 {
				t.Fatal("malformed replay entry")
			}
			var id string
			if json.Unmarshal(entry[0], &id) != nil || len(id) != 64 {
				t.Fatal("unexpected replay ID")
			}
			scriptTestJSONEqual(t, entry[1], map[string]any{"state": "completed", "retainUntil": int64(1700000060000)})
		}
		if strings.Contains(string(output.ReplayEntries), scriptTestToken) {
			t.Fatal("replay map retained token")
		}
	}
}
