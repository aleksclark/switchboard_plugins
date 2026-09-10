package main

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"log/slog"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestRunFlagsAndConfiguration(t *testing.T) {
	for _, tt := range []struct {
		name string
		args []string
		code int
	}{
		{"help", []string{"-help"}, 0},
		{"unknown flag", []string{"-unknown"}, 2},
		{"missing flag", nil, 1},
		{"missing flag value", []string{"-config"}, 2},
		{"positional", []string{"unexpected"}, 2},
		{"missing file", []string{"-config", "/not/a/real/private-config.json"}, 1},
	} {
		t.Run(tt.name, func(t *testing.T) {
			var output bytes.Buffer
			if code := run(tt.args, &output); code != tt.code {
				t.Fatalf("run = %d, want %d", code, tt.code)
			}
			if output.Len() == 0 {
				t.Fatal("no operational feedback")
			}
		})
	}
	cfg := configTestValid()
	cfg.BrowserCommand = "/not/a/real/private-browser-path"
	data, err := json.Marshal(cfg)
	if err != nil {
		t.Fatal(err)
	}
	path := configTestFile(t, string(data), 0600)
	var output bytes.Buffer
	if code := run([]string{"-config", path}, &output); code != 1 {
		t.Fatalf("missing browser exit = %d", code)
	}
	for _, private := range []string{cfg.BrowserCommand, cfg.LocalAPIKey, cfg.Email, path} {
		if strings.Contains(output.String(), private) {
			t.Fatal("startup error leaked configuration")
		}
	}
}

func TestServeLifecycle(t *testing.T) {
	listener, err := net.Listen("tcp4", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	cfg := configTestValid()
	cfg.Port = listener.Addr().(*net.TCPAddr).Port
	cfg.BrowserCommand = filepath.Join(t.TempDir(), "must-not-be-executed")
	address := listener.Addr().String()
	logger := slog.New(slog.NewJSONHandler(io.Discard, nil))
	if err := serve(t.Context(), cfg, logger); err == nil {
		t.Fatal("accepted occupied listener address")
	}
	if err := listener.Close(); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(t.Context())
	defer cancel()
	done := make(chan error, 1)
	go func() { done <- serve(ctx, cfg, logger) }()
	transport := &http.Transport{Proxy: nil}
	defer transport.CloseIdleConnections()
	client := &http.Client{Transport: transport, Timeout: 100 * time.Millisecond}
	deadline := time.Now().Add(3 * time.Second)
	for {
		response, err := client.Get("http://" + address + "/health")
		if err == nil {
			body, readErr := io.ReadAll(response.Body)
			response.Body.Close()
			if readErr != nil || response.StatusCode != 200 || string(body) != `{"ready":true}` {
				t.Fatalf("health = %d, %s, %v", response.StatusCode, body, readErr)
			}
			break
		}
		select {
		case err := <-done:
			t.Fatalf("server stopped during startup: %v", err)
		default:
		}
		if time.Now().After(deadline) {
			t.Fatal("server did not become ready")
		}
		time.Sleep(5 * time.Millisecond)
	}
	cancel()
	select {
	case err := <-done:
		if err != nil {
			t.Fatalf("shutdown = %v", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("server failed to shut down promptly")
	}
	if _, err := os.Stat(cfg.BrowserCommand); !os.IsNotExist(err) {
		t.Fatal("unexpected browser artifact")
	}
}

func TestValidationRegressions(t *testing.T) {
	for _, change := range []func(*config){
		func(cfg *config) { cfg.ClerkUserID = "user_" },
		func(cfg *config) { cfg.BrowserCommand = "browser-use-terminal --headless" },
	} {
		cfg := configTestValid()
		change(&cfg)
		if cfg.validate() == nil {
			t.Fatal("accepted incomplete identity or executable argument string")
		}
	}
}
