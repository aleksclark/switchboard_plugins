package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"mime"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"
)

const maxRequestBody = 256 * 1024

type bodyPolicy int

const (
	noBody bodyPolicy = iota
	objectBody
	emptyObjectBody
)

type browserRequest struct {
	Method string          `json:"method"`
	Path   string          `json:"path"`
	Body   json.RawMessage `json:"body,omitempty"`
}

type route struct {
	body  bodyPolicy
	query string
}

func matchRoute(method, path string) (route, bool) {
	if method == http.MethodGet {
		switch path {
		case "/students", "/schedules", "/occurrences":
			return route{query: "list"}, true
		case "/tasks":
			return route{query: "tasks"}, true
		}
	}
	if method == http.MethodPost && (path == "/tasks" || path == "/schedules") {
		return route{body: objectBody}, true
	}
	parts := strings.Split(path, "/")
	if len(parts) < 3 || len(parts) > 4 || parts[0] != "" || !validUUID(parts[2]) {
		return route{}, false
	}
	if len(parts) == 3 {
		if parts[1] == "occurrences" && method == http.MethodGet {
			return route{}, true
		}
		if parts[1] == "schedules" {
			switch method {
			case http.MethodPatch:
				return route{body: objectBody}, true
			case http.MethodDelete:
				return route{}, true
			}
		}
		return route{}, false
	}
	if method != http.MethodPost {
		return route{}, false
	}
	switch parts[1] + "/" + parts[3] {
	case "tasks/revisions", "occurrences/decision":
		return route{body: objectBody}, true
	case "tasks/publish", "tasks/retire", "occurrences/skip", "occurrences/cancel":
		return route{body: emptyObjectBody}, true
	case "occurrences/retry":
		return route{query: "retry"}, true
	}
	return route{}, false
}

func validUUID(value string) bool {
	if len(value) != 36 {
		return false
	}
	for i, c := range value {
		if i == 8 || i == 13 || i == 18 || i == 23 {
			if c != '-' {
				return false
			}
		} else if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f' || c >= 'A' && c <= 'F') {
			return false
		}
	}
	return true
}

func validateTarget(r *http.Request) (route, string, int) {
	if r.URL == nil || r.URL.IsAbs() || r.URL.Host != "" || r.URL.Opaque != "" || r.URL.Fragment != "" || r.URL.RawPath != "" || strings.ContainsAny(r.URL.Path, "%\\") {
		return route{}, "", http.StatusBadRequest
	}
	if r.RequestURI != "" && r.RequestURI != r.URL.RequestURI() {
		return route{}, "", http.StatusBadRequest
	}
	matched, ok := matchRoute(r.Method, r.URL.Path)
	if !ok {
		return route{}, "", http.StatusNotFound
	}
	query, err := validateQuery(r.URL.RawQuery, matched.query)
	if err != nil || r.URL.ForceQuery {
		return route{}, "", http.StatusBadRequest
	}
	path := r.URL.Path
	if query != "" {
		path += "?" + query
	}
	return matched, path, 0
}

func validateQuery(raw, kind string) (string, error) {
	invalid := errors.New("invalid query")
	if len(raw) > 8192 || strings.Contains(raw, ";") {
		return "", invalid
	}
	values, err := url.ParseQuery(raw)
	if err != nil {
		return "", invalid
	}
	if raw != "" {
		for _, part := range strings.Split(raw, "&") {
			key, _, ok := strings.Cut(part, "=")
			if !ok || key == "" || strings.Contains(key, "%") {
				return "", invalid
			}
		}
	}
	for key, entries := range values {
		if len(entries) != 1 || !queryKeyAllowed(kind, key) {
			return "", invalid
		}
		value := entries[0]
		if len(value) > 2048 || !utf8.ValidString(value) || strings.ContainsFunc(value, unicode.IsControl) {
			return "", invalid
		}
		switch key {
		case "limit", "offset":
			if value == "" || strings.ContainsFunc(value, func(c rune) bool { return c < '0' || c > '9' }) {
				return "", invalid
			}
			n, err := strconv.ParseUint(value, 10, 31)
			if err != nil || key == "limit" && (n < 1 || n > 200) {
				return "", invalid
			}
		case "dir":
			if value != "asc" && value != "desc" {
				return "", invalid
			}
		case "view":
			if value != "templates" {
				return "", invalid
			}
		case "requirementId", "attemptId":
			if !validUUID(value) {
				return "", invalid
			}
		}
	}
	return values.Encode(), nil
}

func queryKeyAllowed(kind, key string) bool {
	switch kind {
	case "list", "tasks":
		switch key {
		case "limit", "offset", "q", "sort", "dir":
			return true
		case "filter":
			return kind == "list"
		case "status", "view":
			return kind == "tasks"
		}
	case "retry":
		return key == "requirementId" || key == "attemptId"
	}
	return false
}

func readBody(w http.ResponseWriter, r *http.Request, policy bodyPolicy) (json.RawMessage, int) {
	if len(r.Trailer) != 0 || hasHeader(r.Header, "Trailer") || hasHeader(r.Header, "Content-Encoding") {
		return nil, http.StatusBadRequest
	}
	if r.Body == nil {
		r.Body = http.NoBody
	}
	limit := int64(maxRequestBody)
	if policy == noBody {
		limit = 0
	}
	data, err := io.ReadAll(http.MaxBytesReader(w, r.Body, limit))
	if err != nil {
		var tooLarge *http.MaxBytesError
		if errors.As(err, &tooLarge) && policy != noBody {
			return nil, http.StatusRequestEntityTooLarge
		}
		return nil, http.StatusBadRequest
	}
	if policy == noBody {
		return nil, 0
	}
	if len(data) == 0 && policy == emptyObjectBody {
		return json.RawMessage(`{}`), 0
	}
	mediaType, params, err := mime.ParseMediaType(r.Header.Get("Content-Type"))
	if err != nil || mediaType != "application/json" || len(r.Header.Values("Content-Type")) != 1 {
		return nil, http.StatusUnsupportedMediaType
	}
	for key, value := range params {
		if key != "charset" || !strings.EqualFold(value, "utf-8") {
			return nil, http.StatusUnsupportedMediaType
		}
	}
	data = bytes.TrimSpace(data)
	if len(data) == 0 || data[0] != '{' || !utf8.Valid(data) || !json.Valid(data) {
		return nil, http.StatusBadRequest
	}
	if policy == emptyObjectBody {
		var fields map[string]json.RawMessage
		if json.Unmarshal(data, &fields) != nil || len(fields) != 0 {
			return nil, http.StatusBadRequest
		}
	}
	return json.RawMessage(data), 0
}

func hasHeader(header http.Header, name string) bool {
	for key := range header {
		if strings.EqualFold(key, name) {
			return true
		}
	}
	return false
}
