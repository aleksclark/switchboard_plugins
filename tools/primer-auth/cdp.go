package main

import (
	"context"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"strconv"
	"sync/atomic"
	"time"

	"github.com/coder/websocket"
)

type cdpClient struct {
	ws     *websocket.Conn
	nextID int
	broken bool
}

type limitedCDPConn struct {
	net.Conn
	reader io.Reader
}

func (conn *limitedCDPConn) Read(p []byte) (int, error) { return conn.reader.Read(p) }

func evaluateCDP(ctx context.Context, endpoint, expression string) (browserResult, error) {
	ctx, cancel := context.WithTimeout(ctx, browserTimeout)
	defer cancel()
	match := browserEndpointPattern.FindStringSubmatch(endpoint)
	if match == nil {
		return browserResult{}, errBrowser
	}
	port, err := strconv.Atoi(match[1])
	if err != nil || port > 65535 {
		return browserResult{}, errBrowser
	}
	address := net.JoinHostPort("127.0.0.1", match[1])
	raw, err := (&net.Dialer{}).DialContext(ctx, "tcp4", address)
	if err != nil {
		return browserResult{}, errBrowser
	}
	defer raw.Close()
	deadline, _ := ctx.Deadline()
	if raw.SetDeadline(deadline) != nil {
		return browserResult{}, errBrowser
	}
	stop := context.AfterFunc(ctx, func() { _ = raw.Close() })
	defer stop()
	conn := &limitedCDPConn{Conn: raw, reader: io.LimitReader(raw, 8*maxBrowserOutput)}
	var dialed atomic.Bool
	transport := &http.Transport{
		DialContext: func(dialCtx context.Context, network, target string) (net.Conn, error) {
			if dialCtx.Err() != nil || network != "tcp" || target != address || dialed.Swap(true) {
				return nil, errBrowser
			}
			return conn, nil
		},
		DisableKeepAlives:      true,
		DisableCompression:     true,
		MaxResponseHeaderBytes: maxBrowserOutput,
		ResponseHeaderTimeout:  browserTimeout,
	}
	defer transport.CloseIdleConnections()
	httpClient := &http.Client{
		Transport: transport,
		CheckRedirect: func(*http.Request, []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
	ws, _, err := websocket.Dial(ctx, endpoint, &websocket.DialOptions{HTTPClient: httpClient})
	if err != nil {
		return browserResult{}, errBrowser
	}
	defer ws.CloseNow()
	ws.SetReadLimit(maxBrowserOutput)
	client := &cdpClient{ws: ws}
	return client.evaluate(ctx, expression)
}

func (client *cdpClient) command(ctx context.Context, method, session string, params, result any) error {
	if client.broken || ctx.Err() != nil {
		return errBrowser
	}
	client.nextID++
	command := struct {
		ID        int    `json:"id"`
		Method    string `json:"method"`
		SessionID string `json:"sessionId,omitempty"`
		Params    any    `json:"params"`
	}{client.nextID, method, session, params}
	data, err := json.Marshal(command)
	if err != nil || client.ws.Write(ctx, websocket.MessageText, data) != nil {
		client.broken = true
		return errBrowser
	}
	for {
		_, data, err := client.ws.Read(ctx)
		if err != nil || ctx.Err() != nil {
			client.broken = true
			return errBrowser
		}
		fields, err := objectFields(data)
		if err != nil {
			client.broken = true
			return errBrowser
		}
		if _, ok := fields["id"]; !ok {
			continue
		}
		var id int
		if json.Unmarshal(fields["id"], &id) != nil || id != command.ID {
			client.broken = true
			return errBrowser
		}
		var replySession string
		if value, ok := fields["sessionId"]; ok && json.Unmarshal(value, &replySession) != nil {
			client.broken = true
			return errBrowser
		}
		if replySession != session {
			client.broken = true
			return errBrowser
		}
		if _, failed := fields["error"]; failed || json.Unmarshal(fields["result"], result) != nil {
			return errBrowser
		}
		return nil
	}
}

type cdpTarget struct {
	ID   string `json:"targetId"`
	Type string `json:"type"`
	URL  string `json:"url"`
}

func selectPrimerTarget(targets []cdpTarget) (string, error) {
	var id string
	count := 0
	for _, target := range targets {
		if target.Type == "page" && primerTabURL(target.URL) {
			count++
			id = target.ID
		}
	}
	if count != 1 || id == "" {
		return "", errBrowser
	}
	return id, nil
}

func (client *cdpClient) evaluate(ctx context.Context, expression string) (browserResult, error) {
	var targets struct {
		Infos []cdpTarget `json:"targetInfos"`
	}
	if client.command(ctx, "Target.getTargets", "", struct{}{}, &targets) != nil {
		return browserResult{}, errBrowser
	}
	target, err := selectPrimerTarget(targets.Infos)
	if err != nil {
		return browserResult{}, errBrowser
	}
	var attached struct {
		SessionID string `json:"sessionId"`
	}
	if client.command(ctx, "Target.attachToTarget", "", map[string]any{"targetId": target, "flatten": true}, &attached) != nil || attached.SessionID == "" {
		return browserResult{}, errBrowser
	}
	defer client.detach(ctx, attached.SessionID)
	var evaluation struct {
		Result struct {
			Type  string          `json:"type"`
			Value json.RawMessage `json:"value"`
		} `json:"result"`
		Exception json.RawMessage `json:"exceptionDetails"`
	}
	params := map[string]any{"expression": expression, "awaitPromise": true, "returnByValue": true}
	if client.command(ctx, "Runtime.evaluate", attached.SessionID, params, &evaluation) != nil || len(evaluation.Exception) != 0 || evaluation.Result.Type != "object" {
		return browserResult{}, errBrowser
	}
	return parseBrowserResult(evaluation.Result.Value)
}

func (client *cdpClient) detach(ctx context.Context, session string) {
	if client.broken || ctx.Err() != nil {
		return
	}
	ctx, cancel := context.WithTimeout(ctx, 250*time.Millisecond)
	defer cancel()
	var result json.RawMessage
	_ = client.command(ctx, "Target.detachFromTarget", "", map[string]any{"sessionId": session}, &result)
}
