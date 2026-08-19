use std::fs::{self, File};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use easyconfig::{ConfigHolder, SaveTo::AppData};
use regex::Regex;
use zip::read::ZipArchive;
use zip::{ZipWriter, write::SimpleFileOptions};

#[derive(Parser)]
#[command(name = "resume2")]
struct Cli {
    #[command(subcommand)]
    command: SubCmd,
}

#[derive(Subcommand)]
enum SubCmd {
    ResumePath {
        #[arg(value_name = "DOCX_FILE")]
        path_to_docx: PathBuf,
    },
    Tech {
        #[command(subcommand)]
        action: SkillAction,
    },
    Database {
        #[command(subcommand)]
        action: SkillAction,
    },
    Tool {
        #[command(subcommand)]
        action: SkillAction,
    },
    Job,
}

#[derive(Subcommand)]
enum SkillAction {
    Add {
        name: String,
        #[arg(long, value_delimiter = ',')]
        aliases: Vec<String>,
    },
    Remove {
        name: String,
    },
    List,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct Skill {
    name: String,
    aliases: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Config {
    path_to_resume_docx: Option<String>,
    tech_skills: Vec<Skill>,
    database_skills: Vec<Skill>,
    tool_skills: Vec<Skill>,
}

#[derive(Clone, Copy)]
enum SkillCategory {
    Tech,
    Database,
    Tool,
}

impl SkillCategory {
    fn label(self) -> &'static str {
        match self {
            Self::Tech => "tech",
            Self::Database => "database",
            Self::Tool => "tool",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Self::Tech => "[SKILLS]",
            Self::Database => "[DATABASES]",
            Self::Tool => "[TOOLS]",
        }
    }

    fn skills(self, config: &Config) -> &[Skill] {
        match self {
            Self::Tech => &config.tech_skills,
            Self::Database => &config.database_skills,
            Self::Tool => &config.tool_skills,
        }
    }

    fn skills_mut(self, config: &mut Config) -> &mut Vec<Skill> {
        match self {
            Self::Tech => &mut config.tech_skills,
            Self::Database => &mut config.database_skills,
            Self::Tool => &mut config.tool_skills,
        }
    }
}

fn get_config() -> Config {
    ConfigHolder::new(AppData, "config")
        .get_or_create()
        .expect("Failed to get config")
}

fn save_config(config: &Config) -> Result<()> {
    ConfigHolder::new(AppData, "config").write(config)
}

fn get_resume_docx_path() -> Result<PathBuf> {
    let config = get_config();
    let path = config
        .path_to_resume_docx
        .ok_or_else(|| anyhow::anyhow!("Path to resume docx not set"))?;
    let path = PathBuf::from(path);
    if !path.exists() {
        bail!("Path to resume docx does not exist");
    }
    Ok(path)
}

fn set_resume_path(path: PathBuf) -> Result<()> {
    if !path.exists() {
        bail!("File does not exist: {}", path.display());
    }
    if path.extension().and_then(|ext| ext.to_str()) != Some("docx") {
        bail!("File must be a .docx document");
    }

    let mut config = get_config();
    config.path_to_resume_docx = Some(path.to_string_lossy().to_string());
    save_config(&config)?;
    println!("Resume path set to {}", path.display());
    Ok(())
}

fn add_skill(category: SkillCategory, name: String, aliases: Vec<String>) -> Result<()> {
    let mut config = get_config();
    let skills = category.skills_mut(&mut config);

    if skills
        .iter()
        .any(|skill| skill.name.eq_ignore_ascii_case(&name))
    {
        bail!("{} skill '{}' already exists", category.label(), name);
    }

    let aliases: Vec<String> = aliases
        .into_iter()
        .map(|alias| alias.trim().to_string())
        .filter(|alias| !alias.is_empty())
        .collect();

    skills.push(Skill { name, aliases });
    save_config(&config)?;
    Ok(())
}

fn remove_skill(category: SkillCategory, name: &str) -> Result<()> {
    let mut config = get_config();
    let skills = category.skills_mut(&mut config);
    let original_len = skills.len();
    skills.retain(|skill| !skill.name.eq_ignore_ascii_case(name));

    if skills.len() == original_len {
        bail!("{} skill '{}' not found", category.label(), name);
    }

    save_config(&config)?;
    Ok(())
}

fn list_skills(category: SkillCategory) -> Result<()> {
    let config = get_config();
    let skills = category.skills(&config);

    if skills.is_empty() {
        println!("No {} skills configured.", category.label());
        return Ok(());
    }

    for skill in skills {
        if skill.aliases.is_empty() {
            println!("{}", skill.name);
        } else {
            println!("{} (aliases: {})", skill.name, skill.aliases.join(", "));
        }
    }

    Ok(())
}

fn count_occurrences(haystack: &str, term: &str) -> Result<usize> {
    if term.is_empty() {
        return Ok(0);
    }

    let escaped = regex::escape(term);
    let pattern = if term.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        format!(r"(?i)\b{escaped}\b")
    } else {
        format!(r"(?i){escaped}")
    };

    let re =
        Regex::new(&pattern).with_context(|| format!("invalid search pattern for '{term}'"))?;
    Ok(re.find_iter(haystack).count())
}

fn count_skill_matches(description: &str, skill: &Skill) -> Result<usize> {
    let mut total = count_occurrences(description, &skill.name)?;
    for alias in &skill.aliases {
        total += count_occurrences(description, alias)?;
    }
    Ok(total)
}

fn rank_skills(skills: &[Skill], description: &str) -> Result<Vec<String>> {
    let mut ranked: Vec<(usize, &Skill, usize)> = skills
        .iter()
        .enumerate()
        .map(|(index, skill)| {
            let count = count_skill_matches(description, skill)?;
            Ok((index, skill, count))
        })
        .collect::<Result<_>>()?;

    ranked.retain(|(_, _, count)| *count > 0);
    ranked.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));

    Ok(ranked
        .into_iter()
        .map(|(_, skill, _)| skill.name.clone())
        .collect())
}

fn prompt_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn prompt_job_description() -> Result<String> {
    println!("Paste the job description (empty line to finish):");
    let mut lines = Vec::new();

    loop {
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        if line.trim().is_empty() {
            break;
        }
        lines.push(line);
    }

    Ok(lines.join(""))
}

fn output_filename(source: &Path, company: &str) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("resume");
    PathBuf::from(format!("{stem} - ({company}).docx"))
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn extract_paragraph_text(paragraph: &str) -> String {
    static TEXT_TAG: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = TEXT_TAG.get_or_init(|| Regex::new(r"<w:t(?:\s[^>]*)?>([^<]*)</w:t>").unwrap());

    re.captures_iter(paragraph)
        .filter_map(|capture| capture.get(1).map(|m| m.as_str()))
        .collect()
}

fn extract_paragraph_properties(paragraph: &str) -> Option<String> {
    static PPR: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = PPR.get_or_init(|| Regex::new(r"(?s)(<w:pPr.*?</w:pPr>)").unwrap());
    re.captures(paragraph)
        .and_then(|capture| capture.get(1).map(|m| m.as_str().to_string()))
}

fn rebuild_paragraph(paragraph: &str, new_text: &str) -> String {
    let p_pr = extract_paragraph_properties(paragraph).unwrap_or_default();
    format!(
        "<w:p>{p_pr}<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        escape_xml(new_text)
    )
}

fn process_xml(xml: &str, replacements: &[(&str, Option<&str>)]) -> Result<String> {
    static PARAGRAPH: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let paragraph_re =
        PARAGRAPH.get_or_init(|| Regex::new(r"(?s)(<w:p(?:\s[^>]*)?>.*?</w:p>)").unwrap());

    let mut output = String::new();
    let mut last_end = 0;

    for capture in paragraph_re.captures_iter(xml) {
        let full_match = capture.get(1).expect("paragraph capture");
        output.push_str(&xml[last_end..full_match.start()]);

        let paragraph = full_match.as_str();
        let paragraph_text = extract_paragraph_text(paragraph);
        let mut keep_paragraph = true;
        let mut updated_paragraph = paragraph.to_string();

        for (placeholder, replacement) in replacements {
            if paragraph_text.contains(placeholder) {
                match replacement {
                    Some(value) => {
                        let new_text = paragraph_text.replace(placeholder, value);
                        updated_paragraph = rebuild_paragraph(paragraph, &new_text);
                    }
                    None => keep_paragraph = false,
                }
                break;
            }
        }

        if keep_paragraph {
            output.push_str(&updated_paragraph);
        }

        last_end = full_match.end();
    }

    output.push_str(&xml[last_end..]);
    Ok(output)
}

fn process_docx(
    source: &Path,
    destination: &Path,
    replacements: &[(&str, Option<&str>)],
) -> Result<()> {
    let source_file = File::open(source)
        .with_context(|| format!("failed to open source docx: {}", source.display()))?;
    let mut archive = ZipArchive::new(source_file)?;

    let mut output_buffer = Vec::new();
    {
        let mut writer = ZipWriter::new(Cursor::new(&mut output_buffer));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            let name = entry.name().to_string();
            let compression_method = entry.compression();
            let options = options.compression_method(compression_method);

            let mut contents = Vec::new();
            entry.read_to_end(&mut contents)?;

            let contents = if name.starts_with("word/") && name.ends_with(".xml") {
                let xml = String::from_utf8(contents)
                    .with_context(|| format!("invalid UTF-8 in zip entry '{name}'"))?;
                process_xml(&xml, replacements)?.into_bytes()
            } else {
                contents
            };

            writer
                .start_file(name, options)
                .with_context(|| "failed to start zip entry")?;
            writer
                .write_all(&contents)
                .with_context(|| "failed to write zip entry")?;
        }

        writer
            .finish()
            .with_context(|| "failed to finalize zip archive")?;
    }

    fs::write(destination, output_buffer)
        .with_context(|| format!("failed to write docx: {}", destination.display()))?;
    Ok(())
}

fn run_job() -> Result<()> {
    let resume_path = get_resume_docx_path()?;
    let config = get_config();

    let company = prompt_line("Company name: ")?;
    if company.is_empty() {
        bail!("Company name cannot be empty");
    }

    let description = prompt_job_description()?;
    if description.trim().is_empty() {
        bail!("Job description cannot be empty");
    }

    let tech = rank_skills(&config.tech_skills, &description)?;
    let databases = rank_skills(&config.database_skills, &description)?;
    let tools = rank_skills(&config.tool_skills, &description)?;

    let tech_text = (!tech.is_empty()).then(|| tech.join(", "));
    let databases_text = (!databases.is_empty()).then(|| databases.join(", "));
    let tools_text = (!tools.is_empty()).then(|| tools.join(", "));

    let replacements: Vec<(&str, Option<&str>)> = vec![
        (SkillCategory::Tech.placeholder(), tech_text.as_deref()),
        (
            SkillCategory::Database.placeholder(),
            databases_text.as_deref(),
        ),
        (SkillCategory::Tool.placeholder(), tools_text.as_deref()),
    ];

    let output_path = output_filename(&resume_path, &company);
    if output_path.exists() {
        bail!("Output file already exists: {}", output_path.display());
    }

    process_docx(&resume_path, &output_path, &replacements)?;
    println!("Created {}", output_path.display());
    Ok(())
}

fn handle_skill_action(category: SkillCategory, action: SkillAction) -> Result<()> {
    match action {
        SkillAction::Add { name, aliases } => {
            add_skill(category, name.clone(), aliases)?;
            println!("Added {} skill '{}'", category.label(), name);
        }
        SkillAction::Remove { name } => {
            remove_skill(category, &name)?;
            println!("Removed {} skill '{}'", category.label(), name);
        }
        SkillAction::List => list_skills(category)?,
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        SubCmd::ResumePath { path_to_docx } => set_resume_path(path_to_docx),
        SubCmd::Tech { action } => handle_skill_action(SkillCategory::Tech, action),
        SubCmd::Database { action } => handle_skill_action(SkillCategory::Database, action),
        SubCmd::Tool { action } => handle_skill_action(SkillCategory::Tool, action),
        SubCmd::Job => run_job(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_skills_orders_by_count_then_insertion_order() {
        let skills = vec![
            Skill {
                name: "C#".into(),
                aliases: vec!["CSharp".into()],
            },
            Skill {
                name: "TypeScript".into(),
                aliases: vec![],
            },
            Skill {
                name: "Java".into(),
                aliases: vec![],
            },
        ];

        let description = "We need Java and java experience. Also CSharp.";
        let ranked = rank_skills(&skills, description).expect("rank skills");

        assert_eq!(ranked, vec!["Java", "C#"]);
    }

    #[test]
    fn process_xml_replaces_placeholder_and_removes_empty_category_line() {
        let xml = r#"<w:document>
<w:p><w:r><w:t>Skills: [SKILLS]</w:t></w:r></w:p>
<w:p><w:r><w:t>Databases: [DATABASES]</w:t></w:r></w:p>
</w:document>"#;

        let replacements = vec![("[SKILLS]", Some("Java, C#")), ("[DATABASES]", None)];

        let processed = process_xml(xml, &replacements).expect("process xml");
        assert!(processed.contains("Skills: Java, C#"));
        assert!(!processed.contains("[DATABASES]"));
        assert!(!processed.contains("Databases:"));
    }
}
