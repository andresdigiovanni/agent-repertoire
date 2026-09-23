use crate::skill::SkillTarget;
use crate::{InvalidArgumentIssues, RepError, Result};
use std::io::{ErrorKind, IsTerminal};

#[derive(Debug)]
pub enum PromptOutcome {
    Selected(Vec<SkillTarget>),
    Aborted,
}

pub fn select_providers() -> Result<PromptOutcome> {
    let interactive = std::io::stdin().is_terminal();
    select(interactive)
}

fn select(interactive: bool) -> Result<PromptOutcome> {
    if !interactive {
        return Err(RepError::InvalidArguments(InvalidArgumentIssues(vec![
            "no provider selected; pass --target <claude|opencode|codex> or --all".to_string(),
        ])));
    }
    let labels: Vec<&str> = SkillTarget::all().iter().map(|t| t.as_str()).collect();
    eprintln!("↑/↓: move · space: toggle · enter: accept · ctrl-c: abort");
    loop {
        let indices = match dialoguer::MultiSelect::new()
            .with_prompt("select providers to install")
            .items(&labels)
            .report(false)
            .interact()
        {
            Ok(indices) => indices,
            Err(dialoguer::Error::IO(io_err))
                if matches!(
                    io_err.kind(),
                    ErrorKind::Interrupted | ErrorKind::UnexpectedEof
                ) =>
            {
                return Ok(PromptOutcome::Aborted);
            }
            Err(e) => {
                return Err(RepError::Spawn(format!(
                    "cannot read provider selection: {}",
                    e
                )));
            }
        };
        if !indices.is_empty() {
            return Ok(PromptOutcome::Selected(
                SkillTarget::all()
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| indices.contains(i))
                    .map(|(_, t)| *t)
                    .collect(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_interactive_errors_with_guidance() {
        let err = select(false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("--target"));
        assert!(msg.contains("--all"));
        assert!(msg.contains("claude"));
        assert!(msg.contains("opencode"));
        assert!(msg.contains("codex"));
    }

    #[test]
    fn non_interactive_error_is_invalid_arguments() {
        assert!(matches!(
            select(false),
            Err(RepError::InvalidArguments(_))
        ));
    }
}
