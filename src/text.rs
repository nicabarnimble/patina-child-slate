pub(crate) fn extract_title(text: &str) -> Option<String> {
    text.lines()
        .find(|line| line.trim_start().starts_with("# "))
        .map(|line| {
            line.trim_start()
                .trim_start_matches("# ")
                .trim()
                .to_string()
        })
}

pub(crate) fn extract_section_paragraph(text: &str, heading: &str) -> Option<String> {
    let mut in_section = false;
    let mut lines = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == heading {
            in_section = true;
            continue;
        }
        if in_section && trimmed.starts_with("## ") {
            break;
        }
        if in_section && !trimmed.is_empty() && !trimmed.starts_with('-') {
            lines.push(trimmed.to_string());
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
}

pub(crate) fn extract_section_items(text: &str, heading: &str) -> Vec<String> {
    let mut in_section = false;
    let mut items = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == heading {
            in_section = true;
            continue;
        }
        if in_section && trimmed.starts_with("## ") {
            break;
        }
        if in_section
            && (trimmed.starts_with("- ") || trimmed.starts_with(|c: char| c.is_ascii_digit()))
        {
            items.push(trimmed.to_string());
        }
    }

    items
}

pub(crate) fn extract_outline(text: &str) -> Vec<String> {
    let mut in_fence = false;
    let mut headings = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence && trimmed.starts_with('#') && trimmed.contains(' ') {
            headings.push(line.to_string());
        }
    }

    headings
}

pub(crate) fn extract_key_files(body: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut in_key_files = false;
    let mut in_fence = false;

    for line in body.lines() {
        if line.starts_with("## Key Files") {
            in_key_files = true;
            continue;
        }
        if in_key_files && !in_fence && line.starts_with("## ") {
            break;
        }
        if in_key_files && line.trim_start().starts_with("```") {
            if in_fence {
                break;
            }
            in_fence = true;
            continue;
        }
        if in_key_files && in_fence {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                if let Some(path) = trimmed.split_whitespace().next() {
                    files.push(path.to_string());
                }
            }
        }
    }

    files
}

pub(crate) fn extract_code_targets(design_text: &str) -> Vec<String> {
    let mut targets = extract_section_items(design_text, "## Direct Code Targets");
    if targets.is_empty() {
        targets = extract_key_files(design_text);
    }
    targets
}

pub(crate) fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;

    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "criterion".to_string()
    } else {
        out
    }
}

pub(crate) fn dedup(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        if !value.trim().is_empty() && !out.contains(&value) {
            out.push(value);
        }
    }
    out
}
