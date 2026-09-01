use project_balloon_contracts::JudgeTask;

use crate::sandbox::SandboxError;

#[derive(Clone, Copy)]
pub(super) enum LanguageConfig {
    C,
    Cpp,
    Java,
    Python,
    Go,
    Rust,
}

impl LanguageConfig {
    pub(super) fn for_task(task: &JudgeTask) -> Result<Self, SandboxError> {
        match task.language.as_str() {
            "c" => Ok(Self::C),
            "cpp" => Ok(Self::Cpp),
            "java" => Ok(Self::Java),
            "python" => Ok(Self::Python),
            "go" => Ok(Self::Go),
            "rust" => Ok(Self::Rust),
            other => Err(SandboxError::UnsupportedLanguage(other.to_owned())),
        }
    }

    pub(super) const fn source_filename(self) -> &'static str {
        match self {
            Self::C => "main.c",
            Self::Cpp => "main.cpp",
            Self::Java => "Main.java",
            Self::Python => "main.py",
            Self::Go => "main.go",
            Self::Rust => "main.rs",
        }
    }

    pub(super) fn compile_command(self) -> Vec<String> {
        match self {
            Self::C | Self::Cpp => {
                let (compiler, standard) =
                    if matches!(self, Self::C) { ("gcc", "gnu11") } else { ("g++", "gnu++17") };
                vec![
                    compiler.to_owned(),
                    format!("/work/{}", self.source_filename()),
                    format!("-std={standard}"),
                    "-O2".to_owned(),
                    "-pipe".to_owned(),
                    "-o".to_owned(),
                    "/work/program".to_owned(),
                ]
            }
            Self::Java => vec![
                "javac".to_owned(),
                "-encoding".to_owned(),
                "UTF-8".to_owned(),
                "-d".to_owned(),
                "/work".to_owned(),
                "/work/Main.java".to_owned(),
            ],
            Self::Python => vec![
                "python3".to_owned(),
                "-I".to_owned(),
                "-m".to_owned(),
                "py_compile".to_owned(),
                "/work/main.py".to_owned(),
            ],
            // Single-file build: `go build` on a bare main.go needs no
            // go.mod, and the module caches must live on the read-write
            // /work mount because the 64 MiB /tmp tmpfs is too small.
            Self::Go => vec![
                "go".to_owned(),
                "build".to_owned(),
                "-o".to_owned(),
                "/work/program".to_owned(),
                "/work/main.go".to_owned(),
            ],
            // Standard library only: rustc with no cargo registry access.
            Self::Rust => vec![
                "rustc".to_owned(),
                "--edition".to_owned(),
                "2021".to_owned(),
                "-O".to_owned(),
                "-o".to_owned(),
                "/work/program".to_owned(),
                "/work/main.rs".to_owned(),
            ],
        }
    }

    /// Extra environment the compile exec needs. Go must keep its build
    /// caches on the read-write /work mount (the 64 MiB /tmp tmpfs is too
    /// small and the image's HOME is not writable), and must build
    /// serially: the container's 64-pid limit cannot absorb the fan-out of
    /// concurrent compile tools a full stdlib build would otherwise spawn.
    pub(super) fn compile_env(self) -> Vec<String> {
        match self {
            Self::Go => vec![
                "GOCACHE=/work/.gocache".to_owned(),
                "GOPATH=/work/.go".to_owned(),
                "GOMAXPROCS=1".to_owned(),
            ],
            _ => Vec::new(),
        }
    }

    pub(super) fn run_command(self, memory_limit_mb: i32) -> String {
        match self {
            Self::C | Self::Cpp | Self::Go | Self::Rust => "/work/program".to_owned(),
            Self::Java => {
                let heap_mb = (memory_limit_mb / 2).max(16);
                format!("java -Xms16m -Xmx{heap_mb}m -cp /work Main")
            }
            Self::Python => "python3 -I -B /work/main.py".to_owned(),
        }
    }
}
