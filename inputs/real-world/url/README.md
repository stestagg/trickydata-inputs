# URLs

URL handling has several overlapping standards and layers:

- RFC 3986 defines generic URI syntax and syntax-based normalization.
- The WHATWG URL Standard defines the parser used by browsers and many modern
  application runtimes, including special handling for HTTP-family schemes.
- RFC 5890 and UTS #46 define internationalized domain representations and the
  processing commonly applied to URL hosts.
- HTTP servers, reverse proxies, routers, filesystems, and form decoders may
  each decode or normalize a URL component again.

These layers are deliberately distinguished in each input description. A
string can be a valid RFC URI-reference but fail WHATWG parsing, or different
parsers can assign different hosts and paths to the same bytes. Query strings
are especially application-defined: generic URI syntax does not define them as
key-value maps.

The `invalid/` directory contains strings tagged `invalid-url`. Most are hard
WHATWG URL parse failures. Files tagged `repair` are invalid RFC 3986 URI text
or contain a WHATWG validation error but are intentionally accepted and
rewritten by browser-compatible parsers; consumers that validate with one
parser and fetch with another need to test both the source and repaired forms.

Pair hints use the following meanings:

- `equal` means the cited standard defines equivalent representations.
- `not-equal` means normalization must preserve the distinction.
- `tricky` means equality or component boundaries depend on parser dialect,
  protocol layer, or application policy.

All network-shaped examples use reserved `.example`, `.invalid`, `.test`, or
TEST-NET addresses. They are data for parsers and must not be treated as
instructions to make network requests.

Primary standards:

- https://www.rfc-editor.org/rfc/rfc3986.html
- https://url.spec.whatwg.org/
- https://www.rfc-editor.org/rfc/rfc5890.html
- https://www.unicode.org/reports/tr46/
- https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#application/x-www-form-urlencoded-encoding-algorithm

Published vulnerabilities represented by inert regression cases:

- Apache HTTP Server CVE-2021-41773 and CVE-2021-42013:
  https://httpd.apache.org/security/vulnerabilities_24.html#CVE-2021-41773
  https://httpd.apache.org/security/vulnerabilities_24.html#CVE-2021-42013
- curl CVE-2022-27780:
  https://curl.se/docs/CVE-2022-27780.html
- CPython CVE-2023-24329:
  https://github.com/python/cpython/issues/102153
- CPython CVE-2019-9740:
  https://github.com/python/cpython/issues/80457
- CPython CVE-2021-29921:
  https://github.com/python/cpython/issues/80565
- Node.js CVE-2018-12123:
  https://nodejs.org/en/blog/vulnerability/november-2018-security-releases
