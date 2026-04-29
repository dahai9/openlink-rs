use std::collections::HashMap;
use std::time::Instant;

use regex::Regex;
use serde_json::Value;

use super::{resolve_abs_path, Tool, ToolContext, ToolResult};
use crate::security::sandbox;

pub struct EditTool;

impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Replace a string in a file (exact match)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "path": "string (required) - file path",
            "old_string": "string (required) - text to replace",
            "new_string": "string (required) - replacement text",
            "replace_all": "bool (optional) - replace all occurrences (default false)"
        })
    }

    fn validate(&self, args: &HashMap<String, Value>) -> Result<(), String> {
        if args.get("path").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            return Err("path is required".to_string());
        }
        if !args.contains_key("old_string") {
            return Err("old_string is required".to_string());
        }
        if !args.contains_key("new_string") {
            return Err("new_string is required".to_string());
        }
        Ok(())
    }

    fn execute(&self, ctx: &ToolContext) -> ToolResult {
        let path = ctx.args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let old_str = ctx.args.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
        let new_str = ctx.args.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
        let replace_all = ctx.args.get("replace_all").and_then(|v| v.as_bool()).unwrap_or(false);

        let safe_path = match if path.starts_with('/') || path.starts_with('~') {
            resolve_abs_path(path, &ctx.config.root_dir)
        } else {
            sandbox::safe_path(&ctx.config.root_dir, path)
        } {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let raw_content = match std::fs::read_to_string(&safe_path) {
            Ok(c) => c,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let content = normalize_line_endings(&raw_content);

        let replaced = match replace(&content, old_str, new_str, replace_all) {
            Ok(r) => r,
            Err(e) => return ToolResult::error(e),
        };

        if let Err(e) = std::fs::write(&safe_path, &replaced) {
            return ToolResult::error(e.to_string());
        }

        ToolResult {
            status: "success",
            output: format!("已替换 '{}' → '{}'", old_str, new_str),
            error: String::new(),
            stop_stream: false,
            start_time: Instant::now(),
            end_time: Some(Instant::now()),
        }
    }
}

// ── Constants ───────────────────────────────────────────────────────────────

const SINGLE_CANDIDATE_SIMILARITY_THRESHOLD: f64 = 0.0;
const MULTIPLE_CANDIDATES_SIMILARITY_THRESHOLD: f64 = 0.3;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn normalize_line_endings(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn levenshtein(a: &str, b: &str) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    // Two-row optimization: O(min(n,m)) space
    let (short, long) = if a_bytes.len() <= b_bytes.len() {
        (a_bytes, b_bytes)
    } else {
        (b_bytes, a_bytes)
    };

    let mut prev: Vec<usize> = (0..=short.len()).collect();
    let mut curr = vec![0usize; short.len() + 1];

    for i in 1..=long.len() {
        curr[0] = i;
        for j in 1..=short.len() {
            let cost = if long[i - 1] == short[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[short.len()]
}

// ── Replacer type ───────────────────────────────────────────────────────────

type Replacer = fn(&str, &str) -> Vec<String>;

// ── 1. SimpleReplacer ──────────────────────────────────────────────────────

fn simple_replacer(_content: &str, find: &str) -> Vec<String> {
    vec![find.to_string()]
}

// ── 2. LineTrimmedReplacer ─────────────────────────────────────────────────

fn line_trimmed_replacer(content: &str, find: &str) -> Vec<String> {
    let original_lines: Vec<&str> = content.split('\n').collect();
    let mut search_lines: Vec<&str> = find.split('\n').collect();

    // Pop trailing empty line (find ends with \n)
    if search_lines.last() == Some(&"") {
        search_lines.pop();
    }
    if search_lines.is_empty() {
        return vec![];
    }

    let mut results = Vec::new();

    for i in 0..=original_lines.len().saturating_sub(search_lines.len()) {
        let mut matches = true;
        for j in 0..search_lines.len() {
            if original_lines[i + j].trim() != search_lines[j].trim() {
                matches = false;
                break;
            }
        }
        if !matches {
            continue;
        }

        // Compute byte offsets
        let mut match_start = 0;
        for k in 0..i {
            match_start += original_lines[k].len() + 1; // +1 for \n
        }
        let mut match_end = match_start;
        for k in 0..search_lines.len() {
            match_end += original_lines[i + k].len();
            if k < search_lines.len() - 1 {
                match_end += 1; // \n between lines
            }
        }
        results.push(content[match_start..match_end].to_string());
    }
    results
}

// ── 3. BlockAnchorReplacer ─────────────────────────────────────────────────

fn block_anchor_replacer(content: &str, find: &str) -> Vec<String> {
    let original_lines: Vec<&str> = content.split('\n').collect();
    let mut search_lines: Vec<&str> = find.split('\n').collect();

    if search_lines.len() < 3 {
        return vec![];
    }
    if search_lines.last() == Some(&"") {
        search_lines.pop();
    }
    if search_lines.len() < 3 {
        return vec![];
    }

    let first_line_search = search_lines[0].trim();
    let last_line_search = search_lines[search_lines.len() - 1].trim();
    let search_block_size = search_lines.len();

    let block_to_substring = |start_line: usize, end_line: usize| -> String {
        let mut match_start = 0;
        for k in 0..start_line {
            match_start += original_lines[k].len() + 1;
        }
        let mut match_end = match_start;
        for k in start_line..=end_line {
            match_end += original_lines[k].len();
            if k < end_line {
                match_end += 1;
            }
        }
        content[match_start..match_end].to_string()
    };

    // Collect candidates
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    for i in 0..original_lines.len() {
        if original_lines[i].trim() != first_line_search {
            continue;
        }
        for j in (i + 2)..original_lines.len() {
            if original_lines[j].trim() == last_line_search {
                candidates.push((i, j));
                break;
            }
        }
    }
    if candidates.is_empty() {
        return vec![];
    }

    if candidates.len() == 1 {
        let (start, end) = candidates[0];
        let actual_block_size = end - start + 1;
        let mut lines_to_check = search_block_size.saturating_sub(2);
        lines_to_check = lines_to_check.min(actual_block_size.saturating_sub(2));

        let mut similarity = 0.0;
        if lines_to_check > 0 {
            for j in 1..search_block_size - 1 {
                if j >= actual_block_size - 1 {
                    break;
                }
                let orig_line = original_lines[start + j].trim();
                let srch_line = search_lines[j].trim();
                let max_len = orig_line.len().max(srch_line.len()) as f64;
                if max_len == 0.0 {
                    continue;
                }
                let dist = levenshtein(orig_line, srch_line) as f64;
                similarity += (1.0 - dist / max_len) / lines_to_check as f64;
                if similarity >= SINGLE_CANDIDATE_SIMILARITY_THRESHOLD {
                    break;
                }
            }
        } else {
            similarity = 1.0;
        }

        if similarity >= SINGLE_CANDIDATE_SIMILARITY_THRESHOLD {
            return vec![block_to_substring(start, end)];
        }
        return vec![];
    }

    // Multiple candidates: pick the best
    let mut best_idx = 0usize;
    let mut max_similarity = -1.0f64;

    for (i, &(start, end)) in candidates.iter().enumerate() {
        let actual_block_size = end - start + 1;
        let mut lines_to_check = search_block_size.saturating_sub(2);
        lines_to_check = lines_to_check.min(actual_block_size.saturating_sub(2));

        let mut similarity = 0.0;
        if lines_to_check > 0 {
            for j in 1..search_block_size - 1 {
                if j >= actual_block_size - 1 {
                    break;
                }
                let orig_line = original_lines[start + j].trim();
                let srch_line = search_lines[j].trim();
                let max_len = orig_line.len().max(srch_line.len()) as f64;
                if max_len == 0.0 {
                    continue;
                }
                let dist = levenshtein(orig_line, srch_line) as f64;
                similarity += 1.0 - dist / max_len;
            }
            similarity /= lines_to_check as f64;
        } else {
            similarity = 1.0;
        }

        if similarity > max_similarity {
            max_similarity = similarity;
            best_idx = i;
        }
    }

    if max_similarity >= MULTIPLE_CANDIDATES_SIMILARITY_THRESHOLD {
        let (start, end) = candidates[best_idx];
        return vec![block_to_substring(start, end)];
    }

    vec![]
}

// ── 4. WhitespaceNormalizedReplacer ─────────────────────────────────────────

static WS_REGEXP: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"\s+").unwrap());

fn normalize_whitespace(s: &str) -> String {
    WS_REGEXP.replace_all(s.trim(), " ").to_string()
}

fn whitespace_normalized_replacer(content: &str, find: &str) -> Vec<String> {
    let normalized_find = normalize_whitespace(find);
    let lines: Vec<&str> = content.split('\n').collect();
    let mut results = Vec::new();

    // Phase 1: single-line match
    for line in &lines {
        let normalized_line = normalize_whitespace(line);
        if normalized_line == normalized_find {
            results.push(line.to_string());
        } else if normalized_line.contains(&normalized_find) {
            let words: Vec<&str> = WS_REGEXP.split(find.trim()).collect();
            if !words.is_empty() {
                let quoted: Vec<String> = words.iter().map(|w| regex::escape(w)).collect();
                let pattern = quoted.join(r"\s+");
                if let Ok(re) = Regex::new(&pattern) {
                    if let Some(m) = re.find(line) {
                        results.push(m.as_str().to_string());
                    }
                }
            }
        }
    }

    // Phase 2: multi-line match (only when find contains \n)
    let find_lines: Vec<&str> = find.split('\n').collect();
    if find_lines.len() > 1 {
        for i in 0..=lines.len().saturating_sub(find_lines.len()) {
            let block = lines[i..i + find_lines.len()].join("\n");
            if normalize_whitespace(&block) == normalized_find {
                results.push(block);
            }
        }
    }
    results
}

// ── 5. IndentationFlexibleReplacer ─────────────────────────────────────────

fn remove_indentation(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut min_indent: Option<usize> = None;
    for l in &lines {
        if l.trim().is_empty() {
            continue;
        }
        let n = l.len() - l.trim_start_matches(|c| c == ' ' || c == '\t').len();
        min_indent = Some(match min_indent {
            Some(m) => m.min(n),
            None => n,
        });
    }
    let min_indent = match min_indent {
        Some(n) if n > 0 => n,
        _ => return text.to_string(),
    };
    let out: Vec<String> = lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                l.to_string()
            } else {
                l[min_indent..].to_string()
            }
        })
        .collect();
    out.join("\n")
}

fn indentation_flexible_replacer(content: &str, find: &str) -> Vec<String> {
    let normalized_find = remove_indentation(find);
    let content_lines: Vec<&str> = content.split('\n').collect();
    let find_lines: Vec<&str> = find.split('\n').collect();

    let mut results = Vec::new();
    for i in 0..=content_lines.len().saturating_sub(find_lines.len()) {
        let block = content_lines[i..i + find_lines.len()].join("\n");
        if remove_indentation(&block) == normalized_find {
            results.push(block);
        }
    }
    results
}

// ── 6. EscapeNormalizedReplacer ─────────────────────────────────────────────

static ESCAPE_REGEXP: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    // Build pattern piecewise to avoid raw-string quote/backtick issues
    // Intended: \\(n|t|r|'|\"|`|\\|\n|\$)
    let mut pat = String::from(r"\\(n|t|r|'");
    pat.push_str("|\"");
    pat.push_str("|`|");
    pat.push_str(r"\\");
    pat.push_str(r"|\n|\$)");
    Regex::new(&pat).unwrap()
});

fn unescape_string(s: &str) -> String {
    ESCAPE_REGEXP
        .replace_all(s, |caps: &regex::Captures| {
            let m = &caps[0];
            if m.len() < 2 {
                return m.to_string();
            }
            match m.as_bytes()[1] {
                b'n' => "\n".to_string(),
                b't' => "\t".to_string(),
                b'r' => "\r".to_string(),
                b'\'' => "'".to_string(),
                b'"' => "\"".to_string(),
                b'`' => "`".to_string(),
                b'\\' => "\\".to_string(),
                b'\n' => "\n".to_string(),
                b'$' => "$".to_string(),
                _ => m.to_string(),
            }
        })
        .to_string()
}

fn escape_normalized_replacer(content: &str, find: &str) -> Vec<String> {
    let unescaped_find = unescape_string(find);
    let mut results = Vec::new();

    // Step 1: direct inclusion
    if content.contains(&unescaped_find) {
        results.push(unescaped_find.clone());
    }

    // Step 2: sliding window, unescape each block then compare
    let lines: Vec<&str> = content.split('\n').collect();
    let find_lines: Vec<&str> = unescaped_find.split('\n').collect();
    for i in 0..=lines.len().saturating_sub(find_lines.len()) {
        let block = lines[i..i + find_lines.len()].join("\n");
        if unescape_string(&block) == unescaped_find {
            results.push(block);
        }
    }
    results
}

// ── 7. TrimmedBoundaryReplacer ──────────────────────────────────────────────

fn trimmed_boundary_replacer(content: &str, find: &str) -> Vec<String> {
    let trimmed_find = find.trim();
    if trimmed_find == find {
        return vec![]; // already trimmed, meaningless
    }

    let mut results = Vec::new();

    // Step 1: direct inclusion
    if content.contains(trimmed_find) {
        results.push(trimmed_find.to_string());
    }

    // Step 2: sliding window
    let lines: Vec<&str> = content.split('\n').collect();
    let find_lines: Vec<&str> = find.split('\n').collect();
    for i in 0..=lines.len().saturating_sub(find_lines.len()) {
        let block = lines[i..i + find_lines.len()].join("\n");
        if block.trim() == trimmed_find {
            results.push(block);
        }
    }
    results
}

// ── 8. TabNewlineReplacer ──────────────────────────────────────────────────
// Handle AI models writing \t instead of \n:
// If find has no \n but has \t, replace each \t with \n\t and try matching.

fn tab_newline_replacer(content: &str, find: &str) -> Vec<String> {
    if find.contains('\n') {
        return vec![];
    }
    if !find.contains('\t') {
        return vec![];
    }
    let fixed = find.replace('\t', "\n\t");
    if content.contains(&fixed) {
        vec![fixed]
    } else {
        vec![]
    }
}

// ── 9. ContextAwareReplacer ─────────────────────────────────────────────────

fn context_aware_replacer(content: &str, find: &str) -> Vec<String> {
    let mut find_lines: Vec<&str> = find.split('\n').collect();
    if find_lines.len() < 3 {
        return vec![];
    }
    if find_lines.last() == Some(&"") {
        find_lines.pop();
    }
    if find_lines.len() < 3 {
        return vec![];
    }

    let first_line = find_lines[0].trim();
    let last_line = find_lines[find_lines.len() - 1].trim();
    let content_lines: Vec<&str> = content.split('\n').collect();

    for i in 0..content_lines.len() {
        if content_lines[i].trim() != first_line {
            continue;
        }
        for j in (i + 2)..content_lines.len() {
            if content_lines[j].trim() != last_line {
                continue;
            }
            let block_lines = &content_lines[i..=j];
            if block_lines.len() != find_lines.len() {
                break;
            }

            // Count matching middle lines
            let mut matching_lines = 0usize;
            let mut total_non_empty = 0usize;
            for k in 1..block_lines.len() - 1 {
                let bl = block_lines[k].trim();
                let fl = find_lines[k].trim();
                if !bl.is_empty() || !fl.is_empty() {
                    total_non_empty += 1;
                    if bl == fl {
                        matching_lines += 1;
                    }
                }
            }
            if total_non_empty == 0
                || matching_lines as f64 / total_non_empty as f64 >= 0.5
            {
                return vec![block_lines.join("\n")];
            }
            break;
        }
    }
    vec![]
}

// ── 10. MultiOccurrenceReplacer ─────────────────────────────────────────────

fn multi_occurrence_replacer(content: &str, find: &str) -> Vec<String> {
    if find.is_empty() {
        return vec![];
    }
    let mut results = Vec::new();
    let mut start = 0;
    while let Some(idx) = content[start..].find(find) {
        results.push(find.to_string());
        start += idx + find.len();
    }
    results
}

// ── replace main function ───────────────────────────────────────────────────

fn replace(content: &str, old_string: &str, new_string: &str, replace_all: bool) -> Result<String, String> {
    if old_string == new_string {
        return Err("No changes to apply: oldString and newString are identical.".to_string());
    }

    let replacers: &[Replacer] = &[
        simple_replacer,
        line_trimmed_replacer,
        block_anchor_replacer,
        whitespace_normalized_replacer,
        indentation_flexible_replacer,
        escape_normalized_replacer,
        trimmed_boundary_replacer,
        tab_newline_replacer,
        context_aware_replacer,
        multi_occurrence_replacer,
    ];

    let mut not_found = true;

    for replacer in replacers {
        for search in replacer(content, old_string) {
            let index = match content.find(&search) {
                Some(i) => i,
                None => continue,
            };
            not_found = false;
            if replace_all {
                return Ok(content.replace(&search, new_string));
            }
            let last_index = content.rfind(&search).unwrap();
            if index != last_index {
                continue; // appears multiple times, skip this candidate
            }
            let mut result = String::with_capacity(content.len() - search.len() + new_string.len());
            result.push_str(&content[..index]);
            result.push_str(new_string);
            result.push_str(&content[index + search.len()..]);
            return Ok(result);
        }
    }

    if not_found {
        Err("Could not find old_string in the file. It must match exactly, including whitespace, indentation, and line endings.".to_string())
    } else {
        Err("Found multiple matches for old_string. Provide more surrounding context to make the match unique.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_replacer() {
        let content = "hello world";
        let result = replace(content, "world", "rust", false).unwrap();
        assert_eq!(result, "hello rust");
    }

    #[test]
    fn test_line_trimmed_replacer() {
        let content = "  hello  \n  world  ";
        let result = replace(content, "hello\nworld", "foo\nbar", false).unwrap();
        assert_eq!(result, "foo\nbar");
    }

    #[test]
    fn test_indentation_flexible() {
        let content = "    hello\n    world";
        let result = replace(content, "hello\nworld", "foo\nbar", false).unwrap();
        assert_eq!(result, "    foo\n    bar");
    }

    #[test]
    fn test_replace_all() {
        let content = "aaa bbb aaa";
        let result = replace(content, "aaa", "ccc", true).unwrap();
        assert_eq!(result, "ccc bbb ccc");
    }

    #[test]
    fn test_not_found() {
        let result = replace("hello world", "xyz", "abc", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_identical_strings() {
        let result = replace("hello", "hello", "hello", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("same", "same"), 0);
    }
}
