use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

/// Joins root_dir + target_path and validates the result stays within root_dir.
/// target_path must be relative.
pub fn safe_path(root_dir: &Path, target_path: &str) -> Result<PathBuf> {
    let abs_root = canonicalize_fallback(root_dir)?;
    let joined = abs_root.join(target_path);
    let abs_target = canonicalize_fallback(&joined)?;

    if abs_target != abs_root
        && !abs_target.starts_with(abs_root.join(""))
        && abs_target.to_string_lossy() != abs_root.to_string_lossy().trim_end_matches('/')
    {
        // Re-check with separator appended
        let root_with_sep = {
            let mut s = abs_root.to_string_lossy().to_string();
            if !s.ends_with(std::path::MAIN_SEPARATOR) {
                s.push(std::path::MAIN_SEPARATOR);
            }
            s
        };
        if !abs_target.to_string_lossy().starts_with(&root_with_sep) {
            bail!("path outside sandbox");
        }
    }
    Ok(abs_target)
}

/// Validates an already-absolute (or ~-prefixed) path against one or more allowed roots.
pub fn safe_abs_path(target_path: &str, allowed_roots: &[&Path]) -> Result<PathBuf> {
    let target_path = if target_path.starts_with("~/") {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
        home.join(&target_path[2..])
    } else {
        PathBuf::from(target_path)
    };

    if !target_path.is_absolute() {
        bail!("not an absolute path");
    }

    let abs_target = canonicalize_fallback(&target_path)?;

    for root_dir in allowed_roots {
        let abs_root = canonicalize_fallback(root_dir)?;
        let root_with_sep = {
            let mut s = abs_root.to_string_lossy().to_string();
            if !s.ends_with(std::path::MAIN_SEPARATOR) {
                s.push(std::path::MAIN_SEPARATOR);
            }
            s
        };
        if abs_target.to_string_lossy().starts_with(&root_with_sep)
            || abs_target == abs_root
        {
            return Ok(abs_target);
        }
    }
    bail!("path outside sandbox")
}

fn canonicalize_fallback(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .or_else(|_| std::path::absolute(path).map_err(Into::into))
}

/// Multi-word dangerous patterns (substring match)
const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf", "rm -fr", "> /dev/", "chmod 777", "kill -9",
];

/// Single-word dangerous commands (word-boundary match)
/// Note: curl/wget are intentionally NOT blocked
const DANGEROUS_COMMANDS: &[&str] = &[
    "mkfs", "format", "nc", "netcat", "sudo", "reboot", "shutdown",
];

fn is_cmd_separator(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t' | b'\n' | b';' | b'|' | b'&' | b'(' | b')' |
        b'`' | b'\'' | b'"' | b'<' | b'>'
    )
}

pub fn is_dangerous_command(cmd: &str) -> bool {
    let lower = cmd.to_lowercase();

    // Multi-word patterns: substring match
    for pattern in DANGEROUS_PATTERNS {
        if lower.contains(pattern) {
            return true;
        }
    }

    // Single-word commands: word-boundary match
    let lower_bytes = lower.as_bytes();
    for word in DANGEROUS_COMMANDS {
        let word_bytes = word.as_bytes();
        let mut idx = 0;
        while let Some(pos) = lower[idx..].find(word) {
            let abs = idx + pos;
            let before = abs == 0 || is_cmd_separator(lower_bytes[abs - 1]);
            let after = abs + word_bytes.len() >= lower_bytes.len()
                || is_cmd_separator(lower_bytes[abs + word_bytes.len()]);
            if before && after {
                return true;
            }
            idx = abs + 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_safe_path_basic() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create a test file
        std::fs::write(root.join("test.txt"), "hello").unwrap();

        let result = safe_path(root, "test.txt");
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("test.txt"));
    }

    #[test]
    fn test_safe_path_traversal() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let result = safe_path(root, "../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("sandbox"));
    }

    #[test]
    fn test_safe_path_subdirectory() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/file.txt"), "data").unwrap();

        let result = safe_path(root, "sub/file.txt");
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_dangerous_command_dangerous() {
        let dangerous = vec![
            "rm -rf /",
            "sudo ls",
            "mkfs /dev/sda",
            "reboot now",
            "kill -9 123",
            "chmod 777 file",
            "echo hi | nc server 80",
            "shutdown -h now",
            "format c:",
        ];
        for cmd in dangerous {
            assert!(is_dangerous_command(cmd), "expected '{}' to be dangerous", cmd);
        }
    }

    #[test]
    fn test_is_dangerous_command_safe() {
        let safe = vec![
            "ls -la",
            "cat file.txt",
            "grep pattern file",
            "python script.py",
            "rm file",             // rm without -rf is allowed
            "cd /home/user",       // normal path
            "curl http://example.com",  // curl explicitly allowed
        ];
        for cmd in safe {
            assert!(!is_dangerous_command(cmd), "expected '{}' to be safe", cmd);
        }
    }
}
