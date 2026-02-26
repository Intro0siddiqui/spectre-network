# Spectre Network: Product Guidelines

## Prose & Documentation Style
- **Technical & Minimal**: Documentation and logs must be precise and concise. Avoid fluff; prioritize raw data and technical specs.
- **Generalist Focus**: Maintain a balanced focus between architectural clarity, security metrics, and CLI utility.
- **Visual Structure**: Use tables, bullet points, and headers (as seen in the current README) to organize technical information efficiently.

## User Experience (UX) & CLI Principles
- **Efficiency-Centric**: Primary tasks (e.g., `spectre run`, `spectre serve`) should remain one-command workflows with sensible defaults.
- **Stealth vs. Information**: Provide a balance between "stealth" (low-feedback operations) and "information-rich" (real-time health stats and progress bars) modes.
- **Platform Native**: Prioritize a clean, ANSI-colored CLI experience that works across standard terminal emulators.

## Error Handling & Security
- **Security-Hardened Errors**: When interacting with the network, prioritize generic error messages to avoid leaking sensitive proxy or network-topology details to unauthorized observers.
- **Fail-Fast Logic**: For critical network or encryption failures, the system must fail immediately and loudly (in local logs) to prevent unencrypted or insecure traffic leakage.
- **Domain-Specific Errors**: Go handles network-level errors (timeouts, pings), while Rust handles internal system-level errors (crypto failures, scoring logic).
