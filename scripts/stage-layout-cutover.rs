//! Narrow repository release helper, compiled against the reviewed candidate
//! library. No new `mine` command, global installation or Git mutation.
#![forbid(unsafe_code)]

use mine::application::release_service::curation::stage_layout_cutover;
use mine::domain::graph::{CommitId, RepoPath};
use std::path::PathBuf;

fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() != 3 {
        eprintln!(
            "usage: stage-layout-cutover <Root> <exact-source-commit> <.mine/runtime/layout-candidates/name>"
        );
        std::process::exit(2);
    }
    let root = PathBuf::from(&arguments[0]);
    let result = (|| {
        let source = CommitId::new(arguments[1].to_str().ok_or("source commit must be UTF-8")?)
            .map_err(|error| error.to_string())?;
        let target = RepoPath::new(
            arguments[2]
                .to_str()
                .ok_or("candidate locator must be UTF-8")?,
        )
        .map_err(|error| error.to_string())?;
        stage_layout_cutover(&root, &source, &target).map_err(|error| error.to_string())
    })();
    match result {
        Ok(staged) => {
            println!(
                "verified {} writes in {}; source S unchanged",
                staged.writes_verified,
                staged.candidate_root.as_str()
            );
            println!("manifest: {}", staged.manifest.as_str());
            println!(
                "Not a release: stage/commit the candidate, then use existing native Release admission and its independent gates."
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
