use project_balloon_contracts::JudgeTask;

use super::SandboxError;

#[derive(Clone, Copy)]
pub(super) enum LanguageConfig {
    C,
    Cpp,
    Java,
    Python,
}

impl LanguageConfig {
    pub(super) fn for_task(task: &JudgeTask) -> Result<Self, SandboxError> {
        match task.language.as_str() {
            "c" => Ok(Self::C),
            "cpp" => Ok(Self::Cpp),
            "java" => Ok(Self::Java),
            "python" => Ok(Self::Python),
            other => Err(SandboxError::UnsupportedLanguage(other.to_owned())),
        }
    }

    pub(super) const fn source_filename(self) -> &'static str {
        match self {
            Self::C => "main.c",
            Self::Cpp => "main.cpp",
            Self::Java => "Main.java",
            Self::Python => "main.py",
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
        }
    }

    pub(super) fn run_command(self, memory_limit_mb: i32) -> String {
        match self {
            Self::C | Self::Cpp => "/work/program".to_owned(),
            Self::Java => {
                let heap_mb = (memory_limit_mb / 2).max(16);
                format!("java -Xms16m -Xmx{heap_mb}m -cp /work Main")
            }
            Self::Python => "python3 -I -B /work/main.py".to_owned(),
        }
    }
}
