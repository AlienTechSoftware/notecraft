//! The branch and commit checks behind the quality levels in SKILLS/nc-devx/SKILL.md.
//!
//! ```text
//! check-branch          L1, before a commit: not on main, a branch name with a known prefix,
//!                       nothing from local/, target/, my-notebooks/ or a log staged, no staged
//!                       file over 10 MB; behind origin/main or a merge commit is a warning
//! check-branch --push   L2, before a push: fetches main, and being behind origin/main or
//!                       carrying a merge commit stops the push. Pushing main is allowed: that
//!                       is how a branch lands, by fast-forward
//! commit-msg FILE       the commit-msg hook: a subject line `type(scope): what`, at most 72
//!                       characters
//! ```
//!
//! On GitHub (`GITHUB_ACTIONS`) the branch is the pull request's, and only its name is
//! checked, as a warning; rebasing there is the repository's "require branches to be up to
//! date" setting.

use std::path::Path;
use std::process::Command;

/// Branch prefixes, the same words as the commit types below.
const PREFIXES: &[&str] = &["feat/", "fix/", "docs/", "test/", "chore/", "refactor/", "perf/", "ci/", "build/", "release/"];

const TYPES: &[&str] = &["feat", "fix", "docs", "test", "chore", "refactor", "perf", "ci", "build", "style", "revert"];

/// A staged file larger than this is almost always a mistake (a build, a video, a notebook).
const LARGEST_FILE: u64 = 10 * 1024 * 1024;

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).current_dir(root).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn branch_name_ok(name: &str) -> bool {
    PREFIXES.iter().any(|p| name.len() > p.len() && name.starts_with(p))
}

/// Why a staged path should not be committed, if it should not.
pub fn stray_file(path: &str) -> Option<&'static str> {
    let path = path.replace('\\', "/");
    let parts: Vec<&str> = path.split('/').collect();
    let dirs = &parts[..parts.len().saturating_sub(1)];
    if dirs.contains(&"local") {
        Some("local/ holds this machine's settings and logs")
    } else if dirs.first() == Some(&"target") {
        Some("target/ is build output")
    } else if dirs.first() == Some(&"my-notebooks") {
        Some("my-notebooks/ is a disposable test copy (AGENTS.md)")
    } else if path.ends_with(".log") {
        Some("a log file")
    } else {
        None
    }
}

/// What is wrong with a commit message's subject line, if anything.
pub fn subject_problem(message: &str) -> Option<String> {
    let subject = message.lines().map(str::trim_end).find(|l| !l.trim().is_empty() && !l.starts_with('#'))?;
    // Git's own subjects, and the ones `git commit --fixup` writes.
    if ["Merge ", "Revert \"", "fixup! ", "squash! ", "amend! "].iter().any(|p| subject.starts_with(p)) {
        return None;
    }
    if subject.chars().count() > 72 {
        return Some(format!("the subject is {} characters; keep it to 72", subject.chars().count()));
    }
    let shape = "write the subject as `type(scope): what changed`, for example `fix(cli): open says when the app is missing`";
    let Some((head, rest)) = subject.split_once(": ") else { return Some(shape.into()) };
    let head = head.strip_suffix('!').unwrap_or(head);
    let (kind, scope) = match head.split_once('(') {
        Some((k, s)) => (k, s.strip_suffix(')')),
        None => (head, Some("x")),
    };
    let scope_ok = scope.is_some_and(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_./".contains(c)));
    if !TYPES.contains(&kind) {
        return Some(format!("'{kind}' is not a commit type. Use one of: {}", TYPES.join(", ")));
    }
    if !scope_ok || rest.trim().is_empty() {
        return Some(shape.into());
    }
    None
}

pub fn commit_msg(file: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("read {}: {e}", file.display()))?;
    match subject_problem(&text) {
        Some(p) => Err(format!("commit-msg: {p}")),
        None if text.lines().all(|l| l.trim().is_empty() || l.starts_with('#')) => Err("commit-msg: the message is empty".into()),
        None => Ok(()),
    }
}

pub fn check_branch(root: &Path, options: &[String]) -> Result<(), String> {
    let push = match options {
        [] => false,
        [o] if o == "--push" => true,
        _ => return Err("usage: cargo xtask check-branch [--push]".into()),
    };
    let mut problems: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        if let Some(b) = std::env::var("GITHUB_HEAD_REF").ok().filter(|b| !b.is_empty())
            && !branch_name_ok(&b)
        {
            println!("::warning title=Branch name::'{b}' does not start with one of {}", PREFIXES.join(" "));
        }
        println!("check-branch: ok");
        return Ok(());
    }

    let Some(branch) = git(root, &["symbolic-ref", "--short", "-q", "HEAD"]) else {
        println!("check-branch: no branch checked out (a rebase in progress, for one); skipped");
        return Ok(());
    };
    let on_main = branch == "main" || branch == "master";
    if on_main && !push {
        problems.push("you are on main. Work on a branch: git switch -c feat/<what> (AGENTS.md)".into());
    } else if !on_main && !branch_name_ok(&branch) {
        warnings.push(format!("the branch '{branch}' does not start with one of: {}", PREFIXES.join(" ")));
    }

    if push && git(root, &["fetch", "--quiet", "origin", "main"]).is_none() {
        warnings.push("could not fetch origin/main; compared with the copy from the last fetch".into());
    }
    if git(root, &["rev-parse", "--verify", "--quiet", "origin/main"]).is_some() {
        let behind: u32 = git(root, &["rev-list", "--count", "HEAD..origin/main"]).and_then(|n| n.parse().ok()).unwrap_or(0);
        if behind > 0 {
            let msg = format!("{behind} commit(s) on origin/main are not in '{branch}'. Rebase: git fetch origin && git rebase origin/main");
            if push { problems.push(msg) } else { warnings.push(msg) }
        }
        let merges: u32 = git(root, &["rev-list", "--merges", "--count", "origin/main..HEAD"]).and_then(|n| n.parse().ok()).unwrap_or(0);
        if merges > 0 {
            let msg = format!("'{branch}' has {merges} merge commit(s); history stays a straight line, so rebase instead (AGENTS.md)");
            if push { problems.push(msg) } else { warnings.push(msg) }
        }
    } else {
        warnings.push("there is no origin/main to compare with".into());
    }

    if !push {
        for path in git(root, &["diff", "--cached", "--name-only", "--diff-filter=AM"]).unwrap_or_default().lines().filter(|l| !l.is_empty()) {
            if let Some(why) = stray_file(path) {
                problems.push(format!("{path} is staged: {why}. Unstage it: git restore --staged {path}"));
            } else if std::fs::metadata(root.join(path)).is_ok_and(|m| m.len() > LARGEST_FILE) {
                problems.push(format!("{path} is over {} MB; a file that size does not belong in git", LARGEST_FILE / 1024 / 1024));
            }
        }
    }

    for w in &warnings {
        println!("warning: {w}");
    }
    if problems.is_empty() {
        println!("check-branch: ok ({branch})");
        Ok(())
    } else {
        Err(problems.join("\n       "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_names_need_a_prefix() {
        assert!(branch_name_ok("feat/open-path"));
        assert!(branch_name_ok("test/coverage"));
        assert!(!branch_name_ok("feat/"));
        assert!(!branch_name_ok("box-select-delete"));
        assert!(!branch_name_ok("main"));
    }

    #[test]
    fn stray_files_are_named() {
        assert!(stray_file("local/settings.toml").is_some());
        assert!(stray_file("apps/notecraft-cli/local/notecraft.log").is_some());
        assert!(stray_file("target/debug/x.exe").is_some());
        assert!(stray_file("my-notebooks/welcome/page.html").is_some());
        assert!(stray_file("notes.log").is_some());
        assert!(stray_file("crates/engine/src/local.rs").is_none(), "a file called local is fine");
        assert!(stray_file("docs/coverage.json").is_none());
    }

    #[test]
    fn subjects_follow_the_shape() {
        assert_eq!(subject_problem("feat(open): one way to open a page\n\nbody"), None);
        assert_eq!(subject_problem("docs: the clipboard matches the code"), None);
        assert_eq!(subject_problem("# comment\nfix(cli)!: breaking"), None);
        assert_eq!(subject_problem("Merge branch 'x'"), None);
        assert_eq!(subject_problem("fixup! feat(open): x"), None);
        assert!(subject_problem("update stuff").is_some_and(|p| p.contains("type(scope)")));
        assert!(subject_problem("feature: x").is_some_and(|p| p.contains("not a commit type")));
        assert!(subject_problem("fix(CLI): x").is_some(), "scopes are lower case");
        assert!(subject_problem("fix(): x").is_some());
        assert!(subject_problem(&format!("fix: {}", "a".repeat(80))).is_some_and(|p| p.contains("72")));
    }
}
