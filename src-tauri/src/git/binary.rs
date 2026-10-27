//! Binary / image content support for the diff viewer (ALPH-158).
//!
//! Git never hands us renderable bytes for binary files, so the diff viewer
//! needs a small amount of plumbing to be honest about them:
//!
//! * classify by extension (image) / git's `binary` numstat flag (binary),
//! * report old/new blob byte sizes without loading the blob (`cat-file -s`),
//! * fetch a *bounded* preview of image blobs as a data URL.
//!
//! Every blob read is size-checked first and capped; oversized blobs are
//! reported by size only and never loaded (PRODUCT_SPEC.md §6.4).

use crate::error::AppError;
use crate::git::runner::GitRunner;
use crate::state::DiffScope;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use tokio::io::AsyncReadExt;

/// Pseudo revision used to address the working-tree copy of a file. `git show`
/// cannot read the worktree, so the command reads it from disk instead.
pub const WORKTREE_REV: &str = "WORKTREE";

/// Per-side cap for inline image previews. Blobs larger than this are reported
/// by size only and never loaded into memory or sent over IPC. 4 MiB per side
/// (the base64 data URL is ~5.3 MiB at the cap).
pub const IMAGE_PREVIEW_CAP_BYTES: u64 = 4 * 1024 * 1024;

/// Map a path extension to an image MIME type we can preview in an `<img>`.
/// Returns `None` for anything that is not a well-known raster/vector image.
pub fn image_mime(path: &str) -> Option<&'static str> {
    let dot = path.rfind('.')?;
    if dot + 1 >= path.len() {
        return None;
    }
    // Only consider an extension within the final path segment.
    let ext = &path[dot + 1..];
    if ext.contains('/') || ext.contains('\\') {
        return None;
    }
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

pub fn is_image_path(path: &str) -> bool {
    image_mime(path).is_some()
}

/// Resolve a diff scope to the (old, new) revisions used for size/preview reads.
/// The old revision is `None` for scopes that have no old side.
pub fn scope_revs(scope: &DiffScope) -> (Option<String>, String) {
    match scope {
        // `<sha>^` fails for a root commit; callers treat that as "no old side".
        DiffScope::Commit { sha } => (Some(format!("{}^", sha)), sha.clone()),
        DiffScope::Range { from, to } => (Some(from.clone()), to.clone()),
        DiffScope::Working => (Some("HEAD".to_string()), WORKTREE_REV.to_string()),
    }
}

/// Minimal, dependency-free base64 encoder for data URLs.
pub fn base64_encode(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        out.push(CHARSET[(b0 >> 2) as usize] as char);
        out.push(CHARSET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARSET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARSET[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Old/new metadata for a single changed file: sizes plus optional image previews.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffMeta {
    pub old_size: Option<u64>,
    pub new_size: Option<u64>,
    pub old_exists: bool,
    pub new_exists: bool,
    /// `data:<mime>;base64,...` for image files within the cap.
    pub old_image: Option<String>,
    pub new_image: Option<String>,
    pub old_too_large: bool,
    pub new_too_large: bool,
    pub cap_bytes: u64,
}

struct SideMeta {
    exists: bool,
    size: Option<u64>,
    too_large: bool,
    image: Option<String>,
}

/// Resolve a repo-relative path to an on-disk worktree path. Absolute paths,
/// `..` traversal and (checked by callers) symlinks are rejected so a crafted
/// repository cannot make the preview read outside the working tree.
fn worktree_path(repo_path: &Path, path: &str) -> Option<PathBuf> {
    let rel = Path::new(path);
    if rel.as_os_str().is_empty() || rel.is_absolute() {
        return None;
    }
    if rel.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    Some(repo_path.join(rel))
}

/// Blob byte size at `<rev>:<path>` (or the worktree copy), without loading it.
/// `None` means the path does not exist at that revision.
async fn rev_size(runner: &GitRunner, repo_path: &Path, rev: &str, path: &str) -> Option<u64> {
    if rev == WORKTREE_REV {
        let full = worktree_path(repo_path, path)?;
        let meta = tokio::fs::symlink_metadata(&full).await.ok()?;
        // Never follow a tracked symlink out of the working tree.
        if !meta.file_type().is_file() {
            return None;
        }
        return Some(meta.len());
    }
    let spec = format!("{}:{}", rev, path);
    let out = runner
        .run_read(repo_path, &["cat-file", "-s", &spec])
        .await
        .ok()?;
    if !out.success() {
        return None;
    }
    out.stdout_string().trim().parse::<u64>().ok()
}

/// Read at most `IMAGE_PREVIEW_CAP_BYTES + 1` bytes of a worktree file, so a
/// file that grows between the size probe and the read still cannot be loaded
/// unbounded. The extra byte lets the caller detect that the cap was exceeded.
async fn read_worktree_bounded(repo_path: &Path, path: &str) -> Option<Vec<u8>> {
    let full = worktree_path(repo_path, path)?;
    let file = tokio::fs::File::open(full).await.ok()?;
    let mut limited = file.take(IMAGE_PREVIEW_CAP_BYTES + 1);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).await.ok()?;
    Some(bytes)
}

async fn read_side(
    runner: &GitRunner,
    repo_path: &Path,
    rev: &str,
    path: &str,
    want_image: bool,
) -> SideMeta {
    let size = rev_size(runner, repo_path, rev, path).await;
    let Some(size) = size else {
        return SideMeta {
            exists: false,
            size: None,
            too_large: false,
            image: None,
        };
    };

    if !want_image {
        // Binary content is never loaded; `cat-file -s` already gave us the size.
        return SideMeta {
            exists: true,
            size: Some(size),
            too_large: false,
            image: None,
        };
    }

    if size > IMAGE_PREVIEW_CAP_BYTES {
        return SideMeta {
            exists: true,
            size: Some(size),
            too_large: true,
            image: None,
        };
    }

    let bytes = if rev == WORKTREE_REV {
        read_worktree_bounded(repo_path, path).await
    } else {
        let spec = format!("{}:{}", rev, path);
        runner
            .run_read(repo_path, &["show", &spec])
            .await
            .ok()
            .and_then(|o| if o.success() { Some(o.stdout) } else { None })
    };

    // Defensive: enforce the cap even if the size probe was unavailable/stale.
    let image = match bytes {
        Some(b) if (b.len() as u64) <= IMAGE_PREVIEW_CAP_BYTES => {
            let mime = image_mime(path).unwrap_or("application/octet-stream");
            Some(format!("data:{};base64,{}", mime, base64_encode(&b)))
        }
        _ => None,
    };

    SideMeta {
        exists: true,
        size: Some(size),
        too_large: false,
        image,
    }
}

pub async fn get_file_diff_meta(
    runner: &GitRunner,
    repo_path: &Path,
    scope: &DiffScope,
    path: &str,
    old_path: Option<&str>,
) -> Result<FileDiffMeta, AppError> {
    let (old_rev, new_rev) = scope_revs(scope);
    let want_image = is_image_path(path);
    // Renames need the old side resolved from the previous path; otherwise both
    // sides refer to the same file name.
    let old_path = old_path
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or(path);

    let old = match old_rev {
        Some(rev) => read_side(runner, repo_path, &rev, old_path, want_image).await,
        None => SideMeta {
            exists: false,
            size: None,
            too_large: false,
            image: None,
        },
    };
    let new = read_side(runner, repo_path, &new_rev, path, want_image).await;

    Ok(FileDiffMeta {
        old_size: old.size,
        new_size: new.size,
        old_exists: old.exists,
        new_exists: new.exists,
        old_image: old.image,
        new_image: new.image,
        old_too_large: old.too_large,
        new_too_large: new.too_large,
        cap_bytes: IMAGE_PREVIEW_CAP_BYTES,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_mime_recognizes_common_formats() {
        assert_eq!(image_mime("a/b/photo.PNG"), Some("image/png"));
        assert_eq!(image_mime("logo.jpg"), Some("image/jpeg"));
        assert_eq!(image_mime("logo.jpeg"), Some("image/jpeg"));
        assert_eq!(image_mime("anim.gif"), Some("image/gif"));
        assert_eq!(image_mime("next.webp"), Some("image/webp"));
        assert_eq!(image_mime("icon.svg"), Some("image/svg+xml"));
    }

    #[test]
    fn image_mime_rejects_non_images() {
        assert_eq!(image_mime("src/main.rs"), None);
        assert_eq!(image_mime("Makefile"), None);
        assert_eq!(image_mime("archive.tar.gz"), None);
        assert_eq!(image_mime(".gitignore"), None);
        assert_eq!(image_mime("weird.png/extra"), None);
        assert_eq!(image_mime("trailingdot."), None);
    }

    #[test]
    fn base64_encode_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        // PNG signature — exercises high bytes.
        assert_eq!(base64_encode(&[0x89, 0x50, 0x4e, 0x47]), "iVBORw==");
    }

    #[test]
    fn scope_revs_resolves_old_and_new_revisions() {
        let commit = scope_revs(&DiffScope::Commit {
            sha: "abc123".to_string(),
        });
        assert_eq!(commit.0, Some("abc123^".to_string()));
        assert_eq!(commit.1, "abc123");

        let range = scope_revs(&DiffScope::Range {
            from: "marker".to_string(),
            to: "HEAD".to_string(),
        });
        assert_eq!(range.0, Some("marker".to_string()));
        assert_eq!(range.1, "HEAD");

        let working = scope_revs(&DiffScope::Working);
        assert_eq!(working.0, Some("HEAD".to_string()));
        assert_eq!(working.1, WORKTREE_REV);
    }

    #[test]
    fn preview_cap_is_four_mib_per_side() {
        assert_eq!(IMAGE_PREVIEW_CAP_BYTES, 4 * 1024 * 1024);
    }

    #[test]
    fn worktree_path_allows_relative_files_only() {
        let base = Path::new("/repo");
        assert_eq!(
            worktree_path(base, "assets/logo.png"),
            Some(std::path::PathBuf::from("/repo/assets/logo.png"))
        );
        assert_eq!(
            worktree_path(base, "logo.png"),
            Some(std::path::PathBuf::from("/repo/logo.png"))
        );
        assert_eq!(worktree_path(base, ""), None);
        assert_eq!(worktree_path(base, "/etc/passwd"), None);
        assert_eq!(worktree_path(base, "../secret.png"), None);
        assert_eq!(worktree_path(base, "assets/../../secret.png"), None);
    }

    struct TempRepo {
        path: PathBuf,
    }

    impl TempRepo {
        async fn new(runner: &GitRunner) -> Self {
            let path =
                std::env::temp_dir().join(format!("vantage_binary_test_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            for args in [
                vec!["init"],
                vec!["config", "user.name", "Vantage Test"],
                vec!["config", "user.email", "vantage@test.local"],
                vec!["config", "commit.gpgsign", "false"],
            ] {
                runner.run_write(&path, &args, 10).await.unwrap();
            }
            Self { path }
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[tokio::test]
    async fn file_diff_meta_reports_worktree_sizes_and_previews() {
        let runner = GitRunner::new();
        let repo = TempRepo::new(&runner).await;

        let old_bytes = b"\x89PNG\r\n\x1a\nold".to_vec();
        std::fs::write(repo.path.join("logo.png"), &old_bytes).unwrap();
        runner
            .run_write(&repo.path, &["add", "logo.png"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "add logo"], 10)
            .await
            .unwrap();

        // Root commit: `<sha>^` does not resolve, so the old side is reported
        // as absent rather than failing the whole request.
        let sha = runner
            .run_read(&repo.path, &["rev-parse", "HEAD"])
            .await
            .unwrap()
            .stdout_string()
            .trim()
            .to_string();
        let root_meta = get_file_diff_meta(
            &runner,
            &repo.path,
            &DiffScope::Commit { sha },
            "logo.png",
            None,
        )
        .await
        .unwrap();
        assert!(!root_meta.old_exists);
        assert!(root_meta.new_exists);
        assert_eq!(root_meta.old_size, None);
        assert_eq!(root_meta.new_size, Some(old_bytes.len() as u64));
        assert!(root_meta.new_image.is_some());

        let new_bytes = b"\x89PNG\r\n\x1a\nnew-and-longer".to_vec();
        std::fs::write(repo.path.join("logo.png"), &new_bytes).unwrap();

        let meta = get_file_diff_meta(&runner, &repo.path, &DiffScope::Working, "logo.png", None)
            .await
            .unwrap();

        assert_eq!(meta.old_size, Some(old_bytes.len() as u64));
        assert_eq!(meta.new_size, Some(new_bytes.len() as u64));
        assert!(meta.old_exists && meta.new_exists);
        assert!(!meta.old_too_large && !meta.new_too_large);
        assert!(meta
            .old_image
            .as_deref()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert!(meta
            .new_image
            .as_deref()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert_eq!(meta.cap_bytes, IMAGE_PREVIEW_CAP_BYTES);

        // Oversized images are described by size only and never loaded.
        let huge = vec![0u8; (IMAGE_PREVIEW_CAP_BYTES + 1) as usize];
        std::fs::write(repo.path.join("huge.png"), huge).unwrap();
        let huge_meta =
            get_file_diff_meta(&runner, &repo.path, &DiffScope::Working, "huge.png", None)
                .await
                .unwrap();
        assert!(huge_meta.new_exists);
        assert!(huge_meta.new_too_large);
        assert_eq!(huge_meta.new_size, Some(IMAGE_PREVIEW_CAP_BYTES + 1));
        assert!(huge_meta.new_image.is_none());
    }
}
