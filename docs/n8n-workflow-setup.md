# Omni Downloader n8n Workflow Setup

This workflow uses n8n as the orchestration layer and the headless `omni-downloader` worker as a one-shot command-line tool inside the same container.

Importable workflow export: `docs/omni-downloader-workflow.n8n.json`

## Container prerequisites

The provided Docker image already includes:

- `n8n`
- `omni-downloader`
- `yt-dlp`
- `ffmpeg`
- `cheerio`

The Compose file also sets `NODE_FUNCTION_ALLOW_EXTERNAL=cheerio`, which the Code node needs for `require('cheerio')`.

## Recommended workflow shape

1. `Email Trigger (IMAP)`
2. `Code` node: extract sender and URLs from the email body
3. `Split Out` node: process one URL per item
4. `HTTP Request` node: fetch raw HTML
5. `Code` node: Cheerio context extraction
6. `LLM` node: select the single best media URL and return strict JSON
7. `Code` node: validate and normalize the LLM response
8. `Execute Command` node: invoke `omni-downloader download`

## Node configuration

### 1. Email Trigger (IMAP)

- Operation: read unread emails
- Download attachments: off
- Simplify: on

### 2. Code node: Extract Sender And URLs

Use JavaScript mode:

```javascript
const email = $input.item.json;
const senderRaw = email.from?.text || email.from || '';
const senderName = senderRaw
  .replace(/<[^>]+>/g, '')
  .replace(/\"/g, '')
  .trim() || 'unknown_sender';

const body = [
  email.text,
  email.html,
  email.snippet,
].filter(Boolean).join('\n');

const urlPattern = /https?:\/\/[^\s<>()"']+/g;
const urls = [...new Set((body.match(urlPattern) || []).map((url) => url.replace(/[),.;]+$/, '')))];

return urls.map((url, index) => ({
  json: {
    sender_name: senderName,
    url,
    order: index + 1,
  },
}));
```

### 3. Split Out

- Field to split out: not needed if the previous Code node already emits one item per URL.
- You can skip this node if you use the Code snippet above exactly as written.

### 4. HTTP Request

- Method: `GET`
- URL: `={{ $json.url }}`
- Response format: `String`
- Put output in field: `html`
- Options: follow redirects on

### 5. Code node: Cheerio Context Extraction

Use JavaScript mode:

```javascript
const html = $input.item.json.html;
const pageUrl = $input.item.json.url;
const { URL } = require('url');
const $ = require('cheerio').load(html);

const pageTitle = $('title').first().text() || $('h1').first().text();
const pageDescription = $('meta[name="description"]').attr('content') || $('meta[property="og:description"]').attr('content') || '';
const canonicalUrl = $('link[rel="canonical"]').attr('href') || pageUrl;

const toAbsolute = (value) => {
  try {
    return new URL(value, pageUrl).toString();
  } catch {
    return value;
  }
};

let extractedData = {
  source_url: pageUrl,
  canonical_url: canonicalUrl,
  article_title: pageTitle.trim(),
  article_description: pageDescription.trim(),
  json_ld_scripts: [],
  embedded_media: []
};

$('script[type="application/ld+json"]').each((i, el) => {
  const content = $(el).html();
  if (!content) {
    return;
  }

  if (!content.includes('VideoObject') && !content.includes('contentUrl') && !content.includes('embedUrl')) {
    return;
  }

  try {
    extractedData.json_ld_scripts.push(JSON.parse(content));
  } catch {
    extractedData.json_ld_scripts.push({ raw_json_ld: content.substring(0, 4000) });
  }
});

$('iframe, video').each((i, el) => {
  let src = $(el).attr('src') || $(el).attr('data-src');
  if (!src && el.tagName === 'video') {
    src = $(el).find('source').first().attr('src');
  }
  if (src) {
    let surroundingText = $(el).parent().text() || $(el).parent().parent().text();
    surroundingText = surroundingText.replace(/\s+/g, ' ').trim().substring(0, 300);
    extractedData.embedded_media.push({
      tag: el.tagName,
      url: toAbsolute(src),
      surrounding_text: surroundingText,
    });
  }
});

return {
  json: {
    ...$input.item.json,
    extracted_context: extractedData,
  },
};
```

### 6. LLM node

Use your preferred chat model node. Pass the extracted context and require strict JSON output.

Recommended system prompt:

```text
You identify the single best downloadable media URL for newsroom ingest.
Prefer the main article video, not ads, analytics, thumbnails, subtitles, or unrelated embeds.
Return JSON only with this schema:
{"selected_url":"string","confidence":0.0,"reason":"string"}
If no reliable media URL exists, return:
{"selected_url":"","confidence":0.0,"reason":"no reliable media url found"}
```

Recommended user prompt:

```text
Select the best direct or player media URL from this article context.

{{ JSON.stringify($json.extracted_context) }}
```

### 7. Code node: Validate LLM Output

Use JavaScript mode:

```javascript
const raw = $input.item.json.text || $input.item.json.response || $input.item.json.output || '';

let parsed;
try {
  parsed = typeof raw === 'string' ? JSON.parse(raw) : raw;
} catch {
  throw new Error(`LLM response was not valid JSON: ${raw}`);
}

if (!parsed.selected_url || typeof parsed.selected_url !== 'string') {
  throw new Error(`LLM did not return a usable selected_url: ${JSON.stringify(parsed)}`);
}

return {
  json: {
    ...$input.item.json,
    selected_url: parsed.selected_url,
    llm_confidence: parsed.confidence ?? 0,
    llm_reason: parsed.reason || '',
    target_filename: `${String($json.order).padStart(4, '0')}_${$json.sender_name}`,
  },
};
```

### 8. Execute Command

- Command:

```bash
omni-downloader download --json --url "{{$json.selected_url}}" --target-filename "{{$json.target_filename}}" --profile-name "Dalet XDCAM 50Mbps"
```

The worker prints JSON on success or failure, which makes downstream error handling simpler.

## Optional MCP mode

If you want to experiment with stdio MCP later, the same binary supports:

```bash
omni-downloader --mcp
```

For the MVP, the one-shot CLI is the simpler and more reliable n8n integration point.