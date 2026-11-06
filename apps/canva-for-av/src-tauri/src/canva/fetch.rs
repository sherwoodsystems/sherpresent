use regex::Regex;
use reqwest::Client;

/// Extract and parse the bootstrap JSON from HTML content.
fn extract_bootstrap(html: &str) -> Result<serde_json::Value, String> {
    // Try JSON.parse('...') format first (most common)
    let re = Regex::new(r"window\['bootstrap'\]\s*=\s*JSON\.parse\('((?:[^'\\]|\\.)*)'\)")
        .map_err(|e| format!("Regex error: {}", e))?;

    if let Some(captures) = re.captures(html) {
        let json_str = &captures[1];
        // The string is JS-escaped inside single quotes: unescape it
        let unescaped = unescape_js_string(json_str);
        return serde_json::from_str(&unescaped)
            .map_err(|e| format!("Failed to parse bootstrap JSON: {}", e));
    }

    // Fallback: try direct assignment `window['bootstrap'] = {...};`
    let re2 = Regex::new(r"window\['bootstrap'\]\s*=\s*(\{[\s\S]*?\});")
        .map_err(|e| format!("Regex error: {}", e))?;

    if let Some(captures) = re2.captures(html) {
        let json_str = &captures[1];
        return serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse bootstrap JSON: {}", e));
    }

    Err("Could not find window['bootstrap'] in page HTML".to_string())
}

/// Unescape a JavaScript string that was inside single quotes.
/// Handles \\, \', \", \n, \r, \t, \uXXXX, and passes through unknown escapes.
fn unescape_js_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => result.push('\\'),
                Some('\'') => result.push('\''),
                Some('"') => result.push('"'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('/') => result.push('/'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(code) {
                            result.push(ch);
                        }
                    }
                }
                Some(other) => {
                    // Pass through unknown escapes
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Fetch the raw HTML and return both the HTML and parsed bootstrap.
pub async fn fetch_canva_page(
    client: &Client,
    url: &str,
) -> Result<(String, serde_json::Value), String> {
    let response = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch URL: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let html = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let bootstrap = extract_bootstrap(&html)?;
    Ok((html, bootstrap))
}
