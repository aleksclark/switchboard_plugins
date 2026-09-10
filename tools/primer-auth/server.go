package main

import (
	"context"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/json"
	"log/slog"
	"net/http"
	"strconv"
	"strings"
	"sync/atomic"
	"time"
)

type companion struct {
	host    string
	keyHash [sha256.Size]byte
	runner  browserRunner
	logger  *slog.Logger
	busy    atomic.Bool
	timeout time.Duration
}

func newCompanion(cfg config, runner browserRunner, logger *slog.Logger) *companion {
	return &companion{
		host:    "127.0.0.1:" + strconv.Itoa(cfg.Port),
		keyHash: sha256.Sum256([]byte("Bearer " + cfg.LocalAPIKey)),
		runner:  runner,
		logger:  logger,
		timeout: browserTimeout,
	}
}

func (server *companion) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Header().Set("Content-Type", "application/json")
	publicHealth := r.Method == http.MethodGet && r.URL != nil && r.URL.Path == "/health"
	if !publicHealth && !server.authorized(r) {
		w.Header().Set("WWW-Authenticate", "Bearer")
		server.reject(w, http.StatusUnauthorized)
		return
	}
	if r.Host != server.host || hasHeader(r.Header, "Origin") {
		server.reject(w, http.StatusForbidden)
		return
	}
	if hasHeader(r.Header, "Upgrade") || hasHeader(r.Header, "Expect") {
		server.reject(w, http.StatusBadRequest)
		return
	}
	if publicHealth {
		server.health(w, r)
		return
	}
	matched, path, status := validateTarget(r)
	if status != 0 {
		server.reject(w, status)
		return
	}
	if !server.busy.CompareAndSwap(false, true) {
		server.reject(w, http.StatusServiceUnavailable)
		return
	}
	defer server.busy.Store(false)
	body, status := readBody(w, r, matched.body)
	if status != 0 {
		server.reject(w, status)
		return
	}
	server.forward(w, r, browserRequest{Method: r.Method, Path: path, Body: body})
}

func (server *companion) authorized(r *http.Request) bool {
	values := r.Header.Values("Authorization")
	if len(values) != 1 || !strings.HasPrefix(values[0], "Bearer ") {
		return false
	}
	provided := sha256.Sum256([]byte(values[0]))
	return subtle.ConstantTimeCompare(provided[:], server.keyHash[:]) == 1
}

func (server *companion) health(w http.ResponseWriter, r *http.Request) {
	if r.URL.IsAbs() || r.URL.Host != "" || r.URL.Opaque != "" || r.URL.RawPath != "" || r.URL.RawQuery != "" || r.URL.ForceQuery || r.URL.Fragment != "" || (r.RequestURI != "" && r.RequestURI != "/health") {
		server.reject(w, http.StatusBadRequest)
		return
	}
	if _, status := readBody(w, r, noBody); status != 0 {
		server.reject(w, status)
		return
	}
	writeJSON(w, http.StatusOK, json.RawMessage(`{"ready":true}`))
}

func (server *companion) forward(w http.ResponseWriter, r *http.Request, request browserRequest) {
	timeout := server.timeout
	if timeout <= 0 || timeout > browserTimeout {
		timeout = browserTimeout
	}
	ctx, cancel := context.WithTimeout(r.Context(), timeout)
	defer cancel()
	result, err := server.runner.Run(ctx, request)
	if err != nil || ctx.Err() != nil || !validBrowserResult(result) {
		status := http.StatusBadGateway
		if ctx.Err() != nil {
			status = http.StatusGatewayTimeout
		}
		server.log("browser_failed", status)
		writeJSON(w, status, failureBody(request.Method != http.MethodGet))
		return
	}
	server.log("upstream_completed", result.Status)
	writeJSON(w, result.Status, result.Body)
}

func (server *companion) reject(w http.ResponseWriter, status int) {
	server.log("request_rejected", status)
	body, _ := json.Marshal(struct {
		Error string `json:"error"`
	}{http.StatusText(status)})
	writeJSON(w, status, body)
}

func (server *companion) log(event string, status int) {
	if server.logger != nil {
		server.logger.Info(event, "status", status)
	}
}

func writeJSON(w http.ResponseWriter, status int, body json.RawMessage) {
	w.WriteHeader(status)
	if status == http.StatusNoContent {
		return
	}
	_, _ = w.Write(body)
}
