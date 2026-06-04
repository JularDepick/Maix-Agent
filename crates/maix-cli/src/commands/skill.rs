//! Skill management commands.

use crate::cli::SkillAction;
use super::copy_dir_recursive;
use maix_core::client::MaixClient;
use std::path::PathBuf;

pub async fn cmd_skill(client: &MaixClient, action: SkillAction) {
    match action {
        SkillAction::Install { source } => {
            let source_path = std::path::Path::new(&source);
            let skills_dir = home::home_dir()
                .map(|h| h.join(".maix").join("skills"))
                .unwrap_or_else(|| PathBuf::from(".maix/skills"));

            if source_path.exists() {
                let skill_name = source_path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unnamed".into());
                let dest = skills_dir.join(&skill_name);

                if dest.exists() {
                    eprintln!("Skill '{}' already exists at {}", skill_name, dest.display());
                    eprintln!("Remove it first or use a different name.");
                    std::process::exit(1);
                }

                if source_path.is_dir() {
                    match copy_dir_recursive(source_path, &dest) {
                        Ok(_) => {
                            match maix_skills::manifest::SkillManifest::from_dir(&dest) {
                                Ok(manifest) => {
                                    println!("Installed skill '{}' v{} from {}", manifest.skill.name, manifest.skill.version, source);
                                    println!("  Location: {}", dest.display());
                                    if let Some(desc) = &manifest.skill.description {
                                        println!("  Description: {}", desc);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Warning: skill installed but manifest invalid: {e}");
                                    println!("Installed to {}", dest.display());
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error copying skill: {e}");
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Source must be a directory");
                    std::process::exit(1);
                }
            } else if source.starts_with("http://") || source.starts_with("https://") {
                match install_from_url(&source, &skills_dir).await {
                    Ok(name) => println!("Installed skill '{}' from URL", name),
                    Err(e) => {
                        eprintln!("Error installing from URL: {e}");
                        std::process::exit(1);
                    }
                }
            } else if source.contains('/') && !source.starts_with('/') {
                // GitHub shorthand: owner/repo or owner/repo:branch
                match install_from_github(&source, &skills_dir).await {
                    Ok(name) => println!("Installed skill '{}' from GitHub", name),
                    Err(e) => {
                        eprintln!("Error installing from GitHub: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!("Source not found: {}", source);
                std::process::exit(1);
            }
        }
        SkillAction::List => match client.list_skills().await {
            Ok(list) => {
                for s in &list {
                    println!(
                        "  {} v{} ({})",
                        s.name,
                        s.version,
                        if s.enabled { "enabled" } else { "disabled" }
                    );
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        },
        SkillAction::Enable { name } => match client.enable_skill(&name).await {
            Ok(_) => println!("Enabled: {name}"),
            Err(e) => eprintln!("Error: {e}"),
        },
        SkillAction::Disable { name } => match client.disable_skill(&name).await {
            Ok(_) => println!("Disabled: {name}"),
            Err(e) => eprintln!("Error: {e}"),
        },
    }
}

async fn install_from_url(url: &str, skills_dir: &std::path::Path) -> Result<String, String> {
    use std::io::Read;

    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let bytes = resp.bytes().await.map_err(|e| format!("read failed: {e}"))?;

    // Try to extract as zip archive
    let cursor = std::io::Cursor::new(bytes.as_ref());
    match zip::ZipArchive::new(cursor) {
        Ok(mut archive) => {
            // Find the root directory name (GitHub archives have a single root dir)
            let root_prefix = archive
                .by_index(0)
                .ok()
                .and_then(|f| {
                    let name = f.name();
                    name.split('/').next().map(|s| s.to_string())
                });

            let temp_dir = std::env::temp_dir().join(format!("maix-skill-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&temp_dir)
                .map_err(|e| format!("create temp dir: {e}"))?;

            for i in 0..archive.len() {
                let mut file = archive.by_index(i)
                    .map_err(|e| format!("read zip entry: {e}"))?;
                let outpath = temp_dir.join(file.name());

                if file.is_dir() {
                    std::fs::create_dir_all(&outpath)
                        .map_err(|e| format!("create dir: {e}"))?;
                } else {
                    if let Some(parent) = outpath.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("create parent: {e}"))?;
                    }
                    let mut contents = Vec::new();
                    file.read_to_end(&mut contents)
                        .map_err(|e| format!("read file: {e}"))?;
                    std::fs::write(&outpath, contents)
                        .map_err(|e| format!("write file: {e}"))?;
                }
            }

            // Find the skill directory (either root or first subdir)
            let skill_src = if let Some(prefix) = root_prefix {
                let root_path = temp_dir.join(&prefix);
                if root_path.join("skill.toml").exists() || root_path.join("manifest.toml").exists() {
                    root_path
                } else {
                    temp_dir.clone()
                }
            } else {
                temp_dir.clone()
            };

            let skill_name = skill_src.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unnamed".into());
            let dest = skills_dir.join(&skill_name);

            if dest.exists() {
                return Err(format!("skill '{}' already exists at {}", skill_name, dest.display()));
            }

            copy_dir_recursive(&skill_src, &dest)
                .map_err(|e| format!("copy skill: {e}"))?;

            // Clean up temp dir
            let _ = std::fs::remove_dir_all(&temp_dir);

            match maix_skills::manifest::SkillManifest::from_dir(&dest) {
                Ok(manifest) => {
                    println!("  Location: {}", dest.display());
                    if let Some(desc) = &manifest.skill.description {
                        println!("  Description: {}", desc);
                    }
                    Ok(manifest.skill.name)
                }
                Err(_) => Ok(skill_name),
            }
        }
        Err(_) => Err("downloaded file is not a valid zip archive".to_string()),
    }
}

async fn install_from_github(shorthand: &str, skills_dir: &std::path::Path) -> Result<String, String> {
    let url = github_archive_url(shorthand);
    install_from_url(&url, skills_dir).await
}

/// Construct a GitHub archive URL from a shorthand like "owner/repo" or "owner/repo:branch".
fn github_archive_url(shorthand: &str) -> String {
    let (repo_path, branch) = if let Some((repo, branch)) = shorthand.split_once(':') {
        (repo, branch)
    } else {
        (shorthand, "main")
    };
    format!(
        "https://github.com/{}/archive/refs/heads/{}.zip",
        repo_path, branch
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_archive_url_default_branch() {
        let url = github_archive_url("owner/repo");
        assert_eq!(url, "https://github.com/owner/repo/archive/refs/heads/main.zip");
    }

    #[test]
    fn test_github_archive_url_custom_branch() {
        let url = github_archive_url("owner/repo:develop");
        assert_eq!(url, "https://github.com/owner/repo/archive/refs/heads/develop.zip");
    }

    #[test]
    fn test_github_archive_url_nested_repo() {
        let url = github_archive_url("org/skill-repo:feature-branch");
        assert_eq!(url, "https://github.com/org/skill-repo/archive/refs/heads/feature-branch.zip");
    }
}
