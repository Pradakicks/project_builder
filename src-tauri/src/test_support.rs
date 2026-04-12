use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub struct TestTools;

static TEST_TOOLS: OnceLock<TestTools> = OnceLock::new();

pub fn ensure_test_tools() -> &'static TestTools {
    TEST_TOOLS.get_or_init(|| {
        let root = std::env::temp_dir().join(format!(
            "project-builder-test-tools-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create test tools root");

        let real_git = find_real_git();
        write_script(
            &root.join("git"),
            &format!(
                "#!/bin/sh\n\
                 case \"$1\" in\n\
                   commit)\n\
                     case \"$PWD\" in\n\
                       *rollback*)\n\
                         echo \"simulated git commit failure\" >&2\n\
                         exit 1\n\
                         ;;\n\
                     esac\n\
                     ;;\n\
                 esac\n\
                 export GIT_AUTHOR_NAME=\"${{GIT_AUTHOR_NAME:-Test User}}\"\n\
                 export GIT_AUTHOR_EMAIL=\"${{GIT_AUTHOR_EMAIL:-test@example.com}}\"\n\
                 export GIT_COMMITTER_NAME=\"${{GIT_COMMITTER_NAME:-Test User}}\"\n\
                 export GIT_COMMITTER_EMAIL=\"${{GIT_COMMITTER_EMAIL:-test@example.com}}\"\n\
                 exec '{}' \"$@\"\n",
                real_git.display()
            ),
        );

        write_script(
            &root.join("codex"),
            "#!/bin/sh\n\
             printf '%s\\n' \"fake codex run\" >> \"$PWD/generated-from-codex.txt\"\n\
             exit 0\n",
        );

        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![root.clone()];
        paths.extend(std::env::split_paths(&path));
        let new_path = std::env::join_paths(paths).expect("join PATH for test helpers");
        std::env::set_var("PATH", new_path);

        TestTools
    })
}

fn find_real_git() -> PathBuf {
    let output = std::process::Command::new("which")
        .arg("git")
        .output()
        .expect("locate git");
    assert!(output.status.success(), "git must be available for tests");
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(!path.is_empty(), "git path should not be empty");
    PathBuf::from(path)
}

fn write_script(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write test helper script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).expect("stat helper script").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod helper script");
    }
}
