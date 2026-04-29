use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Info {
    pub name: String,
    pub description: String,
    pub dir: PathBuf,
    pub location: PathBuf, // absolute path to SKILL.md
}

pub fn skill_dirs(root_dir: &Path) -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    vec![
        root_dir.join(".skills"),
        root_dir.join(".openlink").join("skills"),
        root_dir.join(".agent").join("skills"),
        root_dir.join(".claude").join("skills"),
        home.join(".openlink").join("skills"),
        home.join(".agent").join("skills"),
        home.join(".claude").join("skills"),
    ]
}

pub fn load_infos(root_dir: &Path) -> Vec<Info> {
    use std::collections::HashMap;

    let mut seen: HashMap<String, Info> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for dir in skill_dirs(root_dir) {
        let dir_meta = match std::fs::metadata(&dir) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !dir_meta.is_dir() {
            continue;
        }

        tracing::info!("[Skill] scanning directory: {}", dir.display());

        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let sub_path = dir.join(entry.file_name());

            // Follow symlinks: use std::fs::metadata instead of entry.file_type()
            let info = match std::fs::metadata(&sub_path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if !info.is_dir() {
                continue;
            }

            let skill_file = match find_skill_md(&sub_path) {
                Some(f) => f,
                None => continue,
            };

            let data = match std::fs::read_to_string(&skill_file) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let mut sk = parse_skill(&skill_file, &data);
            sk.dir = sub_path.clone();
            sk.location = skill_file;

            tracing::info!(
                "[Skill] loaded: name={}, description={:.60}",
                sk.name,
                sk.description
            );

            if !seen.contains_key(&sk.name) {
                order.push(sk.name.clone());
            }
            seen.insert(sk.name.clone(), sk);
        }
    }

    tracing::info!("[Skill] total loaded: {}", order.len());

    order
        .into_iter()
        .filter_map(|name| seen.remove(&name))
        .collect()
}

pub fn get(root_dir: &Path, name: &str) -> Option<Info> {
    if name.contains(&['/', '\\'][..]) || name.contains("..") {
        return None;
    }
    for info in load_infos(root_dir) {
        if info.name.eq_ignore_ascii_case(name) {
            return Some(info);
        }
    }
    None
}

pub fn find_skill(root_dir: &Path, name: &str) -> Result<(String, PathBuf), String> {
    if name.contains(&['/', '\\'][..]) || name.contains("..") {
        return Err(format!("invalid skill name: {:?}", name));
    }

    for d in skill_dirs(root_dir) {
        // Flat file: dir/<name>.md
        let flat = d.join(format!("{}.md", name));
        if let Ok(data) = std::fs::read_to_string(&flat) {
            return Ok((data, d));
        }

        // Subdir: dir/<name>/SKILL.md (case-insensitive match)
        let entries = match std::fs::read_dir(&d) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
                let skill_path = d.join(entry.file_name()).join("SKILL.md");
                if let Ok(data) = std::fs::read_to_string(&skill_path) {
                    return Ok((data, d.join(entry.file_name())));
                }
            }
        }
    }

    Err(format!("skill {:?} not found", name))
}

/// Find SKILL.md in a directory (case-insensitive).
fn find_skill_md(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.eq_ignore_ascii_case("skill.md") {
                return Some(dir.join(entry.file_name()));
            }
        }
    }
    None
}

/// Parse YAML frontmatter from SKILL.md content.
fn parse_skill(path: &Path, content: &str) -> Info {
    let dir_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut name = dir_name.clone();
    let mut description = String::new();

    if !content.starts_with("---") {
        return Info {
            name,
            description,
            dir: PathBuf::new(),
            location: path.to_path_buf(),
        };
    }

    let end = match content[3..].find("---") {
        Some(i) => i,
        None => {
            return Info {
                name,
                description,
                dir: PathBuf::new(),
                location: path.to_path_buf(),
            }
        }
    };

    let front = &content[3..end + 3];
    for line in front.split('\n') {
        let line = line.trim();
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim();
            match k.trim() {
                "name" => name = v.to_string(),
                "description" => description = v.to_string(),
                _ => {}
            }
        }
    }

    Info {
        name,
        description,
        dir: PathBuf::new(),
        location: path.to_path_buf(),
    }
}
