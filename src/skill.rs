//! Agent skills: a directory per skill, each holding a `SKILL.md`.
//!
//! Unlike providers and MCP servers, skills are not config keys — they are
//! folders on disk, and the agent picks up whatever it finds. So the operations
//! here are filesystem ones, and the interesting question is not "what does the
//! config say" but "what is actually installed, and does it parse".
//!
//! Codex has no equivalent; its plugins are a different mechanism and are not
//! modelled here.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// The file that makes a directory a skill.
pub const MANIFEST: &str = "SKILL.md";

/// The front matter an agent reads before deciding whether a skill is relevant.
#[derive(Debug, Clone, Default, Deserialize)]
struct FrontMatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "allowed-tools")]
    allowed_tools: Vec<String>,
}

/// One installed skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// The directory name, which is what the agent addresses it by.
    pub directory: String,
    /// The `name` in the front matter, if it declares one.
    pub declared_name: Option<String>,
    pub description: Option<String>,
    pub allowed_tools: Vec<String>,
    pub path: PathBuf,
    /// Why this skill is not usable, if it is not.
    pub problem: Option<String>,
}

impl Skill {
    pub fn is_healthy(&self) -> bool {
        self.problem.is_none()
    }

    /// First line of the description, for a listing that has one row per skill.
    pub fn summary(&self) -> String {
        let Some(description) = &self.description else {
            return String::new();
        };
        description.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

/// Read every skill directly inside `dir`.
///
/// A directory without a `SKILL.md`, or with one the agent could not read, is
/// still reported — a silently ignored skill is the thing people come here to
/// diagnose.
pub fn read_dir(dir: &Path) -> Vec<Skill> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut skills: Vec<Skill> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .map(|path| read_one(&path))
        .collect();
    skills.sort_by(|a, b| a.directory.cmp(&b.directory));
    skills
}

fn read_one(path: &Path) -> Skill {
    let directory = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let mut skill = Skill {
        directory: directory.clone(),
        declared_name: None,
        description: None,
        allowed_tools: Vec::new(),
        path: path.to_path_buf(),
        problem: None,
    };

    let manifest = path.join(MANIFEST);
    let Ok(text) = fs::read_to_string(&manifest) else {
        skill.problem = Some(format!("no {MANIFEST}"));
        return skill;
    };

    match parse_front_matter(&text) {
        Ok(front) => {
            skill.declared_name = front.name.clone();
            skill.description = front.description.clone();
            skill.allowed_tools = front.allowed_tools;

            skill.problem = if front.description.is_none() {
                Some(
                    "front matter has no description, so the agent cannot tell when to use it"
                        .into(),
                )
            } else if front.name.as_deref().is_some_and(|name| name != directory) {
                // Agents address a skill by its directory; a mismatched `name`
                // is the kind of thing that silently half-works.
                Some(format!(
                    "front matter says name {:?} but the directory is {directory:?}",
                    front.name.unwrap_or_default()
                ))
            } else {
                None
            };
        }
        Err(err) => skill.problem = Some(err.to_string()),
    }
    skill
}

/// Pull the YAML block delimited by `---` from the top of a Markdown file.
fn parse_front_matter(text: &str) -> Result<FrontMatter> {
    let body = text.strip_prefix("---").or_else(|| text.strip_prefix("\u{feff}---")).context(
        "no front matter: a SKILL.md must open with a `---` block naming and describing the skill",
    )?;

    let end = body.find("\n---").context("front matter is not closed by a `---` line")?;

    serde_norway::from_str(&body[..end]).context("front matter is not valid YAML")
}

/// Copy a skill directory, refusing to overwrite unless asked.
pub fn copy(from: &Path, to: &Path, force: bool) -> Result<usize> {
    if !from.join(MANIFEST).exists() {
        bail!("{} has no {MANIFEST}, so it is not a skill", from.display());
    }
    if to.exists() && !force {
        bail!("{} already exists; pass --force to replace it", to.display());
    }
    if to.exists() {
        fs::remove_dir_all(to).with_context(|| format!("replacing {}", to.display()))?;
    }
    copy_tree(from, to)
}

/// Recursive copy, returning how many files landed.
fn copy_tree(from: &Path, to: &Path) -> Result<usize> {
    fs::create_dir_all(to).with_context(|| format!("creating {}", to.display()))?;
    let mut copied = 0;

    for entry in fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());

        if source.is_dir() {
            copied += copy_tree(&source, &target)?;
        } else {
            fs::copy(&source, &target).with_context(|| format!("copying {}", source.display()))?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("confai-skill-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_skill(root: &Path, name: &str, manifest: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MANIFEST), manifest).unwrap();
    }

    #[test]
    fn a_folded_description_survives_parsing() {
        // The form real skills use, and the reason this is not hand-rolled.
        let front = parse_front_matter(
            "---\nname: consult\ndescription: >-\n  first line\n  second line\n---\nbody\n",
        )
        .unwrap();

        assert_eq!(front.name.as_deref(), Some("consult"));
        assert_eq!(front.description.as_deref(), Some("first line second line"));
    }

    #[test]
    fn allowed_tools_come_through_as_a_list() {
        let front = parse_front_matter(
            "---\nname: a\ndescription: d\nallowed-tools:\n  - Bash\n  - Read\n---\n",
        )
        .unwrap();
        assert_eq!(front.allowed_tools, vec!["Bash", "Read"]);
    }

    #[test]
    fn a_file_without_front_matter_says_so() {
        let err = parse_front_matter("# Just markdown\n").unwrap_err().to_string();
        assert!(err.contains("no front matter"), "{err}");

        let err = parse_front_matter("---\nname: a\n").unwrap_err().to_string();
        assert!(err.contains("not closed"), "{err}");
    }

    #[test]
    fn reading_a_directory_reports_healthy_and_broken_alike() {
        let root = scratch("read");
        write_skill(&root, "good", "---\nname: good\ndescription: does a thing\n---\n");
        write_skill(&root, "nameless", "---\nname: nameless\n---\n");
        write_skill(&root, "mismatched", "---\nname: other\ndescription: d\n---\n");
        fs::create_dir_all(root.join("empty")).unwrap();

        let skills = read_dir(&root);
        assert_eq!(skills.len(), 4);

        let by = |name: &str| skills.iter().find(|s| s.directory == name).unwrap().clone();
        assert!(by("good").is_healthy());
        assert_eq!(by("good").summary(), "does a thing");
        assert!(by("nameless").problem.unwrap().contains("no description"));
        assert!(by("mismatched").problem.unwrap().contains("directory"));
        assert!(by("empty").problem.unwrap().contains(MANIFEST));
    }

    #[test]
    fn an_absent_directory_reads_as_no_skills() {
        assert!(read_dir(Path::new("definitely-not-here-xyz")).is_empty());
    }

    #[test]
    fn copying_carries_nested_files_and_refuses_to_clobber() {
        let root = scratch("copy");
        write_skill(&root, "src", "---\nname: src\ndescription: d\n---\n");
        fs::create_dir_all(root.join("src").join("scripts")).unwrap();
        fs::write(root.join("src").join("scripts").join("run.sh"), "echo hi").unwrap();

        let target = root.join("dest");
        assert_eq!(copy(&root.join("src"), &target, false).unwrap(), 2);
        assert!(target.join("scripts").join("run.sh").exists());

        let err = copy(&root.join("src"), &target, false).unwrap_err().to_string();
        assert!(err.contains("--force"), "{err}");
        assert_eq!(copy(&root.join("src"), &target, true).unwrap(), 2);
    }

    #[test]
    fn a_directory_without_a_manifest_is_not_copyable() {
        let root = scratch("nomanifest");
        fs::create_dir_all(root.join("bare")).unwrap();
        let err = copy(&root.join("bare"), &root.join("out"), false).unwrap_err().to_string();
        assert!(err.contains(MANIFEST), "{err}");
    }
}
