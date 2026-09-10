package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/url"
	"os"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"
	"unicode/utf8"
)

const (
	maxBrowserOutput = 2 * 1024 * 1024
	browserTimeout   = 28 * time.Second
)

var errBrowser = errors.New("browser unavailable")

type browserResult struct {
	Status int             `json:"status"`
	Body   json.RawMessage `json:"body"`
}

type browserRunner interface {
	Run(context.Context, browserRequest) (browserResult, error)
}

type commandRunner struct {
	cfg         config
	unavailable atomic.Bool
}

func (runner *commandRunner) Run(ctx context.Context, request browserRequest) (browserResult, error) {
	if runner.unavailable.Load() {
		return browserResult{}, errBrowser
	}
	ctx, cancel := context.WithTimeout(ctx, browserTimeout)
	defer cancel()
	deadline, _ := ctx.Deadline()
	script, err := browserScript(runner.cfg, request, deadline)
	if err != nil {
		return browserResult{}, errBrowser
	}
	endpoint, err := discoverBrowser(ctx, runner.cfg.BrowserCommand)
	if err != nil {
		runner.unavailable.Store(true)
		return browserResult{}, errBrowser
	}
	result, err := evaluateCDP(ctx, endpoint, script)
	if err != nil || ctx.Err() != nil {
		runner.unavailable.Store(true)
		return browserResult{}, errBrowser
	}
	if result.Status == 502 {
		runner.unavailable.Store(true)
	}
	return result, nil
}

func discoverBrowser(ctx context.Context, executable string) (string, error) {
	dir, err := os.MkdirTemp("", "primer-auth-discovery-")
	if err != nil {
		return "", errBrowser
	}
	defer os.RemoveAll(dir)
	ctx, cancel := context.WithTimeout(ctx, browserTimeout)
	defer cancel()
	output := &outputBudget{cancel: cancel}
	command := exec.CommandContext(ctx, executable, "browser", "status", "--json")
	command.Dir = dir
	command.Stdout = outputStream{budget: output, stdout: true}
	command.Stderr = outputStream{budget: output}
	command.Env = browserEnvironment()
	command.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	command.Cancel = func() error {
		err := syscall.Kill(-command.Process.Pid, syscall.SIGKILL)
		if errors.Is(err, syscall.ESRCH) {
			return os.ErrProcessDone
		}
		return err
	}
	command.WaitDelay = 250 * time.Millisecond
	if command.Run() != nil || ctx.Err() != nil || output.exceeded {
		return "", errBrowser
	}
	return parseBrowserStatus(output.stdout.Bytes())
}

var browserEndpointPattern = regexp.MustCompile(`^ws://127\.0\.0\.1:([1-9][0-9]{0,4})/devtools/browser/[a-zA-Z0-9_-]+$`)

func parseBrowserStatus(data []byte) (string, error) {
	status, err := objectFields(data)
	if err != nil {
		return "", errBrowser
	}
	var connection, mode string
	if json.Unmarshal(status["connection"], &connection) != nil || connection != "connected" ||
		json.Unmarshal(status["mode"], &mode) != nil || mode != "managed" {
		return "", errBrowser
	}
	endpoint, err := objectFields(status["endpoint"])
	if err != nil {
		return "", errBrowser
	}
	var address string
	if json.Unmarshal(endpoint["ws_url"], &address) != nil {
		return "", errBrowser
	}
	match := browserEndpointPattern.FindStringSubmatch(address)
	if match == nil {
		return "", errBrowser
	}
	port, err := strconv.Atoi(match[1])
	if err != nil || port > 65535 {
		return "", errBrowser
	}
	return address, nil
}

func objectFields(data []byte) (map[string]json.RawMessage, error) {
	if len(data) > maxBrowserOutput || !utf8.Valid(data) {
		return nil, errBrowser
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	token, err := decoder.Token()
	if err != nil || token != json.Delim('{') {
		return nil, errBrowser
	}
	fields := make(map[string]json.RawMessage)
	for decoder.More() {
		token, err = decoder.Token()
		key, ok := token.(string)
		if err != nil || !ok || fields[key] != nil {
			return nil, errBrowser
		}
		var value json.RawMessage
		if decoder.Decode(&value) != nil {
			return nil, errBrowser
		}
		fields[key] = value
	}
	if _, err := decoder.Token(); err != nil || decoder.Decode(new(any)) != io.EOF {
		return nil, errBrowser
	}
	return fields, nil
}

func primerTabURL(address string) bool {
	parsed, err := url.Parse(address)
	return err == nil && parsed.Scheme == "https" && parsed.Host == "api.primerlms.com" &&
		parsed.User == nil && parsed.Opaque == "" && parsed.RawPath == "" && strings.HasPrefix(parsed.Path, "/tasks/")
}

type outputBudget struct {
	mu       sync.Mutex
	stdout   bytes.Buffer
	total    int
	exceeded bool
	cancel   context.CancelFunc
}

type outputStream struct {
	budget *outputBudget
	stdout bool
}

func (stream outputStream) Write(data []byte) (int, error) {
	budget := stream.budget
	budget.mu.Lock()
	defer budget.mu.Unlock()
	if budget.exceeded || len(data) > maxBrowserOutput-budget.total {
		budget.exceeded = true
		budget.cancel()
		return 0, errBrowser
	}
	budget.total += len(data)
	if stream.stdout {
		return budget.stdout.Write(data)
	}
	return len(data), nil
}

func browserEnvironment() []string {
	keys := []string{"PATH", "HOME"}
	env := make([]string, 0, len(keys))
	for _, key := range keys {
		if value, ok := os.LookupEnv(key); ok {
			env = append(env, key+"="+value)
		}
	}
	return env
}

func parseBrowserResult(data []byte) (browserResult, error) {
	var result browserResult
	if len(data) > maxBrowserOutput || !validResultEnvelope(data) || decodeStrict(data, &result) != nil || !validBrowserResult(result) {
		return browserResult{}, errBrowser
	}
	return result, nil
}

func validResultEnvelope(data []byte) bool {
	if !utf8.Valid(data) {
		return false
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	token, err := decoder.Token()
	if err != nil || token != json.Delim('{') {
		return false
	}
	seen := make(map[string]bool, 2)
	for decoder.More() {
		token, err = decoder.Token()
		key, ok := token.(string)
		if err != nil || !ok || (key != "status" && key != "body") || seen[key] {
			return false
		}
		seen[key] = true
		if decoder.Decode(new(json.RawMessage)) != nil {
			return false
		}
	}
	_, err = decoder.Token()
	return err == nil && len(seen) == 2 && decoder.Decode(new(any)) == io.EOF
}

func validBrowserResult(result browserResult) bool {
	return result.Status >= 200 && result.Status <= 599 && (result.Status < 300 || result.Status >= 400) &&
		result.Status != 205 && result.Status != 304 &&
		len(result.Body) <= maxBrowserOutput && utf8.Valid(result.Body) && json.Valid(result.Body)
}

func failureBody(write bool) json.RawMessage {
	if write {
		return json.RawMessage(`{"error":"Browser request failed; write outcome unknown. Check state before any manual retry."}`)
	}
	return json.RawMessage(`{"error":"Browser session or upstream request unavailable."}`)
}
