use std::fs;
use std::path::{Path, PathBuf};

const AUTH_CRATES: &[&str] = &[
    "auth",
    "auth-azure",
    "auth-aws",
    "auth-google",
    "auth-oauth",
];
const OPERATION_CRATES: &[&str] = &[
    "operation-audio-transcription",
    "operation-chat-completions",
    "operation-messages",
    "operation-ocr",
    "operation-realtime",
    "operation-responses",
];

fn workspace_root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .canonicalize()
        .expect("workspace root should resolve")
}

fn manifest(crate_name: &str) -> String {
    fs::read_to_string(
        workspace_root()
            .join("crates")
            .join(crate_name)
            .join("Cargo.toml"),
    )
    .expect("crate manifest should be readable")
}

fn litellm_dependencies(manifest: &str) -> impl Iterator<Item = &str> {
    let mut dependencies = false;
    manifest.lines().filter_map(move |line| {
        let line = line.trim();
        if line.starts_with('[') {
            dependencies = matches!(
                line,
                "[dependencies]" | "[dev-dependencies]" | "[build-dependencies]"
            );
            return None;
        }
        let (name, _) = dependencies.then(|| line.split_once('='))??;
        let name = name.trim().trim_matches('"').trim_end_matches(".workspace");
        name.starts_with("litellm-").then_some(name)
    })
}

#[test]
fn auth_crates_are_operation_independent() {
    for crate_name in AUTH_CRATES {
        assert!(
            !manifest(crate_name).contains("litellm-operation"),
            "auth crate {crate_name} must not depend on operation crates"
        );
    }
}

#[test]
fn operation_crates_depend_only_on_shared_foundations() {
    for crate_name in OPERATION_CRATES {
        let manifest = manifest(crate_name);
        assert!(
            manifest.contains("litellm-operation.workspace = true"),
            "semantic operation crate {crate_name} must depend on litellm-operation"
        );
        for dependency in litellm_dependencies(&manifest) {
            assert!(
                matches!(
                    dependency,
                    "litellm-operation" | "litellm-transport" | "litellm-lifecycle"
                ) || dependency.starts_with("litellm-auth"),
                "operation crate {crate_name} must not depend on {dependency}"
            );
        }
    }
}
