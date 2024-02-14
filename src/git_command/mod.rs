use anyhow::{Error, Result};
use mockall::{automock, concretize};
use std::process::Command;

pub struct GitCommand {}

#[automock]
pub trait GitCommandTrait {
    #[concretize]
    fn run(&self, args: Vec<&str>) -> Result<String>;
}

impl GitCommandTrait for GitCommand {
    fn run(&self, args: Vec<&str>) -> Result<String> {
        let output = Command::new("git").args(args).output()?;

        if output.status.code().unwrap() != 0 {
            return Err(Error::msg(format!(
                "Git command failed: {}",
                String::from_utf8(output.stderr)?
            )));
        }

        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }
}
