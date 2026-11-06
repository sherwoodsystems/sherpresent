use crate::models::{FontRef, Presentation, Slide, SlideElement};

// Obfuscated key constants — single place to update when Canva changes them
const KEY_PAGES: &str = "Bj";
const KEY_PAGE_CONTENT: &str = "A";
const KEY_ELEMENTS: &str = "C";
const KEY_ELEMENT_LIST: &str = "A";
const KEY_ELEMENT_TYPE: &str = "t";
const TYPE_TEXT: &str = "K";
const TYPE_IMAGE: &str = "I";
const TYPE_SHAPE_U: &str = "U";
const TYPE_SHAPE_J: &str = "J";

/// Parse the bootstrap JSON into a structured Presentation.
pub fn parse_presentation(
    bootstrap: &serde_json::Value,
    html: &str,
    url: &str,
) -> Result<Presentation, String> {
    let id = extract_id_from_url(url).unwrap_or_else(|| "unknown".to_string());
    let title = extract_title_from_html(html).unwrap_or_else(|| "Untitled Presentation".to_string());

    // Debug: dump top-level bootstrap keys
    if let Some(obj) = bootstrap.as_object() {
        let keys: Vec<&String> = obj.keys().collect();
        log::info!("[PARSER] Bootstrap top-level keys: {:?}", keys);
        for key in &keys {
            let val = &obj[*key];
            match val {
                serde_json::Value::Object(inner) => {
                    let inner_keys: Vec<&String> = inner.keys().collect();
                    log::info!("[PARSER]   bootstrap.{} = Object with keys: {:?}", key, inner_keys);
                }
                serde_json::Value::Array(arr) => {
                    log::info!("[PARSER]   bootstrap.{} = Array[{}]", key, arr.len());
                }
                serde_json::Value::String(s) => {
                    log::info!("[PARSER]   bootstrap.{} = String(len={})", key, s.len());
                }
                other => {
                    log::info!("[PARSER]   bootstrap.{} = {:?}", key, other);
                }
            }
        }
    } else {
        log::warn!("[PARSER] Bootstrap is not an object! Type: {}", json_type_name(bootstrap));
    }

    let pages = find_pages(bootstrap);
    log::info!("[PARSER] Found {} pages/slides", pages.len());

    let slides = parse_slides(&pages);
    let fonts = extract_fonts(bootstrap);
    log::info!("[PARSER] Parsed {} fonts", fonts.len());

    let total_elements: usize = slides.iter().map(|s| s.elements.len()).sum();
    log::info!("[PARSER] Total elements across all slides: {}", total_elements);

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    Ok(Presentation {
        id,
        title,
        url: url.to_string(),
        width: 1920,
        height: 1080,
        slides,
        fonts,
        imported_at: now,
    })
}

fn json_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Extract the design ID from a Canva URL.
fn extract_id_from_url(url: &str) -> Option<String> {
    let re = regex::Regex::new(r"/design/([^/]+)").ok()?;
    re.captures(url).map(|c| c[1].to_string())
}

fn extract_title_from_html(html: &str) -> Option<String> {
    let re = regex::Regex::new(r"<title>([^<]+)</title>").ok()?;
    let title = re.captures(html).map(|c| c[1].to_string())?;
    Some(
        title
            .trim_end_matches(" - Canva")
            .trim_end_matches(" | Canva")
            .to_string(),
    )
}

/// Navigate the bootstrap JSON to find the pages array.
fn find_pages(bootstrap: &serde_json::Value) -> Vec<serde_json::Value> {
    // Debug: trace through the expected path step by step
    let page = bootstrap.get("page");
    log::info!("[PARSER] bootstrap.page exists: {}", page.is_some());

    if let Some(page) = page {
        if let Some(obj) = page.as_object() {
            let keys: Vec<&String> = obj.keys().collect();
            log::info!("[PARSER] bootstrap.page keys: {:?}", keys);
        }

        let bj = page.get(KEY_PAGES);
        log::info!("[PARSER] bootstrap.page.{} exists: {}", KEY_PAGES, bj.is_some());

        if let Some(bj) = bj {
            if let Some(obj) = bj.as_object() {
                let keys: Vec<&String> = obj.keys().collect();
                log::info!("[PARSER] bootstrap.page.{} keys: {:?}", KEY_PAGES, keys);
            } else if let Some(arr) = bj.as_array() {
                log::info!("[PARSER] bootstrap.page.{} is Array[{}]", KEY_PAGES, arr.len());
            } else {
                log::info!("[PARSER] bootstrap.page.{} is {}", KEY_PAGES, json_type_name(bj));
            }

            let a = bj.get(KEY_PAGE_CONTENT);
            log::info!("[PARSER] bootstrap.page.{}.{} exists: {}", KEY_PAGES, KEY_PAGE_CONTENT, a.is_some());

            if let Some(a) = a {
                if let Some(obj) = a.as_object() {
                    let keys: Vec<&String> = obj.keys().collect();
                    log::info!("[PARSER] bootstrap.page.{}.{} keys: {:?}", KEY_PAGES, KEY_PAGE_CONTENT, keys);
                } else if let Some(arr) = a.as_array() {
                    log::info!("[PARSER] bootstrap.page.{}.{} is Array[{}]", KEY_PAGES, KEY_PAGE_CONTENT, arr.len());
                }

                let c = a.get(KEY_ELEMENTS);
                log::info!("[PARSER] bootstrap.page.{}.{}.{} exists: {}", KEY_PAGES, KEY_PAGE_CONTENT, KEY_ELEMENTS, c.is_some());

                if let Some(c) = c {
                    if let Some(obj) = c.as_object() {
                        let keys: Vec<&String> = obj.keys().collect();
                        log::info!("[PARSER] ...{}.{} keys: {:?}", KEY_ELEMENTS, "", keys);
                    } else if let Some(arr) = c.as_array() {
                        log::info!("[PARSER] ...{} is Array[{}]", KEY_ELEMENTS, arr.len());
                    }

                    let a2 = c.get(KEY_ELEMENT_LIST);
                    log::info!("[PARSER] ...{}.{} exists: {}", KEY_ELEMENTS, KEY_ELEMENT_LIST, a2.is_some());

                    if let Some(a2) = a2 {
                        if let Some(arr) = a2.as_array() {
                            log::info!("[PARSER] Found pages array at expected path! Length: {}", arr.len());
                            return arr.clone();
                        } else {
                            log::info!("[PARSER] ...{}.{} is {} (expected array)", KEY_ELEMENTS, KEY_ELEMENT_LIST, json_type_name(a2));
                        }
                    }
                }
            }
        }
    }

    // Fallback: search for arrays that look like slide collections
    log::warn!("[PARSER] Expected path failed, doing recursive search...");

    if let Some(page) = bootstrap.get("page") {
        if let Some(pages) = search_for_pages(page, 0, "page") {
            return pages;
        }
    }

    // Also try searching the whole bootstrap
    if let Some(pages) = search_for_pages(bootstrap, 0, "root") {
        return pages;
    }

    log::warn!("[PARSER] No pages found anywhere in bootstrap data");
    Vec::new()
}

/// Recursively search for an array that looks like a collection of slides.
fn search_for_pages(value: &serde_json::Value, depth: usize, path: &str) -> Option<Vec<serde_json::Value>> {
    if depth > 6 {
        return None;
    }

    if let Some(arr) = value.as_array() {
        if arr.len() > 1 && arr.iter().any(|item| item.is_object()) {
            log::info!("[PARSER] search_for_pages: found candidate array at '{}' with {} items", path, arr.len());
            // Log what the first item looks like
            if let Some(first) = arr.first() {
                if let Some(obj) = first.as_object() {
                    let keys: Vec<&String> = obj.keys().collect();
                    log::info!("[PARSER]   first item keys: {:?}", keys);
                }
            }
            return Some(arr.clone());
        }
    }

    if let Some(obj) = value.as_object() {
        for (key, val) in obj {
            let child_path = format!("{}.{}", path, key);
            if let Some(result) = search_for_pages(val, depth + 1, &child_path) {
                return Some(result);
            }
        }
    }

    None
}

fn parse_slides(pages: &[serde_json::Value]) -> Vec<Slide> {
    pages
        .iter()
        .enumerate()
        .map(|(i, page)| {
            // Debug: dump each page's structure
            if let Some(obj) = page.as_object() {
                let keys: Vec<&String> = obj.keys().collect();
                log::info!("[PARSER] Slide {} top-level keys: {:?}", i, keys);

                // Dump first 2 levels of structure for the first 2 slides
                if i < 2 {
                    for (key, val) in obj {
                        match val {
                            serde_json::Value::Object(inner) => {
                                let inner_keys: Vec<&String> = inner.keys().collect();
                                log::info!("[PARSER]   slide[{}].{} = Object keys: {:?}", i, key, inner_keys);
                                // Go one more level
                                for (k2, v2) in inner {
                                    match v2 {
                                        serde_json::Value::Object(inner2) => {
                                            let keys2: Vec<&String> = inner2.keys().collect();
                                            log::info!("[PARSER]     .{}.{} = Object keys: {:?}", key, k2, keys2);
                                        }
                                        serde_json::Value::Array(arr) => {
                                            log::info!("[PARSER]     .{}.{} = Array[{}]", key, k2, arr.len());
                                            // If it's an array of objects, show first item's keys
                                            if let Some(first) = arr.first() {
                                                if let Some(fobj) = first.as_object() {
                                                    let fkeys: Vec<&String> = fobj.keys().collect();
                                                    log::info!("[PARSER]       first item keys: {:?}", fkeys);
                                                }
                                            }
                                        }
                                        serde_json::Value::String(s) => {
                                            log::info!("[PARSER]     .{}.{} = String(len={}): {:?}", key, k2, s.len(), &s[..s.len().min(100)]);
                                        }
                                        serde_json::Value::Number(n) => {
                                            log::info!("[PARSER]     .{}.{} = {}", key, k2, n);
                                        }
                                        other => {
                                            log::info!("[PARSER]     .{}.{} = {}", key, k2, json_type_name(other));
                                        }
                                    }
                                }
                            }
                            serde_json::Value::Array(arr) => {
                                log::info!("[PARSER]   slide[{}].{} = Array[{}]", i, key, arr.len());
                                if let Some(first) = arr.first() {
                                    if let Some(fobj) = first.as_object() {
                                        let fkeys: Vec<&String> = fobj.keys().collect();
                                        log::info!("[PARSER]     first item keys: {:?}", fkeys);
                                    }
                                }
                            }
                            serde_json::Value::String(s) => {
                                log::info!("[PARSER]   slide[{}].{} = String(len={}): {:?}", i, key, s.len(), &s[..s.len().min(100)]);
                            }
                            serde_json::Value::Number(n) => {
                                log::info!("[PARSER]   slide[{}].{} = {}", i, key, n);
                            }
                            other => {
                                log::info!("[PARSER]   slide[{}].{} = {}", i, key, json_type_name(other));
                            }
                        }
                    }
                }
            } else {
                log::info!("[PARSER] Slide {} is not an object, it's a {}", i, json_type_name(page));
            }

            let elements = parse_elements(page, i);
            let thumbnail_url = extract_thumbnail(page);

            log::info!("[PARSER] Slide {} => {} elements, thumbnail: {:?}", i, elements.len(),
                thumbnail_url.as_ref().map(|u| &u[..u.len().min(80)]));

            Slide {
                index: i,
                elements,
                thumbnail_url,
                thumbnail_local: None,
            }
        })
        .collect()
}

fn parse_elements(page: &serde_json::Value, slide_idx: usize) -> Vec<SlideElement> {
    let mut elements = Vec::new();

    // Try to find element list within the page via known paths
    let paths_tried = vec![
        (format!("{}.{}.{}", KEY_PAGE_CONTENT, KEY_ELEMENTS, KEY_ELEMENT_LIST),
         page.get(KEY_PAGE_CONTENT)
            .and_then(|a| a.get(KEY_ELEMENTS))
            .and_then(|c| c.get(KEY_ELEMENT_LIST))
            .and_then(|a| a.as_array())),
        (format!("{}.{}", KEY_ELEMENTS, KEY_ELEMENT_LIST),
         page.get(KEY_ELEMENTS)
            .and_then(|c| c.get(KEY_ELEMENT_LIST))
            .and_then(|a| a.as_array())),
        (KEY_ELEMENT_LIST.to_string(),
         page.get(KEY_ELEMENT_LIST).and_then(|a| a.as_array())),
    ];

    for (path_name, source) in &paths_tried {
        if let Some(arr) = source {
            log::info!("[PARSER] Slide {}: found element array at path '{}' with {} items", slide_idx, path_name, arr.len());

            // Log type distribution
            let mut type_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            let mut no_type_count = 0usize;
            for elem in *arr {
                if let Some(t) = elem.get(KEY_ELEMENT_TYPE).and_then(|v| v.as_str()) {
                    *type_counts.entry(t.to_string()).or_insert(0) += 1;
                } else {
                    no_type_count += 1;
                    // Log keys of elements without type
                    if no_type_count <= 3 {
                        if let Some(obj) = elem.as_object() {
                            let keys: Vec<&String> = obj.keys().collect();
                            log::info!("[PARSER]   element without '{}' field, keys: {:?}", KEY_ELEMENT_TYPE, keys);
                        }
                    }
                }
            }
            log::info!("[PARSER] Slide {}: element type distribution: {:?}, no-type: {}", slide_idx, type_counts, no_type_count);

            // Try parsing with current type constants
            for elem in *arr {
                if let Some(parsed) = parse_single_element(elem, slide_idx) {
                    elements.push(parsed);
                }
            }

            if !elements.is_empty() {
                return elements;
            }

            // If no elements parsed, dump first few raw elements for debugging
            log::warn!("[PARSER] Slide {}: found {} raw elements but parsed 0! Dumping first 3:", slide_idx, arr.len());
            for (ei, elem) in arr.iter().take(3).enumerate() {
                let truncated = truncate_json(elem, 500);
                log::warn!("[PARSER]   raw element[{}]: {}", ei, truncated);
            }
        }
    }

    // No elements found via known paths
    log::warn!("[PARSER] Slide {}: no element arrays found at known paths, trying recursive search", slide_idx);

    // Try to find any arrays within the page that contain element-like objects
    if let Some(obj) = page.as_object() {
        for (key, val) in obj {
            if let Some(arr) = val.as_array() {
                if !arr.is_empty() {
                    let has_objects = arr.iter().any(|item| item.is_object());
                    if has_objects {
                        log::info!("[PARSER] Slide {}: checking array at key '{}' ({} items)", slide_idx, key, arr.len());
                        if let Some(first) = arr.first() {
                            if let Some(fobj) = first.as_object() {
                                let fkeys: Vec<&String> = fobj.keys().collect();
                                log::info!("[PARSER]   first item keys: {:?}", fkeys);
                            }
                        }
                    }
                }
            }
        }
    }

    collect_elements_recursive(page, &mut elements, 0, slide_idx);
    if elements.is_empty() {
        log::warn!("[PARSER] Slide {}: recursive search also found 0 elements", slide_idx);
    }

    elements
}

fn parse_single_element(elem: &serde_json::Value, slide_idx: usize) -> Option<SlideElement> {
    let elem_type = elem.get(KEY_ELEMENT_TYPE).and_then(|v| v.as_str());

    let elem_type = match elem_type {
        Some(t) => t,
        None => {
            // Try alternate type field names
            let alt_type = elem.get("type").and_then(|v| v.as_str())
                .or_else(|| elem.get("T").and_then(|v| v.as_str()))
                .or_else(|| elem.get("kind").and_then(|v| v.as_str()));
            if let Some(t) = alt_type {
                log::info!("[PARSER] Slide {}: found type via alternate field: {}", slide_idx, t);
                t
            } else {
                return None;
            }
        }
    };

    let x = get_f64(elem, "x").or_else(|| get_f64(elem, "X")).unwrap_or(0.0);
    let y = get_f64(elem, "y").or_else(|| get_f64(elem, "Y")).unwrap_or(0.0);
    let width = get_f64(elem, "width").or_else(|| get_f64(elem, "w")).or_else(|| get_f64(elem, "W")).unwrap_or(0.0);
    let height = get_f64(elem, "height").or_else(|| get_f64(elem, "h")).or_else(|| get_f64(elem, "H")).unwrap_or(0.0);
    let rotation = get_f64(elem, "rotation").or_else(|| get_f64(elem, "r")).unwrap_or(0.0);

    match elem_type {
        TYPE_TEXT => {
            let content = elem
                .get("v")
                .or_else(|| elem.get("text"))
                .or_else(|| elem.get("content"))
                .or_else(|| elem.get("V"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            Some(SlideElement::Text {
                content,
                x,
                y,
                width,
                height,
                font_family: elem.get("fontFamily").or_else(|| elem.get("ff")).and_then(|v| v.as_str()).map(|s| s.to_string()),
                font_size: elem.get("fontSize").or_else(|| elem.get("fs")).and_then(|v| v.as_f64()),
                color: elem.get("color").or_else(|| elem.get("c")).and_then(|v| v.as_str()).map(|s| s.to_string()),
                bold: elem.get("bold").or_else(|| elem.get("b")).and_then(|v| v.as_bool()).unwrap_or(false),
                italic: elem.get("italic").or_else(|| elem.get("i")).and_then(|v| v.as_bool()).unwrap_or(false),
                rotation,
            })
        }
        TYPE_IMAGE => {
            let asset_url = elem
                .get("url")
                .or_else(|| elem.get("src"))
                .or_else(|| elem.get("u"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            Some(SlideElement::Image {
                x,
                y,
                width,
                height,
                asset_url,
                local_path: None,
                rotation,
            })
        }
        TYPE_SHAPE_U | TYPE_SHAPE_J => Some(SlideElement::Shape {
            x,
            y,
            width,
            height,
            svg_path: elem.get("path").or_else(|| elem.get("d")).and_then(|v| v.as_str()).map(|s| s.to_string()),
            fill_color: elem.get("fill").or_else(|| elem.get("f")).and_then(|v| v.as_str()).map(|s| s.to_string()),
            stroke_color: elem.get("stroke").or_else(|| elem.get("s")).and_then(|v| v.as_str()).map(|s| s.to_string()),
            rotation,
        }),
        _ => {
            // Log unknown types
            log::debug!("[PARSER] Slide {}: skipping element with unknown type '{}'", slide_idx, elem_type);
            None
        }
    }
}

/// Recursively look for elements in deeply nested structures.
fn collect_elements_recursive(
    value: &serde_json::Value,
    elements: &mut Vec<SlideElement>,
    depth: usize,
    slide_idx: usize,
) {
    if depth > 8 || elements.len() > 100 {
        return;
    }

    if let Some(obj) = value.as_object() {
        if obj.contains_key(KEY_ELEMENT_TYPE) {
            if let Some(elem) = parse_single_element(value, slide_idx) {
                elements.push(elem);
                return;
            }
        }

        for (_key, val) in obj {
            collect_elements_recursive(val, elements, depth + 1, slide_idx);
        }
    }

    if let Some(arr) = value.as_array() {
        for val in arr {
            collect_elements_recursive(val, elements, depth + 1, slide_idx);
        }
    }
}

fn extract_thumbnail(page: &serde_json::Value) -> Option<String> {
    page.get("thumbnail")
        .or_else(|| page.get("thumb"))
        .or_else(|| page.get("image"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn extract_fonts(bootstrap: &serde_json::Value) -> Vec<FontRef> {
    let mut fonts = Vec::new();

    let font_sources = [
        ("bootstrap.fonts", bootstrap.get("fonts")),
        ("bootstrap.page.fonts", bootstrap.get("page").and_then(|p| p.get("fonts"))),
        ("bootstrap.assets.fonts", bootstrap.get("assets").and_then(|a| a.get("fonts"))),
    ];

    for (name, source) in &font_sources {
        if let Some(source) = source {
            log::info!("[PARSER] Found font source at '{}': {}", name, json_type_name(source));

            if let Some(obj) = source.as_object() {
                for (family, info) in obj {
                    let woff2_url = info
                        .get("woff2")
                        .or_else(|| info.get("url"))
                        .or_else(|| info.get("src"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    fonts.push(FontRef {
                        family: family.clone(),
                        woff2_url,
                        local_path: None,
                    });
                }
            } else if let Some(arr) = source.as_array() {
                for item in arr {
                    if let Some(family) = item.get("family").or_else(|| item.get("name")).and_then(|v| v.as_str()) {
                        let woff2_url = item
                            .get("woff2")
                            .or_else(|| item.get("url"))
                            .or_else(|| item.get("src"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        fonts.push(FontRef {
                            family: family.to_string(),
                            woff2_url,
                            local_path: None,
                        });
                    }
                }
            }
        }
    }

    fonts
}

fn get_f64(value: &serde_json::Value, key: &str) -> Option<f64> {
    value.get(key).and_then(|v| v.as_f64())
}

/// Truncate a JSON value to a string of max length for logging.
fn truncate_json(value: &serde_json::Value, max_len: usize) -> String {
    let s = serde_json::to_string(value).unwrap_or_else(|_| format!("{:?}", value));
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s
    }
}
