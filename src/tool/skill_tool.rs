use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::time::Instant;

use serde_json::Value;

use super::{Tool, ToolContext, ToolResult};
use crate::skill;

pub struct SkillTool;

impl Tool for SkillTool {
    fn name(&self) -> &str {
        "skill"
    }

    fn description(&self) -> &str {
        "Load a specialized skill from skills directories"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "skill": "string (optional) - skill name to load; omit to list available skills"
        })
    }

    fn validate(&self, _args: &HashMap<String, Value>) -> Result<(), String> {
        Ok(())
    }

    fn execute(&self, ctx: &ToolContext) -> ToolResult {
        let skill_name = ctx.args.get("skill").and_then(|v| v.as_str());

        match skill_name {
            None | Some("") => {
                let infos = skill::load_infos(&ctx.config.root_dir);
                if infos.is_empty() {
                    return ToolResult {
                        status: "success",
                        output: "没有找到可用的 skills".to_string(),
                        error: String::new(),
                        stop_stream: false,
                        start_time: Instant::now(),
                        end_time: Some(Instant::now()),
                    };
                }
                let names: Vec<&str> = infos.iter().map(|s| s.name.as_str()).collect();
                ToolResult {
                    status: "success",
                    output: format!("可用 skills: {}", names.join(", ")),
                    error: String::new(),
                    stop_stream: false,
                    start_time: Instant::now(),
                    end_time: Some(Instant::now()),
                }
            }
            Some(name) => {
                let info = match skill::get(&ctx.config.root_dir, name) {
                    Some(i) => i,
                    None => return ToolResult::error(format!("skill {:?} not found", name)),
                };

                let data = match std::fs::read_to_string(&info.location) {
                    Ok(d) => d,
                    Err(e) => return ToolResult::error(e.to_string()),
                };

                // List sibling files (up to 10, excluding SKILL.md)
                let mut siblings = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&info.dir) {
                    for entry in entries.flatten() {
                        let fname = entry.file_name().to_string_lossy().to_string();
                        if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
                            && !fname.eq_ignore_ascii_case("skill.md")
                        {
                            siblings.push(fname);
                            if siblings.len() >= 10 {
                                break;
                            }
                        }
                    }
                }

                let mut out = String::new();
                let _ = writeln!(out, "<skill_content name={:?}>", name);
                let _ = writeln!(
                    out,
                    "IMPORTANT: All file paths referenced in this skill must use absolute paths. The skill directory is: {}",
                    info.dir.display()
                );
                if !siblings.is_empty() {
                    out.push_str("Available files in skill directory (use these absolute paths directly):\n");
                    for sib in &siblings {
                        let _ = writeln!(out, "  - {}", info.dir.join(sib).display());
                    }
                }
                out.push('\n');
                out.push_str(&data);
                out.push_str("\n</skill_content>");

                ToolResult {
                    status: "success",
                    output: out,
                    error: String::new(),
                    stop_stream: false,
                    start_time: Instant::now(),
                    end_time: Some(Instant::now()),
                }
            }
        }
    }
}
