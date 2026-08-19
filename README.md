# resume2

CLI tool that tailors a resume DOCX to a job posting by matching your configured skills against the posting text and filling in template placeholders.

## Setup

Build and install:

```bash
cargo build --release
```

Set the path to your resume template (must be a `.docx` file):

```bash
resume2 resume-path path/to/resume.docx
```

Your template should include these placeholders (typically in the header):

```
Skills: [SKILLS]
Databases: [DATABASES]
Tools & Platforms: [TOOLS]
```

Config and skills are stored in app data and persist across runs.

## Commands

### Set resume path

```bash
resume2 resume-path <DOCX_FILE>
```

### Manage skills

Skills are grouped into three categories: **tech**, **database**, and **tool**. Each category uses the same subcommands:

```bash
resume2 tech add "C#" --aliases CSharp,C-Sharp
resume2 tech add "Java"
resume2 tech remove "Java"
resume2 tech list

resume2 database add "PostgreSQL" --aliases Postgres,PG
resume2 database list

resume2 tool add "Docker"
resume2 tool list
```

- **add** — Add a skill. Use `--aliases` for alternate spellings (comma-separated).
- **remove** — Remove a skill by name.
- **list** — Show configured skills and their aliases.

### Generate a tailored resume

```bash
resume2 job
```

This will:

1. Prompt for the company name.
2. Prompt for the job description (paste text; enter an empty line when done).
3. Scan the posting for your configured skills (case-insensitive, including aliases).
4. Write a new file in the **current directory**: `{Original filename} - ({Company name}).docx`

## How matching works

- Each skill's **name** and **aliases** are counted in the job description.
- Only skills with at least one match are included.
- Matched skills are ordered by mention count (most first). Ties keep the order they were added.
- Unmatched skills are omitted.

**Example:** Skills `C#` (aliases: `CSharp`, `C-Sharp`), `TypeScript`, `Java`. A posting mentions `java` twice and `CSharp` once:

- `[SKILLS]` becomes `Java, C#`
- `TypeScript` is omitted (never mentioned)

## Placeholder behavior

| Placeholder   | Category  |
|---------------|-----------|
| `[SKILLS]`    | tech      |
| `[DATABASES]` | database  |
| `[TOOLS]`     | tool      |

If **no skills** in a category match the posting, the entire paragraph containing that placeholder is removed from the output document.
