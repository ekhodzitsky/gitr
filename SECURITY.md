# Security Policy

## Supported Versions

| Version | Supported |
|---|---|
| 0.1.x | ✅ |

## Reporting a Vulnerability

Please report security vulnerabilities privately via GitHub Security Advisories
or by emailing the maintainer directly.

Do **not** open public issues for security vulnerabilities.

## Security Considerations

`gitr` shells out to the `git` binary. Ensure the `git` binary in your `PATH`
is trustworthy. `gitr` sets non-interactive environment variables
(`GIT_TERMINAL_PROMPT=0`, `GIT_ASKPASS=echo`) to prevent prompts that could
block or confuse automation.

Supply-chain security is enforced via `cargo-deny` in CI. See `deny.toml` for
license and advisory policies.
