# The captured web-scraper contract

The capture comes from the reference service, never from this crate. A lock regenerated from the
rewrite cannot fail for the reason `--check-lock` exists.

| path | what it is |
|---|---|
| `webscraper/openapi.lock.json` | the OpenAPI 3.1 document the reference serves (6 paths, 6 operations) |
| `webscraper/manifest.json` | provenance: capture time, source commit, operation and schema counts; `dropped` is empty |
| `fixtures/` | 12 synthetic pages, served by any static file server |
| `cases.txt` | the capture cases: `name\|METHOD\|path\|body`, `{BASE}` = the fixture base URL |
| `webscraper/golden/` | one request/response pair per case, 19 of them |

Regenerate, never hand-edit:

```sh
# the reference service, locally, with auth off and only the fixture host allowed past its SSRF guard
PYTHONPATH=src ENABLE_AUTH=false ENVIRONMENT=development ENABLE_DOCS=true PORT=19119 \
  SSRF_ALLOWED_HOSTS='["127.0.0.1"]' .venv/bin/uvicorn main:app --host 127.0.0.1 --port 19119
python3 -m http.server 18900 --bind 127.0.0.1 --directory contract/fixtures

contract/capture.sh --live http://127.0.0.1:19119 --source-commit <sha>
contract/capture-goldens.sh --live http://127.0.0.1:19119 --fixtures http://127.0.0.1:18900
```

No service address is stored in the tree; URLs are supplied at capture time.

## What the reference actually returns

Measured from the goldens, because it decides how much of this is portable:

- **`text_content` is Markdown, not plain text.** Headings (`#`, `##`), lists (`- item`), tables
  (`| Name | Value |` with a `|---|---|` separator row), and links rendered as
  `[text](absolute-url)` — relative hrefs are resolved against the page URL.
- **`markdown_content`** is the same rendering applied to the whole page rather than the extracted
  body.
- **`links[]`** is `{text, url, rel, type, score}`; `type` is `internal`/`external` and `score` is the
  reference's own link-scoring heuristic (e.g. `0.98`, `1.0`).
- **`images[]`** is `{src, alt, title, width, height, srcset, content_base64, mime_type, byte_size}`
  with `src` absolutised.
- **`extracted_data`** is the CSS-schema records (`baseSelector` + per-field `selector`).
- The SSRF guard refuses loopback/private/link-local targets with a `400` `HTTP_400` error, which is
  captured as the `blocked_address` golden.

## Deviations (declared, not silently ignored)

| capability | state in `apps/research` | why |
|---|---|---|
| `scraper_type` = `playwright` / `scrapy` / `curl_cffi` | refused **by name** (`422`, the field is named in the error) | these need a JS engine, a crawl engine and a Chrome TLS/JA3 stack; none has a Rust equivalent in this workspace |
| `javascript_enabled`, `stealth`, `virtual_scroll`, `screenshot`, `wait_for_selector`, `wait_until` | refused by name for the same reason | every one of them forces the browser backend |
| `text_content` rendering | **the BeautifulSoup boilerplate-strip baseline is ported verbatim; trafilatura is not** | trafilatura has no Rust counterpart. The baseline is deterministic and portable: remove `script/style/noscript/nav/header/footer/aside/form`, drop consent/dialog containers by class/id/role/aria, linearise tables, normalise whitespace. The reference itself falls back to this baseline whenever the primary extractor retains less than `MIN_RATIO` of it |

The parity test compares each text field exactly where the baseline won in the reference, and inside
a declared similarity bound elsewhere — the bound and the reason live with the test, never as a
silent pass.
