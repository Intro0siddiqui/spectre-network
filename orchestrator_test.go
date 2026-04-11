package main

import (
	"testing"
)

func TestValidateMode(t *testing.T) {
	tests := []struct {
		mode string
		want bool
	}{
		{"lite", true},
		{"stealth", true},
		{"high", true},
		{"phantom", true},
		{"solo", false},
		{"cascade", false},
		{"mesh", false},
		{"pool", false},
		{"invalid", false},
		{"", false},
	}

	for _, tt := range tests {
		t.Run(tt.mode, func(t *testing.T) {
			if got := validateMode(tt.mode); got != tt.want {
				t.Errorf("validateMode(%q) = %v; want %v", tt.mode, got, tt.want)
			}
		})
	}
}

func TestValidateLimit(t *testing.T) {
	tests := []struct {
		name  string
		limit int
		want  bool
	}{
		{"min valid", 1, true},
		{"typical", 500, true},
		{"max valid", 10000, true},
		{"zero", 0, false},
		{"negative", -1, false},
		{"over max", 10001, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := validateLimit(tt.limit); got != tt.want {
				t.Errorf("validateLimit(%d) = %v; want %v", tt.limit, got, tt.want)
			}
		})
	}
}

func TestValidateProtocol(t *testing.T) {
	tests := []struct {
		proto string
		want  bool
	}{
		{"all", true},
		{"socks5", true},
		{"https", true},
		{"http", true},
		{"socks4", false},
		{"ftp", false},
		{"", false},
	}

	for _, tt := range tests {
		t.Run(tt.proto, func(t *testing.T) {
			if got := validateProtocol(tt.proto); got != tt.want {
				t.Errorf("validateProtocol(%q) = %v; want %v", tt.proto, got, tt.want)
			}
		})
	}
}

func TestSanitizeMode(t *testing.T) {
	tests := []struct {
		name      string
		input     string
		wantMode  string
		wantValid bool
	}{
		{"uppercase", "PHANTOM", "phantom", true},
		{"whitespace", "  lite  ", "lite", true},
		{"mixed case", "Stealth", "stealth", true},
		{"valid high", "HIGH", "high", true},
		{"invalid", "invalid", "", false},
		{"empty after trim", "  ", "", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			gotMode, gotValid := sanitizeMode(tt.input)
			if gotMode != tt.wantMode || gotValid != tt.wantValid {
				t.Errorf("sanitizeMode(%q) = (%q, %v); want (%q, %v)", tt.input, gotMode, gotValid, tt.wantMode, tt.wantValid)
			}
		})
	}
}
