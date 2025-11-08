use crate::tools::base::{Tool, ToolResult};
use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
}

#[derive(Default)]
pub struct SkillLoader {
    pub root: PathBuf,
    pub loaded: BTreeMap<String, Skill>,
}

impl SkillLoader {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            loaded: BTreeMap::new(),
        }
    }

    pub fn discover(&mut self) -> anyhow::Result<usize> {
        if !self.root.exists() {
            return Ok(0);
        }
        for entry in walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_name() == "SKILL.md" {
                let _ = self.load_file(entry.path());
            }
        }
        Ok(self.loaded.len())
    }

    fn load_file(&mut self, path: &Path) -> anyhow::Result<()> {
        let content = std::fs::read_to_string(path)?;
        // very simple frontmatter parser
        let fm = Regex::new(r"^---\n(?s)(.*?)\n---\n(.*)$").unwrap();
        if let Some(caps) = fm.captures(&content) {
            let meta: serde_yaml::Value = serde_yaml::from_str(caps.get(1).unwrap().as_str())?;
            let name = meta
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let desc = meta
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                return Ok(());
            }
            let raw_body = caps.get(2).unwrap().as_str().trim().to_string();
            let processed =
                Self::process_skill_paths(&raw_body, path.parent().unwrap_or(Path::new(".")));
            let skill = Skill {
                name: name.clone(),
                description: desc,
                content: processed,
            };
            self.loaded.insert(name, skill);
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<String> {
        self.loaded.keys().cloned().collect()
    }
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.loaded.get(name)
    }
    pub fn metadata_prompt(&self) -> String {
        if self.loaded.is_empty() {
            return String::new();
        }
        let mut out = String::from("## Available Skills\n");
        for s in self.loaded.values() {
            out.push_str(&format!("- `{}`: {}\n", s.name, s.description));
        }
        out
    }

    fn process_skill_paths(content: &str, skill_dir: &Path) -> String {
        let mut result = content.to_string();

        // Pattern 1a: python <relpath>
        let re_python =
            Regex::new(r"(?m)^\s*python\s+(?P<rel>(?:scripts|examples|templates|reference)/\S+)")
                .unwrap();
        result = re_python
            .replace_all(&result, |caps: &regex::Captures| {
                let rel = &caps["rel"];
                let abs = skill_dir.join(rel);
                if abs.exists() {
                    format!("python {}", abs.display())
                } else {
                    caps.get(0).unwrap().as_str().to_string()
                }
            })
            .to_string();
        // Pattern 1b: backticked `relpath`
        let re_backtick =
            Regex::new(r"`(?P<rel>(?:scripts|examples|templates|reference)/[^\s`\)]+)`").unwrap();
        result = re_backtick
            .replace_all(&result, |caps: &regex::Captures| {
                let rel = &caps["rel"];
                let abs = skill_dir.join(rel);
                if abs.exists() {
                    format!("`{}`", abs.display())
                } else {
                    caps.get(0).unwrap().as_str().to_string()
                }
            })
            .to_string();

        // Pattern 2: Direct document references with verbs (see/read/refer to/check) + filename
        let re_docs = Regex::new(
            r"(?i)(?P<prefix>(?:see|read|refer to|check)\s+)(?P<file>[A-Za-z0-9_-]+\.(?:md|txt|json|yaml))(?P<suffix>[\.,;\s])",
        )
        .unwrap();
        result = re_docs
            .replace_all(&result, |caps: &regex::Captures| {
                let prefix = &caps["prefix"];
                let filename = &caps["file"];
                let suffix = &caps["suffix"];
                let abs = skill_dir.join(filename);
                if abs.exists() {
                    format!(
                        "{} `{}` (use read_file to access){}",
                        prefix,
                        abs.display(),
                        suffix
                    )
                } else {
                    caps.get(0).unwrap().as_str().to_string()
                }
            })
            .to_string();

        // Pattern 3: Markdown links with optional prefix words
        // Example: Read [`doc.md`](doc.md) or [Guide](./reference/guide.md) or [Script](scripts/run.py)
        let re_links = Regex::new(
            r"(?i)(?:(?P<prefix>(?:Read|See|Check|Refer to|Load|View)\s+))?\[(?P<text>`?[^`\]]+`?)\]\((?P<path>(?:\./)?[^)]+\.(?:md|txt|json|yaml|js|py|html))\)",
        )
        .unwrap();
        result = re_links
            .replace_all(&result, |caps: &regex::Captures| {
                let prefix = caps.name("prefix").map(|m| m.as_str()).unwrap_or("");
                let link_text = &caps["text"];
                let filepath = &caps["path"];
                let clean = filepath.strip_prefix("./").unwrap_or(filepath);
                let abs = skill_dir.join(clean);
                if abs.exists() {
                    format!(
                        "{}[{}](`{}`) (use read_file to access)",
                        prefix,
                        link_text,
                        abs.display()
                    )
                } else {
                    caps.get(0).unwrap().as_str().to_string()
                }
            })
            .to_string();

        result
    }

    #[cfg(test)]
    pub fn test_load_file(&mut self, path: &Path) -> anyhow::Result<()> {
        self.load_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(p: &Path, s: &str) {
        fs::write(p, s).expect("write");
    }

    #[test]
    fn test_rewrite_paths_when_exist() {
        let root =
            std::env::temp_dir().join(format!("miniagent_skill_test_{}", uuid::Uuid::new_v4()));
        let skill_dir = root.join("demo");
        let scripts_dir = skill_dir.join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();
        fs::create_dir_all(&skill_dir).unwrap();
        // create referenced files
        write(&scripts_dir.join("hello.py"), "print('hi')\n");
        write(&skill_dir.join("reference.md"), "ref\n");

        // SKILL.md content with three patterns
        let skill_md = format!(
            "---\nname: demo\ndescription: demo\n---\n\n`scripts/hello.py`\n\nSee reference.md.\n\nRead [Guide](./reference.md)\n"
        );
        write(&skill_dir.join("SKILL.md"), &skill_md);

        // load (direct)
        let mut loader = SkillLoader::new(&root);
        loader.test_load_file(&skill_dir.join("SKILL.md")).unwrap();
        let skill = loader.get("demo").expect("skill loaded");

        // expect absolute paths appeared
        let abs_ref = skill_dir.join("reference.md").to_string_lossy().to_string();
        assert!(skill.content.contains(&abs_ref));
    }

    #[test]
    fn test_do_not_rewrite_when_missing() {
        let root = std::env::temp_dir().join(format!(
            "miniagent_skill_test_missing_{}",
            uuid::Uuid::new_v4()
        ));
        let skill_dir = root.join("demo");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_md = "---\nname: demo\ndescription: demo\n---\n\npython scripts/missing.py\n\n";
        write(&skill_dir.join("SKILL.md"), skill_md);
        let mut loader = SkillLoader::new(&root);
        loader.test_load_file(&skill_dir.join("SKILL.md")).unwrap();
        let skill = loader.get("demo").unwrap();
        assert!(skill.content.contains("scripts/missing.py"));
    }
}

pub struct GetSkillTool {
    pub loader: std::sync::Arc<tokio::sync::RwLock<SkillLoader>>,
}

#[async_trait]
impl Tool for GetSkillTool {
    fn name(&self) -> &str {
        "get_skill"
    }
    fn description(&self) -> &str {
        "Get full content of a named Claude Skill"
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"skill_name": {"type": "string"}},
            "required": ["skill_name"],
        })
    }
    async fn execute(&self, args: Value) -> ToolResult {
        let Some(name) = args.get("skill_name").and_then(|v| v.as_str()) else {
            return ToolResult {
                success: false,
                content: String::new(),
                error: Some("missing 'skill_name'".into()),
            };
        };
        let loader = self.loader.read().await;
        match loader.get(name) {
            Some(s) => {
                let txt = format!(
                    "# Skill: {}\n\n{}\n\n---\n\n{}",
                    s.name, s.description, s.content
                );
                ToolResult {
                    success: true,
                    content: txt,
                    error: None,
                }
            }
            None => ToolResult {
                success: false,
                content: String::new(),
                error: Some(format!("Skill '{}' not found", name)),
            },
        }
    }
}
