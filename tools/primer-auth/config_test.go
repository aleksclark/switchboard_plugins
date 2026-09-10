package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"time"
)

func configTestValid() config {
	return config{
		LocalAPIKey:   "synthetic-local-api-key-0123456789abcdef",
		ClerkUserID:   "user_synthetic123",
		Email:         "parent@example.test",
		TenantID:      "tenant_synthetic",
		ParentSubject: "parent:synthetic@example.test",
		Port:          9876,
	}
}

func configTestJSON(t *testing.T) string {
	t.Helper()
	data, err := json.Marshal(configTestValid())
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

func configTestFile(t *testing.T, data string, mode os.FileMode) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(path, []byte(data), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(path, mode); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestLoadConfigPrivateFiles(t *testing.T) {
	for _, mode := range []os.FileMode{0400, 0600} {
		t.Run(fmt.Sprintf("%04o", mode), func(t *testing.T) {
			got, err := loadConfig(configTestFile(t, configTestJSON(t), mode))
			if err != nil {
				t.Fatalf("loadConfig: %v", err)
			}
			want := configTestValid()
			want.BrowserCommand = "browser-use-terminal"
			if got != want {
				t.Errorf("config = %#v, want %#v", got, want)
			}
		})
	}
	for _, command := range []string{"custom-browser", "/opt/browser/bin/browser-use-terminal", "/opt/Browser Tools/browser"} {
		t.Run(command, func(t *testing.T) {
			want := configTestValid()
			want.BrowserCommand = command
			data, err := json.Marshal(want)
			if err != nil {
				t.Fatal(err)
			}
			got, err := loadConfig(configTestFile(t, string(data), 0600))
			if err != nil || got != want {
				t.Fatalf("loadConfig = %#v, %v; want %#v", got, err, want)
			}
		})
	}
}

func TestLoadConfigRejectsUnsafeFiles(t *testing.T) {
	data := configTestJSON(t)
	tests := []struct {
		name string
		path func(*testing.T) string
	}{
		{"missing flag", func(t *testing.T) string { return "" }},
		{"missing file", func(t *testing.T) string { return filepath.Join(t.TempDir(), "secret-missing.json") }},
		{"directory", func(t *testing.T) string { return t.TempDir() }},
		{"symlink", func(t *testing.T) string {
			target := configTestFile(t, data, 0600)
			path := filepath.Join(filepath.Dir(target), "secret-link.json")
			if err := os.Symlink(target, path); err != nil {
				t.Fatal(err)
			}
			return path
		}},
		{"dangling symlink", func(t *testing.T) string {
			path := filepath.Join(t.TempDir(), "secret-dangling.json")
			if err := os.Symlink(path+".missing", path); err != nil {
				t.Fatal(err)
			}
			return path
		}},
		{"fifo", func(t *testing.T) string {
			path := filepath.Join(t.TempDir(), "secret-fifo")
			if err := syscall.Mkfifo(path, 0600); err != nil {
				t.Fatal(err)
			}
			return path
		}},
	}
	for _, mode := range []os.FileMode{0601, 0602, 0604, 0610, 0620, 0640, 0644, 0666, 0700, 0777} {
		tests = append(tests, struct {
			name string
			path func(*testing.T) string
		}{fmt.Sprintf("mode %04o", mode), func(t *testing.T) string { return configTestFile(t, data, mode) }})
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			path := tt.path(t)
			got, err := loadConfig(path)
			if err == nil {
				t.Fatal("loadConfig accepted unsafe file")
			}
			if got != (config{}) {
				t.Errorf("failed load returned populated config: %#v", got)
			}
			if strings.Contains(err.Error(), "secret-") || strings.Contains(err.Error(), configTestValid().LocalAPIKey) {
				t.Errorf("error leaked private data: %v", err)
			}
		})
	}
}

type configTestFileInfo struct {
	mode os.FileMode
}

func (info configTestFileInfo) Name() string       { return "config.json" }
func (info configTestFileInfo) Size() int64        { return 0 }
func (info configTestFileInfo) Mode() os.FileMode  { return info.mode }
func (info configTestFileInfo) ModTime() time.Time { return time.Time{} }
func (info configTestFileInfo) IsDir() bool        { return info.mode.IsDir() }
func (info configTestFileInfo) Sys() any           { return nil }

func TestPrivateRegular(t *testing.T) {
	if privateRegular(nil) {
		t.Fatal("nil FileInfo accepted")
	}
	for mode := os.FileMode(0); mode <= 0777; mode++ {
		want := mode == 0 || mode == 0200 || mode == 0400 || mode == 0600
		if got := privateRegular(configTestFileInfo{mode: mode}); got != want {
			t.Errorf("privateRegular(%04o) = %v, want %v", mode, got, want)
		}
	}
	for _, mode := range []os.FileMode{os.ModeDir, os.ModeSymlink, os.ModeNamedPipe, os.ModeSocket, os.ModeDevice, os.ModeCharDevice, os.ModeIrregular, os.ModeSetuid, os.ModeSetgid, os.ModeSticky} {
		if privateRegular(configTestFileInfo{mode: mode | 0600}) {
			t.Errorf("accepted special mode %v", mode)
		}
	}
}

func TestLoadConfigStrictJSON(t *testing.T) {
	valid := configTestJSON(t)
	tests := []struct {
		name string
		data string
	}{
		{"empty", ""},
		{"whitespace", " \n\t"},
		{"null", "null"},
		{"array", "[" + valid + "]"},
		{"string", `"secret-string"`},
		{"number", "42"},
		{"boolean", "true"},
		{"empty object", "{}"},
		{"malformed", strings.TrimSuffix(valid, "}")},
		{"trailing comma", strings.TrimSuffix(valid, "}") + ",}"},
		{"unknown field", strings.TrimSuffix(valid, "}") + `,"secret_unknown":"secret-value"}`},
		{"case variant", strings.Replace(valid, `"local_api_key"`, `"LOCAL_API_KEY"`, 1)},
		{"trailing object", valid + "{}"},
		{"trailing null", valid + " null"},
		{"trailing number", valid + " 1"},
		{"trailing garbage", valid + " secret-garbage"},
		{"invalid utf8", valid + string([]byte{0xff})},
		{"bom", "\ufeff" + valid},
		{"over size limit", valid + strings.Repeat(" ", 16*1024-len(valid)+1)},
		{"wrong port type", strings.Replace(valid, `"port":9876`, `"port":"9876"`, 1)},
		{"fractional port", strings.Replace(valid, `"port":9876`, `"port":9876.5`, 1)},
		{"nested key", strings.Replace(valid, `"clerk_user_id":"user_synthetic123"`, `"clerk_user_id":{"secret":"value"}`, 1)},
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal([]byte(valid), &fields); err != nil {
		t.Fatal(err)
	}
	fields["browser_command"] = json.RawMessage(`"custom-browser"`)
	for _, key := range []string{"local_api_key", "clerk_user_id", "email", "tenant_id", "parent_subject", "port", "browser_command"} {
		prefix := valid
		if key == "browser_command" {
			prefix = strings.TrimSuffix(prefix, "}") + `,"browser_command":"custom-browser"}`
		}
		tests = append(tests, struct{ name, data string }{"duplicate " + key, strings.TrimSuffix(prefix, "}") + fmt.Sprintf(",%q:%s}", key, fields[key])})
	}
	tests = append(tests, struct{ name, data string }{"escaped duplicate", strings.TrimSuffix(valid, "}") + `,"\u0065mail":"secret@example.test"}`})
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := loadConfig(configTestFile(t, tt.data, 0600))
			if err == nil {
				t.Fatal("invalid config accepted")
			}
			if got != (config{}) {
				t.Errorf("failed load returned populated config: %#v", got)
			}
			for _, secret := range []string{"secret-", configTestValid().LocalAPIKey, configTestValid().Email} {
				if strings.Contains(err.Error(), secret) {
					t.Errorf("error leaked %q: %v", secret, err)
				}
			}
		})
	}
	for name, data := range map[string]string{
		"surrounding whitespace": " \n\t" + valid + "\r\n ",
		"exact size limit":       valid + strings.Repeat(" ", 16*1024-len(valid)),
		"escaped known key":      strings.Replace(valid, `"email"`, `"\u0065mail"`, 1),
	} {
		t.Run(name, func(t *testing.T) {
			if _, err := loadConfig(configTestFile(t, data, 0600)); err != nil {
				t.Fatalf("valid JSON rejected: %v", err)
			}
		})
	}
}

func TestLoadConfigRequiredFields(t *testing.T) {
	for _, key := range []string{"local_api_key", "clerk_user_id", "email", "tenant_id", "parent_subject", "port"} {
		for _, missing := range []bool{false, true} {
			t.Run(fmt.Sprintf("%s/missing=%v", key, missing), func(t *testing.T) {
				var fields map[string]any
				if err := json.Unmarshal([]byte(configTestJSON(t)), &fields); err != nil {
					t.Fatal(err)
				}
				if missing {
					delete(fields, key)
				} else {
					fields[key] = nil
				}
				data, err := json.Marshal(fields)
				if err != nil {
					t.Fatal(err)
				}
				got, err := loadConfig(configTestFile(t, string(data), 0600))
				if err == nil || got != (config{}) {
					t.Fatalf("loadConfig = %#v, %v; want zero config and error", got, err)
				}
			})
		}
	}
}

func TestConfigValidation(t *testing.T) {
	tests := []struct {
		name   string
		change func(*config)
		valid  bool
	}{
		{"valid defaults", func(*config) {}, true},
		{"key minimum", func(c *config) { c.LocalAPIKey = strings.Repeat("k", 32) }, true},
		{"key maximum", func(c *config) { c.LocalAPIKey = strings.Repeat("k", 512) }, true},
		{"key punctuation", func(c *config) { c.LocalAPIKey = strings.Repeat("!~", 16) }, true},
		{"key short", func(c *config) { c.LocalAPIKey = strings.Repeat("k", 31) }, false},
		{"key long", func(c *config) { c.LocalAPIKey = strings.Repeat("k", 513) }, false},
		{"key space", func(c *config) { c.LocalAPIKey += " " }, false},
		{"key newline", func(c *config) { c.LocalAPIKey += "\n" }, false},
		{"key del", func(c *config) { c.LocalAPIKey += "\x7f" }, false},
		{"key unicode", func(c *config) { c.LocalAPIKey += "é" }, false},
		{"key invalid utf8", func(c *config) { c.LocalAPIKey += "\xff" }, false},
		{"user missing", func(c *config) { c.ClerkUserID = "" }, false},
		{"user wrong prefix", func(c *config) { c.ClerkUserID = "org_synthetic" }, false},
		{"user case", func(c *config) { c.ClerkUserID = "User_synthetic" }, false},
		{"user slash", func(c *config) { c.ClerkUserID = "user_bad/path" }, false},
		{"user maximum", func(c *config) { c.ClerkUserID = "user_" + strings.Repeat("a", 251) }, true},
		{"user long", func(c *config) { c.ClerkUserID = "user_" + strings.Repeat("a", 252) }, false},
		{"email missing", func(c *config) { c.Email = "" }, false},
		{"email malformed", func(c *config) { c.Email = "parent" }, false},
		{"email display name", func(c *config) { c.Email = "Parent <parent@example.test>" }, false},
		{"email angle brackets", func(c *config) { c.Email = "<parent@example.test>" }, false},
		{"email leading space", func(c *config) { c.Email = " " + c.Email }, false},
		{"email trailing space", func(c *config) { c.Email += " " }, false},
		{"email newline", func(c *config) { c.Email += "\r\n" }, false},
		{"email multiple", func(c *config) { c.Email += ", other@example.test" }, false},
		{"email maximum", func(c *config) { c.Email = "a@" + strings.Repeat("b", 247) + ".test" }, true},
		{"email long", func(c *config) { c.Email = "a@" + strings.Repeat("b", 248) + ".test" }, false},
		{"tenant missing", func(c *config) { c.TenantID = "" }, false},
		{"tenant unicode", func(c *config) { c.TenantID = "tenant_é" }, false},
		{"tenant space", func(c *config) { c.TenantID = "tenant bad" }, false},
		{"tenant maximum", func(c *config) { c.TenantID = strings.Repeat("t", 256) }, true},
		{"tenant long", func(c *config) { c.TenantID = strings.Repeat("t", 257) }, false},
		{"subject missing", func(c *config) { c.ParentSubject = "" }, false},
		{"subject punctuation", func(c *config) { c.ParentSubject = "AZaz09_-.:@" }, true},
		{"subject maximum", func(c *config) { c.ParentSubject = strings.Repeat("p", 256) }, true},
		{"subject long", func(c *config) { c.ParentSubject = strings.Repeat("p", 257) }, false},
		{"subject slash", func(c *config) { c.ParentSubject = "parent/path" }, false},
		{"subject control", func(c *config) { c.ParentSubject += "\x00" }, false},
		{"port minimum", func(c *config) { c.Port = 1 }, true},
		{"port maximum", func(c *config) { c.Port = 65535 }, true},
		{"port zero", func(c *config) { c.Port = 0 }, false},
		{"port negative", func(c *config) { c.Port = -1 }, false},
		{"port too large", func(c *config) { c.Port = 65536 }, false},
		{"command basename", func(c *config) { c.BrowserCommand = "custom-browser" }, true},
		{"command absolute", func(c *config) { c.BrowserCommand = "/opt/browser" }, true},
		{"command maximum", func(c *config) { c.BrowserCommand = strings.Repeat("b", 4096) }, true},
		{"command long", func(c *config) { c.BrowserCommand = strings.Repeat("b", 4097) }, false},
		{"command relative", func(c *config) { c.BrowserCommand = "./browser" }, false},
		{"command parent relative", func(c *config) { c.BrowserCommand = "../browser" }, false},
		{"command nested relative", func(c *config) { c.BrowserCommand = "bin/browser" }, false},
		{"command windows relative", func(c *config) { c.BrowserCommand = `bin\browser` }, false},
		{"command whitespace", func(c *config) { c.BrowserCommand = " " }, false},
		{"command leading space", func(c *config) { c.BrowserCommand = " browser" }, false},
		{"command trailing space", func(c *config) { c.BrowserCommand = "browser " }, false},
		{"command tab", func(c *config) { c.BrowserCommand = "browser\targ" }, false},
		{"command newline", func(c *config) { c.BrowserCommand = "browser\narg" }, false},
		{"command invalid utf8", func(c *config) { c.BrowserCommand = "browser\xff" }, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			cfg := configTestValid()
			tt.change(&cfg)
			err := cfg.validate()
			if (err == nil) != tt.valid {
				t.Fatalf("validate() = %v, want valid=%v", err, tt.valid)
			}
			if tt.valid && cfg.BrowserCommand == "" {
				t.Error("browser command default was not populated")
			}
			if err != nil {
				for _, secret := range []string{configTestValid().LocalAPIKey, configTestValid().Email, "synthetic"} {
					if strings.Contains(err.Error(), secret) {
						t.Errorf("validation error leaked %q: %v", secret, err)
					}
				}
			}
		})
	}
}
