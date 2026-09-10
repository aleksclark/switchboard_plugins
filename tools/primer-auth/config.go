package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"net/mail"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"unicode"
	"unicode/utf8"
)

type config struct {
	LocalAPIKey    string `json:"local_api_key"`
	ClerkUserID    string `json:"clerk_user_id"`
	Email          string `json:"email"`
	TenantID       string `json:"tenant_id"`
	ParentSubject  string `json:"parent_subject"`
	Port           int    `json:"port"`
	BrowserCommand string `json:"browser_command,omitempty"`
}

func loadConfig(path string) (config, error) {
	var cfg config
	if path == "" {
		return cfg, errors.New("-config is required")
	}
	info, err := os.Lstat(path)
	if err != nil || !privateRegular(info) {
		return cfg, errors.New("config must be a private regular file with mode 0600 or stricter, not a symlink")
	}
	fd, err := syscall.Open(path, syscall.O_RDONLY|syscall.O_CLOEXEC|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0)
	if err != nil {
		return cfg, errors.New("cannot securely open config")
	}
	file := os.NewFile(uintptr(fd), path)
	defer file.Close()
	opened, err := file.Stat()
	if err != nil || !privateRegular(opened) || !os.SameFile(info, opened) {
		return cfg, errors.New("config changed or has unsafe permissions")
	}
	data, err := io.ReadAll(io.LimitReader(file, 16*1024+1))
	if err != nil || len(data) > 16*1024 || !utf8.Valid(data) {
		return cfg, errors.New("config unreadable or too large")
	}
	if !uniqueConfigFields(data) || decodeStrict(data, &cfg) != nil {
		return config{}, errors.New("config must be one JSON object with unique, known fields")
	}
	if err := cfg.validate(); err != nil {
		return config{}, err
	}
	return cfg, nil
}

func privateRegular(info os.FileInfo) bool {
	return info != nil && info.Mode().IsRegular() && info.Mode().Perm() & ^os.FileMode(0600) == 0 && info.Mode()&(os.ModeSetuid|os.ModeSetgid|os.ModeSticky) == 0
}

func uniqueConfigFields(data []byte) bool {
	decoder := json.NewDecoder(bytes.NewReader(data))
	token, err := decoder.Token()
	if err != nil || token != json.Delim('{') {
		return false
	}
	seen := make(map[string]bool)
	for decoder.More() {
		token, err = decoder.Token()
		key, ok := token.(string)
		if err != nil || !ok || seen[key] {
			return false
		}
		switch key {
		case "local_api_key", "clerk_user_id", "email", "tenant_id", "parent_subject", "port", "browser_command":
		default:
			return false
		}
		seen[key] = true
		var value json.RawMessage
		if decoder.Decode(&value) != nil {
			return false
		}
	}
	_, err = decoder.Token()
	return err == nil
}

func decodeStrict(data []byte, value any) error {
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(value); err != nil {
		return err
	}
	if decoder.Decode(new(any)) != io.EOF {
		return errors.New("trailing JSON")
	}
	return nil
}

func (cfg *config) validate() error {
	if len(cfg.LocalAPIKey) < 32 || len(cfg.LocalAPIKey) > 512 || !asciiToken(cfg.LocalAPIKey) {
		return errors.New("local_api_key must contain 32 to 512 printable non-space ASCII characters; generate it randomly")
	}
	if !strings.HasPrefix(cfg.ClerkUserID, "user_") || len(cfg.ClerkUserID) == len("user_") || !identifier(cfg.ClerkUserID) {
		return errors.New("clerk_user_id must be a Clerk user identifier")
	}
	address, err := mail.ParseAddress(cfg.Email)
	if err != nil || address.Address != cfg.Email || !plainText(cfg.Email, 254) {
		return errors.New("email must be a plain email address")
	}
	if !identifier(cfg.TenantID) || !identifier(cfg.ParentSubject) {
		return errors.New("tenant_id and parent_subject must be nonempty identifiers")
	}
	if cfg.Port < 1 || cfg.Port > 65535 {
		return errors.New("port must be between 1 and 65535")
	}
	if cfg.BrowserCommand == "" {
		cfg.BrowserCommand = "browser-use-terminal"
	}
	if !plainText(cfg.BrowserCommand, 4096) || (!filepath.IsAbs(cfg.BrowserCommand) && (strings.ContainsAny(cfg.BrowserCommand, "/\\") || strings.ContainsFunc(cfg.BrowserCommand, unicode.IsSpace))) {
		return errors.New("browser_command must be an executable name or absolute path, without arguments")
	}
	return nil
}

func identifier(value string) bool {
	if len(value) == 0 || len(value) > 256 {
		return false
	}
	for _, c := range value {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || strings.ContainsRune("_-.:@", c)) {
			return false
		}
	}
	return true
}

func asciiToken(value string) bool {
	for _, c := range value {
		if c < 33 || c > 126 {
			return false
		}
	}
	return true
}

func plainText(value string, max int) bool {
	return len(value) > 0 && len(value) <= max && utf8.ValidString(value) && strings.TrimSpace(value) == value && !strings.ContainsFunc(value, unicode.IsControl)
}
