# Task: Apply False Positive Filtering to Security Findings

You have completed an initial security scan that produced the following raw findings. Apply the false positive filtering rules to determine which findings should be kept and which should be removed.

## Raw Findings

1. **SQL Injection in `api/users.py:45`** — User input concatenated directly into SQL query: `cursor.execute(f"SELECT * FROM users WHERE id = {user_id}")`. Confidence: 9/10.

2. **Missing Rate Limiting on `/api/login`** — The login endpoint has no rate limiting, allowing brute force attacks. Confidence: 7/10.

3. **Denial of Service via unbounded file upload in `api/upload.py:23`** — No file size limit on uploads could exhaust server resources. Confidence: 6/10.

4. **Hardcoded API key in `tests/test_integration.py:12`** — Test file contains `API_KEY = "test-key-12345"`. Confidence: 5/10.

5. **Command Injection in `api/export.py:67`** — User-controlled filename passed to `subprocess.run(f"convert {filename} output.pdf", shell=True)`. Confidence: 9/10.

6. **SSRF in `frontend/src/components/Preview.tsx:34`** — User-controlled URL passed to fetch() in client-side React component. Confidence: 6/10.

7. **Path Traversal via `../` in HTTP request path** — API allows `../` in URL path parameter. Confidence: 4/10.

8. **Deserialization RCE in `api/import.py:89`** — `pickle.loads(request.data)` deserializes untrusted user input. Confidence: 10/10.

9. **Missing CSRF protection on `api/settings.py:15`** — Settings update endpoint missing CSRF token validation. Confidence: 3/10.

10. **Log spoofing in `api/auth.py:56`** — User input logged without sanitization. Confidence: 4/10.

For each finding, state whether it should be KEPT or REMOVED, citing the specific filtering rule that applies. Then produce the final filtered output with only the kept findings in the correct format.
