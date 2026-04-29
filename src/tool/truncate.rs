use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_LINES: usize = 2000;
pub const MAX_BYTES: usize = 50 * 1024;

/// Check if output exceeds limits, truncate and save full output to a temp file.
/// Returns (output, truncated).
pub fn truncate(output: &str) -> (String, bool) {
    let normalized = output.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();

    if lines.len() <= MAX_LINES && normalized.len() <= MAX_BYTES {
        return (output.to_string(), false);
    }

    let end = std::cmp::min(MAX_LINES, lines.len());
    let mut preview = lines[..end].join("\n");
    if preview.len() > MAX_BYTES {
        preview.truncate(MAX_BYTES);
    }

    // Save full output to file
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = home.join(".openlink").join("tool-output");
    let _ = std::fs::create_dir_all(&dir);

    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let full_path = dir.join(id.to_string());
    let _ = std::fs::write(&full_path, output);

    let hint = format!(
        "\n\n...输出已截断（共 {} 行），完整内容保存至:\n{}\n使用 read_file 工具加 offset 参数分段读取",
        lines.len(),
        full_path.display(),
    );

    (preview + &hint, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_truncation() {
        let output = "line1\nline2\nline3";
        let (result, truncated) = truncate(output);
        assert!(!truncated);
        assert_eq!(result, output);
    }

    #[test]
    fn test_line_truncation() {
        let lines: String = (1..=3000).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        let (result, truncated) = truncate(&lines);
        assert!(truncated);
        assert!(result.contains("输出已截断"));
        assert!(result.contains("3000 行"));
    }
}
