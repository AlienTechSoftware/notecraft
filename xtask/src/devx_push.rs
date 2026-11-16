//! Prepare and verify a fast, hash-bound pre-push validation.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git").args(args).current_dir(root).output().map_err(|e| format!("cannot run git {}: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!("git {} failed", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn validation_path(root: &Path) -> Result<PathBuf, String> {
    let path = PathBuf::from(git(root, &["rev-parse", "--git-path", "notecraft-push-validation"])?);
    Ok(if path.is_absolute() { path } else { root.join(path) })
}

fn require_clean_tree(root: &Path) -> Result<(), String> {
    let status = git(root, &["status", "--porcelain", "--untracked-files=normal"])?;
    if status.is_empty() {
        Ok(())
    } else {
        Err("the working tree must be clean for a push validation; commit or move the changes, then retry".into())
    }
}

fn validation_stamp(root: &Path) -> Result<String, String> {
    require_clean_tree(root)?;
    let branch = git(root, &["symbolic-ref", "--short", "-q", "HEAD"])?;
    let head = git(root, &["rev-parse", "HEAD"])?;
    let main = git(root, &["rev-parse", "origin/main"])?;
    Ok(format!("branch={branch}\nhead={head}\norigin_main={main}\n"))
}

fn assert_current_branch(root: &Path) -> Result<(), String> {
    super::quality::check_branch(root, &["--push".into()])
}

/// Run the complete local push gate before Git opens its network transport.
pub fn prepare(root: &Path) -> Result<(), String> {
    let stamp_path = validation_path(root)?;
    let _ = fs::remove_file(&stamp_path);
    assert_current_branch(root)?;
    require_clean_tree(root)?;

    let pii = Command::new("bash")
        .args(["SKILLS/nc-devx/scripts/check-pii.sh", "origin/main"])
        .current_dir(root)
        .status()
        .map_err(|e| format!("cannot run the PII check: {e}"))?;
    if !pii.success() {
        return Err("the PII check failed".into());
    }

    let status = Command::new("cargo").args(["xtask", "ci"]).current_dir(root).status().map_err(|e| format!("cannot run cargo xtask ci: {e}"))?;
    if !status.success() {
        return Err("cargo xtask ci failed".into());
    }

    // CI must not authorize a different tree or a base that moved while CI ran.
    assert_current_branch(root)?;
    let stamp = validation_stamp(root)?;
    let parent = stamp_path.parent().ok_or_else(|| "git validation path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("create git metadata directory: {e}"))?;
    fs::write(&stamp_path, stamp).map_err(|e| format!("write push validation: {e}"))?;
    println!("pre-push: full gate passed; validation is bound to this clean commit and origin/main");
    Ok(())
}

/// Verify that the completed full gate still applies to the current push.
pub fn check(root: &Path) -> Result<(), String> {
    assert_current_branch(root)?;
    let stamp_path = validation_path(root)?;
    let recorded =
        fs::read_to_string(&stamp_path).map_err(|_| "no current push validation; run `cargo xtask pre-push` before pushing".to_string())?;
    let current = validation_stamp(root)?;
    if recorded != current {
        return Err("push validation is stale; run `cargo xtask pre-push` again before pushing".into());
    }
    println!("pre-push: current validation found; allowing push");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_ok(root: &Path, args: &[&str]) {
        let status = Command::new("git").args(args).current_dir(root).status().expect("run git");
        assert!(status.success(), "git {} failed", args.join(" "));
    }

    fn commit(root: &Path, file: &str, contents: &str) {
        fs::write(root.join(file), contents).expect("write test commit file");
        git_ok(root, &["add", file]);
        git_ok(root, &["commit", "-qm", "test"]);
    }

    fn test_repo() -> PathBuf {
        let root = std::env::temp_dir().join(format!("notecraft-push-stamp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temporary repository");
        git_ok(&root, &["init", "-q", "-b", "feat/test"]);
        git_ok(&root, &["config", "user.name", "Test"]);
        git_ok(&root, &["config", "user.email", "test@example.invalid"]);
        root
    }

    #[test]
    fn validation_stamp_changes_when_the_commit_or_base_moves() {
        let root = test_repo();
        commit(&root, "first.txt", "first\n");
        git_ok(&root, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        let initial = validation_stamp(&root).expect("stamp initial tree");

        commit(&root, "second.txt", "second\n");
        let changed_head = validation_stamp(&root).expect("stamp changed head");
        assert_ne!(initial, changed_head, "a different commit must invalidate validation");

        git_ok(&root, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        let changed_base = validation_stamp(&root).expect("stamp changed base");
        assert_ne!(changed_head, changed_base, "a different origin/main must invalidate validation");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn validation_requires_a_clean_worktree() {
        let root = std::env::temp_dir().join(format!("notecraft-push-clean-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temporary repository");
        git_ok(&root, &["init", "-q"]);
        fs::write(root.join("tracked.txt"), "committed\n").expect("write tracked file");
        git_ok(&root, &["add", "tracked.txt"]);
        let commit = Command::new("git")
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "test"])
            .current_dir(&root)
            .status()
            .expect("git commit");
        assert!(commit.success());
        assert!(require_clean_tree(&root).is_ok());
        fs::write(root.join("untracked.txt"), "changed\n").expect("write untracked file");
        assert!(require_clean_tree(&root).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
