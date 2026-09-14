# False Positive Filtering Rules

Apply these rules to every finding before including it in the report. Source: Anthropic's claude-code-security-review.

## Hard Exclusions — Automatically Remove

1. **DOS/Resource Exhaustion** — Denial of service, resource exhaustion, infinite loops, unbounded recursion
2. **Rate Limiting** — Missing rate limiting recommendations
3. **Resource Leaks** — Unclosed files/connections, memory leaks, file descriptor leaks
4. **Open Redirects** — Unvalidated redirect vulnerabilities (low impact)
5. **Regex Injection** — Injecting untrusted content into regex, regex DOS
6. **Memory Safety in Non-C/C++** — Buffer overflows, use-after-free, null pointer dereference in Rust, Go, Python, JS, etc. (only valid in C/C++)
7. **SSRF in Client-Side Code** — SSRF in `.js`, `.ts`, `.tsx`, `.jsx` files (client-side can't bypass firewalls)
8. **SSRF Path-Only** — SSRF that only controls path, not host or protocol
9. **Test Files** — Findings in `*_test.*`, `*.test.*`, `__tests__/`, or files only used for testing
10. **Documentation** — Findings in `.md`, `.rst`, or other documentation files
11. **Secrets on Disk** — Secrets/credentials stored on disk (managed by separate processes)
12. **Log Spoofing** — Outputting unsanitized user input to logs is not a vulnerability
13. **Missing Hardening** — Lack of security best practices without a concrete vulnerability
14. **Theoretical Race Conditions** — Only report race conditions that are concretely problematic
15. **Outdated Dependencies** — Managed separately, not reported here
16. **Crash-Only Bugs** — Undefined/null variables that crash but aren't exploitable
17. **AI Prompt Injection** — User-controlled content in AI system prompts is not a vulnerability
18. **Internal Package Dependencies** — Depending on non-public internal libraries is not a vulnerability
19. **Log Query Injection** — Only report if injection definitely exposes sensitive data to external users
20. **Path Traversal in HTTP** — `../` in HTTP request paths is generally not exploitable; only relevant for local file reads

## Signal Quality Criteria

For findings that pass hard exclusions, assess:

1. Is there a **concrete, exploitable vulnerability** with a clear attack path?
2. Does this represent a **real security risk** vs theoretical best practice?
3. Are there **specific code locations** and reproduction steps?
4. Would this finding be **actionable** for a security team?

## Precedents — Framework-Specific Rules

1. **Logging** — Logging high-value secrets in plaintext IS a vulnerability. Logging URLs is safe. Logging request headers is dangerous (likely contain credentials). Logging non-PII data is NOT a vulnerability.
2. **UUIDs** — UUIDs are unguessable; vulnerabilities requiring UUID guessing are invalid.
3. **Environment Variables & CLI Flags** — Trusted values. Attacks requiring control of env vars are invalid.
4. **React/Angular XSS** — These frameworks are secure against XSS by default. Only report XSS if using `dangerouslySetInnerHTML`, `bypassSecurityTrustHtml`, or similar unsafe methods.
5. **GitHub Actions** — Most workflow vulnerabilities are not exploitable in practice. Require concrete attack path with untrusted input.
6. **Client-Side Auth** — Lack of permission checking in client-side JS/TS is NOT a vulnerability. Server-side handles auth.
7. **Jupyter Notebooks** — Most notebook vulnerabilities are not exploitable. Require concrete attack path with untrusted input.
8. **Shell Scripts** — Command injection in shell scripts is generally not exploitable (no untrusted user input). Require specific untrusted input path.
9. **Subtle Web Vulns** — Tabnabbing, XS-Leaks, prototype pollution, open redirects are invalid unless extremely high confidence.
10. **Audit Logs** — Missing or modifiable audit logs is NOT a vulnerability.
11. **MEDIUM Findings** — Only include if obvious and concrete.
12. **Path Traversal in Client Code** — `../` attacks are not a problem in client-side JS.
